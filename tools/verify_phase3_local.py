#!/usr/bin/env python3
"""Run Phase 3 verification locally without GitHub Actions.

The default mode is the fast Phase 3 gate. ``--acceptance`` expands this to
workspace, MSRV, portability, HTTP adapter, policy/vector, and fuzz-smoke
checks. The script never installs toolchains or packages: a complete acceptance
run fails if a required local toolchain, target, or cargo-fuzz is absent.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REPORT = ROOT / "target" / "phase3-local-verification.json"


class VerificationFailure(RuntimeError):
    pass


def _capture_optional(command: list[str]) -> str | None:
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    except OSError:
        return None
    if result.returncode != 0:
        return None
    return result.stdout.strip()


class Runner:
    def __init__(self, report_path: Path, offline: bool) -> None:
        self.report_path = report_path
        self.env = os.environ.copy()
        self.env["PYTHONDONTWRITEBYTECODE"] = "1"
        if offline:
            self.env["CARGO_NET_OFFLINE"] = "true"
        self.records: list[dict] = []
        self.started = datetime.now(timezone.utc).isoformat()
        self.ok = False
        self.failure: str | None = None
        self.acceptance_sha: str | None = None

    def record_static(self, name: str, detail: str) -> None:
        self.records.append(
            {
                "name": name,
                "command": None,
                "status": "pass",
                "seconds": 0.0,
                "detail": detail,
            }
        )
        print(f"PASS {name}: {detail}")

    def run(self, name: str, command: list[str], *, capture: bool = False) -> str:
        print(f"\n== {name} ==\n$ {shlex.join(command)}")
        started = time.monotonic()
        try:
            completed = subprocess.run(
                command,
                cwd=ROOT,
                env=self.env,
                text=True,
                stdout=subprocess.PIPE if capture else None,
                stderr=subprocess.STDOUT if capture else None,
                check=False,
            )
        except OSError as exc:
            elapsed = time.monotonic() - started
            self.records.append(
                {
                    "name": name,
                    "command": command,
                    "status": "fail",
                    "seconds": round(elapsed, 3),
                    "detail": str(exc),
                }
            )
            raise VerificationFailure(f"{name} could not start: {exc}") from exc

        elapsed = time.monotonic() - started
        record = {
            "name": name,
            "command": command,
            "status": "pass" if completed.returncode == 0 else "fail",
            "returncode": completed.returncode,
            "seconds": round(elapsed, 3),
        }
        if capture:
            record["output"] = completed.stdout
        self.records.append(record)
        if completed.returncode != 0:
            if capture and completed.stdout:
                print(completed.stdout, end="")
            raise VerificationFailure(
                f"{name} failed with exit status {completed.returncode}"
            )
        if capture and completed.stdout:
            print(completed.stdout, end="")
        return completed.stdout or ""

    def write_report(self, mode: str, offline: bool, skips: list[str]) -> None:
        self.report_path.parent.mkdir(parents=True, exist_ok=True)
        git_sha = _capture_optional(["git", "rev-parse", "HEAD"])
        dirty_text = _capture_optional(["git", "status", "--porcelain"])
        payload = {
            "schema": "ucof-phase3-local-verification-v1",
            "mode": mode,
            "offline": offline,
            "started_utc": self.started,
            "completed_utc": datetime.now(timezone.utc).isoformat(),
            "ok": self.ok,
            "failure": self.failure,
            "git_sha": git_sha,
            "acceptance_sha": self.acceptance_sha,
            "git_branch": _capture_optional(["git", "branch", "--show-current"]),
            "dirty_worktree": None if dirty_text is None else bool(dirty_text),
            "python": sys.version.split()[0],
            "rustc": _capture_optional(["rustc", "--version"]),
            "cargo": _capture_optional(["cargo", "--version"]),
            "skipped": skips,
            "checks": self.records,
        }
        self.report_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
        print(f"\nLocal verification report: {self.report_path}")


def require_file(path: Path) -> None:
    if not path.is_file():
        raise VerificationFailure(f"required file is missing: {path.relative_to(ROOT)}")


def verify_acceptance_candidate(runner: Runner) -> None:
    sha = _capture_optional(["git", "rev-parse", "HEAD"])
    if not sha:
        raise VerificationFailure("acceptance requires a resolvable Git HEAD")
    dirty = _capture_optional(["git", "status", "--porcelain"])
    if dirty is None:
        raise VerificationFailure("acceptance requires Git worktree status")
    if dirty:
        raise VerificationFailure("acceptance requires a clean worktree")
    runner.acceptance_sha = sha
    runner.record_static("Pinned clean acceptance candidate", sha)


def verify_phase3_tooling_host() -> None:
    required_apis = ("geteuid", "statvfs")
    missing = [name for name in required_apis if not hasattr(os, name)]
    if os.name != "posix" or missing:
        detail = "" if not missing else f"; missing os APIs: {', '.join(missing)}"
        raise VerificationFailure(
            "Phase 3 tool checks require a POSIX host with os.geteuid and os.statvfs"
            f" (os.name={os.name!r}{detail})"
        )


def verify_acceptance_candidate_unchanged(runner: Runner) -> None:
    if runner.acceptance_sha is None:
        return
    current = _capture_optional(["git", "rev-parse", "HEAD"])
    if current != runner.acceptance_sha:
        raise VerificationFailure(
            "acceptance candidate HEAD changed during verification"
        )
    dirty = _capture_optional(["git", "status", "--porcelain"])
    if dirty is None or dirty:
        raise VerificationFailure(
            "acceptance worktree changed during verification"
        )


def verify_wiring(runner: Runner) -> None:
    parent = (
        ROOT
        / "crates/ucof-experiments/src/immutable_successor/bounded_end_to_end_candidate.rs"
    )
    require_file(parent)
    text = parent.read_text()
    required = [
        "restart_metadata_compaction.rs",
        "compacted_restart_classification.rs",
        "compacted_source_bound_restart.rs",
        "compacted_private_lifecycle_quota.rs",
        "restart_metadata_compaction_tests.rs",
        "restart_metadata_compaction_retry_tests.rs",
        "restart_metadata_compaction_graph_tests.rs",
        "restart_metadata_compaction_checkpoint_consistency_tests.rs",
        "restart_metadata_compaction_property_tests.rs",
        "restart_metadata_compaction_accounting_tests.rs",
        "compacted_restart_retry_tests.rs",
        "compacted_private_lifecycle_quota_tests.rs",
    ]
    missing = [
        name
        for name in required
        if f'include!("bounded_end_to_end_candidate/{name}")' not in text
    ]
    if missing:
        raise VerificationFailure(
            f"Experiment 0179 files are not wired: {', '.join(missing)}"
        )

    base = parent.parent / "bounded_end_to_end_candidate"
    for name in required:
        require_file(base / name)

    stale_checks = {
        "restart_metadata_compaction_tests.rs": ["fixture.publication"],
        "compacted_private_lifecycle_quota_tests.rs": ["fixture.publication"],
        "compacted_restart_classification.rs": [
            "manifest.object_count",
            "manifest.stage_bytes",
            "manifest.lease_first",
            "EncryptedStageRestartScanStats",
        ],
    }
    for name, forbidden in stale_checks.items():
        source = (base / name).read_text()
        present = [token for token in forbidden if token in source]
        if present:
            raise VerificationFailure(
                f"stale 0179 API references in {name}: {', '.join(present)}"
            )

    required_tokens = {
        "restart_metadata_compaction.rs": [
            'return Err("compacted nonce filename generation".into());',
            'return Err("compaction retirement context".into());',
            'return Err("compaction source-set context".into());',
            "nonce_record.generation != manifest.generation",
            "for checkpoint in checkpoints.iter().copied()",
            "validate_compacted_directory_entry_count",
            "ensure_compacted_nonce_commit_directory_headroom",
            "saw_unrecognized_entry",
            "AfterSourceSetPruneBeforeRetirementPrune",
            "AfterPreparedRetirementPruneBeforeTerminalPrune",
        ],
        "compacted_source_bound_restart.rs": [
            'return Err("compacted restart manifest/nonce context".into());',
        ],
        "restart_metadata_compaction_graph_tests.rs": [
            "compacted_scan_rejects_authenticated_record_replayed_under_wrong_generation_name",
            "compaction_rejects_authenticated_retirement_from_foreign_journal_context",
            "compaction_rejects_authenticated_source_set_from_foreign_journal_context",
        ],
        "restart_metadata_compaction_checkpoint_consistency_tests.rs": [
            "newer_checkpoint_cannot_mask_older_checkpoint_below_surviving_record",
            "newer_checkpoint_cannot_mask_older_same_generation_record_mismatch",
        ],
        "restart_metadata_compaction_retry_tests.rs": [
            "retry_after_terminal_source_set_prune_before_retirement_prune_completes",
            "retry_after_prepared_retirement_prune_keeps_terminal_authority",
        ],
        "restart_metadata_compaction_accounting_tests.rs": [
            "authenticated_checkpoint_gets_exactly_one_transient_directory_entry_at_ceiling",
            "unrelated_extra_directory_entry_does_not_receive_checkpoint_headroom",
            "compacted_nonce_commit_reserves_one_directory_slot_for_next_checkpoint",
            "checkpoint_does_not_lend_transient_headroom_to_unknown_entry",
        ],
        "compacted_restart_retry_tests.rs": [
            "compacted_restart_survives_pruned_burn_then_publishes_retires_and_reclaims",
            "compacted_destination_exists_burn_can_be_pruned_and_retried",
        ],
        "compacted_private_lifecycle_quota_tests.rs": [
            "checkpointed_source_bound_restart_quota_preserves_pre_side_effect_rejection",
            "checkpointed_publication_quota_rejects_before_nonce_or_backend_side_effects",
        ],
    }
    for name, tokens in required_tokens.items():
        source = (base / name).read_text()
        missing_tokens = [token for token in tokens if token not in source]
        if missing_tokens:
            raise VerificationFailure(
                f"critical 0179 fail-closed coverage missing in {name}: "
                + ", ".join(missing_tokens)
            )

    compaction_source = (base / "restart_metadata_compaction.rs").read_text()
    try:
        compact_body = compaction_source.split("fn compact_restart_metadata(", 1)[1]
    except IndexError as exc:
        raise VerificationFailure(
            "Experiment 0179 compaction executor is missing"
        ) from exc

    order_tokens = [
        "for (name, record) in nonce_records {",
        "for (name, record) in &metadata.source_sets {",
        "record.state == EncryptedRetirementState::Prepared",
        "record.state == EncryptedRetirementState::Terminal",
        "for (name, old_checkpoint) in old_checkpoints {",
    ]
    positions = [compact_body.find(token) for token in order_tokens]
    if any(position < 0 for position in positions) or positions != sorted(positions):
        raise VerificationFailure(
            "Experiment 0179 destructive order must remain "
            "nonce -> source-set -> Prepared -> Terminal -> old checkpoint"
        )

    runner.record_static(
        "Experiment 0179 checkpoint history consistency",
        "every authenticated checkpoint is checked against all surviving "
        "nonce records at/below it",
    )
    runner.record_static(
        "Experiment 0179 directory headroom",
        "one authenticated checkpoint transient entry is allowed and ordinary "
        "commits reserve the next checkpoint slot",
    )
    runner.record_static(
        "Experiment 0179 prune order",
        "nonce -> terminal source-set -> Prepared retirement -> "
        "Terminal retirement -> old checkpoint",
    )

    obsolete = (
        ROOT
        / ".github/workflows/one-shot-accept-restart-metadata-compaction.yml"
    )
    if obsolete.exists():
        raise VerificationFailure(
            "obsolete Actions-based 0179 acceptance coordinator is still present"
        )
    runner.record_static(
        "Experiment 0179 wiring",
        f"{len(required)} implementation/test files wired",
    )


def run_model(runner: Runner, campaigns: int, steps: int) -> None:
    model = ROOT / "tools/verify_restart_metadata_compaction_model.py"
    require_file(model)
    runner.run(
        "Independent restart-metadata compaction model",
        [
            sys.executable,
            str(model),
            "--campaigns",
            str(campaigns),
            "--steps",
            str(steps),
        ],
    )


def run_quick(runner: Runner) -> None:
    runner.run(
        "Phase 3 Python tool self-tests",
        [
            sys.executable,
            "-m",
            "unittest",
            "tools.test_verify_phase3_local",
            "tools.test_record_phase3_local_acceptance",
            "tools.test_check_phase3_storage_headroom",
            "tools.test_phase3_preflight_tools",
            "tools.test_qualify_phase3_filesystem",
            "tools.test_qualify_phase3_key_material",
        ],
    )
    runner.run(
        "Locked dependency graph",
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
    )
    runner.run("Rust formatting", ["cargo", "fmt", "--all", "--", "--check"])
    runner.run(
        "Phase 3 Clippy",
        [
            "cargo",
            "clippy",
            "--locked",
            "-p",
            "ucof-experiments",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )
    runner.run(
        "Phase 3 Rust tests",
        [
            "cargo",
            "test",
            "--locked",
            "-p",
            "ucof-experiments",
            "--all-targets",
        ],
    )
    runner.run(
        "Independent Phase 3 models",
        [sys.executable, "tools/validate_phase3_models.py"],
    )
    runner.run(
        "Pinned EXP-0002 invalid corpus",
        [
            sys.executable,
            "tools/exp0002_invalid_vectors.py",
            "--verify-vectors",
            "tests/vectors/exp-0002-invalid",
        ],
    )
    runner.run(
        "Immutable successor invalid recipes",
        [sys.executable, "tools/verify_exp0002_immutable_invalid_recipes.py"],
    )
    runner.run(
        "EXP-0003 candidate corpus scaffold",
        [sys.executable, "tools/plan_exp0003_candidate_corpus.py", "--verify"],
    )
    runner.run(
        "EXP-0003 normative amendment map",
        [sys.executable, "tools/plan_exp0003_normative_amendment.py", "--verify"],
    )


def run_http_and_policy(runner: Runner) -> None:
    for label, test_filter in [
        ("Reqwest conditional transport", "conditional_reqwest"),
        ("Async HTTP targeted lookup", "conditional_async_source_lookup"),
        ("Async HTTP full validation", "conditional_async_source_full"),
        ("Async HTTP linked history", "conditional_async_source_history"),
        ("Async HTTP recovery", "conditional_async_source_recovery"),
        ("Versioned S3 source adapter", "s3_versioned_reqwest"),
    ]:
        runner.run(
            label,
            [
                "cargo",
                "test",
                "--locked",
                "-p",
                "ucof-experiments",
                "--features",
                "http-reqwest",
                test_filter,
            ],
        )

    runner.run(
        "Deletion policy trace",
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "ucof-experiments",
            "--example",
            "exp0003_delete_policy_trace",
        ],
    )
    runner.run(
        "Deletion policy trace matrix",
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "ucof-experiments",
            "--example",
            "exp0003_delete_policy_trace_matrix",
        ],
    )
    runner.run(
        "Policy reward decomposition",
        [sys.executable, "tools/verify_exp0003_policy_reward_decomposition.py"],
    )
    runner.run(
        "Candidate deletion policy vectors",
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "ucof-experiments",
            "--example",
            "exp0003_delete_policy_candidate_vectors",
            "--",
            "--verify",
            "tests/vectors/exp-0003-candidate-delete-policy/manifest.txt",
        ],
    )
    runner.run(
        "Source deletion policy I/O",
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "ucof-experiments",
            "--example",
            "exp0003_source_delete_policy_io",
        ],
    )
    runner.run(
        "Deletion parent reward catalog",
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "ucof-experiments",
            "--example",
            "exp0003_delete_parent_reward_catalog",
            "--",
            "--verify",
            "tests/vectors/exp-0003-delete-reward-catalog/manifest.csv",
        ],
    )


def require_command(command: list[str], label: str) -> None:
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    except OSError as exc:
        raise VerificationFailure(f"{label} is not installed") from exc
    if result.returncode != 0:
        raise VerificationFailure(f"{label} is not installed or not usable")


def run_msrv_and_portability(runner: Runner) -> None:
    require_command(
        ["cargo", "+1.85.0", "--version"],
        "Rust 1.85.0 toolchain",
    )
    runner.run(
        "Rust 1.85 workspace check",
        ["cargo", "+1.85.0", "check", "--locked", "--workspace", "--all-targets"],
    )
    runner.run(
        "Rust 1.85 HTTP feature check",
        [
            "cargo",
            "+1.85.0",
            "check",
            "--locked",
            "-p",
            "ucof-experiments",
            "--all-targets",
            "--features",
            "http-reqwest",
        ],
    )

    installed = _capture_optional(["rustup", "target", "list", "--installed"])
    if installed is None:
        raise VerificationFailure("rustup target inventory is unavailable")
    installed_targets = set(installed.splitlines())
    for target in ("i686-unknown-linux-gnu", "powerpc64-unknown-linux-gnu"):
        if target not in installed_targets:
            raise VerificationFailure(
                f"required portability target is not installed: {target}"
            )
        runner.run(
            f"Portability {target}",
            [
                "cargo",
                "check",
                "--locked",
                "--workspace",
                "--all-targets",
                "--target",
                target,
            ],
        )


def run_fuzz_smoke(runner: Runner) -> None:
    require_command(["cargo", "+nightly", "--version"], "nightly Rust toolchain")
    require_command(["cargo", "fuzz", "--help"], "cargo-fuzz")
    runner.run(
        "Fuzz formatting",
        [
            "cargo",
            "+nightly",
            "fmt",
            "--manifest-path",
            "fuzz/Cargo.toml",
            "--",
            "--check",
        ],
    )
    runner.run("Compile fuzz targets", ["cargo", "+nightly", "fuzz", "build"])
    targets_text = runner.run(
        "List fuzz targets",
        ["cargo", "+nightly", "fuzz", "list"],
        capture=True,
    )
    targets = [line.strip() for line in targets_text.splitlines() if line.strip()]
    if not targets:
        raise VerificationFailure("cargo fuzz list returned no targets")
    for target in targets:
        runner.run(
            f"Fuzz smoke {target}",
            [
                "cargo",
                "+nightly",
                "fuzz",
                "run",
                target,
                "--",
                "-runs=256",
                "-max_len=65536",
            ],
        )


def run_acceptance(runner: Runner, skip_fuzz: bool, skips: list[str]) -> None:
    runner.run(
        "Workspace Clippy",
        [
            "cargo",
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )
    runner.run(
        "Workspace Rust tests",
        ["cargo", "test", "--locked", "--workspace", "--all-targets"],
    )
    runner.run(
        "Rust documentation tests",
        ["cargo", "test", "--locked", "--workspace", "--doc"],
    )
    run_http_and_policy(runner)
    run_msrv_and_portability(runner)
    if skip_fuzz:
        skips.append("fuzz-smoke")
        print(
            "SKIP fuzz smoke (--skip-fuzz); report is not a complete acceptance run"
        )
    else:
        run_fuzz_smoke(runner)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--acceptance",
        action="store_true",
        help="run the complete local acceptance surface",
    )
    mode.add_argument(
        "--model-only",
        action="store_true",
        help="run only wiring plus the independent 0179 model",
    )
    parser.add_argument(
        "--offline",
        action="store_true",
        help="set CARGO_NET_OFFLINE=true for Cargo commands",
    )
    parser.add_argument(
        "--skip-fuzz",
        action="store_true",
        help="development escape hatch; makes --acceptance incomplete",
    )
    parser.add_argument(
        "--campaigns",
        type=int,
        default=128,
        help="independent-model randomized campaigns",
    )
    parser.add_argument(
        "--steps",
        type=int,
        default=128,
        help="steps per independent-model campaign",
    )
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    args = parser.parse_args()
    if args.campaigns <= 0 or args.steps <= 0:
        parser.error("campaigns and steps must be positive")
    if args.skip_fuzz and not args.acceptance:
        parser.error("--skip-fuzz is only meaningful with --acceptance")
    return args


def main() -> int:
    args = parse_args()
    mode = "model-only" if args.model_only else "acceptance" if args.acceptance else "phase3"
    report_path = args.report if args.report.is_absolute() else ROOT / args.report
    runner = Runner(report_path, args.offline)
    skips: list[str] = []
    try:
        if args.acceptance:
            verify_acceptance_candidate(runner)
        if not args.model_only:
            verify_phase3_tooling_host()
        verify_wiring(runner)
        run_model(runner, args.campaigns, args.steps)
        if not args.model_only:
            run_quick(runner)
        if args.acceptance:
            run_acceptance(runner, args.skip_fuzz, skips)
            verify_acceptance_candidate_unchanged(runner)
        runner.ok = not skips
        if skips:
            runner.failure = "acceptance run intentionally skipped required checks"
    except (VerificationFailure, OSError) as exc:
        runner.failure = str(exc)
        print(f"\nFAIL: {exc}", file=sys.stderr)
    finally:
        runner.write_report(mode, args.offline, skips)

    if runner.ok:
        print(f"\nPhase 3 local verification: PASS ({mode})")
        return 0
    print(f"\nPhase 3 local verification: FAIL ({mode})", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
