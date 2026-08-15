#!/usr/bin/env python3
"""Record a complete local Phase 3 acceptance report as SHA-bound evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REPORT = ROOT / "target" / "phase3-local-verification.json"
EVIDENCE_DIR = ROOT / "docs" / "verification"

REQUIRED_CHECKS = {
    "Pinned clean acceptance candidate",
    "Experiment 0179 checkpoint history consistency",
    "Experiment 0179 directory headroom",
    "Experiment 0179 prune order",
    "Experiment 0179 wiring",
    "Independent restart-metadata compaction model",
    "Locked dependency graph",
    "Rust formatting",
    "Phase 3 Clippy",
    "Phase 3 Rust tests",
    "Independent Phase 3 models",
    "Pinned EXP-0002 invalid corpus",
    "Immutable successor invalid recipes",
    "EXP-0003 candidate corpus scaffold",
    "EXP-0003 normative amendment map",
    "Workspace Clippy",
    "Workspace Rust tests",
    "Rust documentation tests",
    "Reqwest conditional transport",
    "Async HTTP targeted lookup",
    "Async HTTP full validation",
    "Async HTTP linked history",
    "Async HTTP recovery",
    "Versioned S3 source adapter",
    "Deletion policy trace",
    "Deletion policy trace matrix",
    "Policy reward decomposition",
    "Candidate deletion policy vectors",
    "Source deletion policy I/O",
    "Deletion parent reward catalog",
    "Rust 1.85 workspace check",
    "Rust 1.85 HTTP feature check",
    "Portability i686-unknown-linux-gnu",
    "Portability powerpc64-unknown-linux-gnu",
    "Fuzz formatting",
    "Compile fuzz targets",
    "List fuzz targets",
}


class RecordError(RuntimeError):
    pass


def git_output(*args: str) -> str:
    try:
        completed = subprocess.run(
            ["git", *args],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except OSError as exc:
        raise RecordError(f"git is unavailable: {exc}") from exc
    if completed.returncode != 0:
        raise RecordError(
            completed.stderr.strip() or f"git {' '.join(args)} failed"
        )
    return completed.stdout.strip()


def load_report(path: Path) -> tuple[bytes, dict]:
    try:
        raw = path.read_bytes()
    except OSError as exc:
        raise RecordError(f"cannot read local acceptance report: {exc}") from exc
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise RecordError(f"local acceptance report is not valid JSON: {exc}") from exc
    if not isinstance(payload, dict):
        raise RecordError("local acceptance report must be a JSON object")
    return raw, payload


def validate_report(report: dict) -> tuple[str, list[dict], list[str]]:
    if report.get("schema") != "ucof-phase3-local-verification-v1":
        raise RecordError("unexpected local verification schema")
    if report.get("mode") != "acceptance" or report.get("ok") is not True:
        raise RecordError("only a successful acceptance-mode report can be recorded")
    if report.get("failure") not in (None, ""):
        raise RecordError("successful report unexpectedly contains a failure")
    if report.get("skipped") not in ([], None):
        raise RecordError("acceptance report contains skipped checks")
    if report.get("dirty_worktree") is not False:
        raise RecordError("acceptance report was not produced from a clean worktree")

    report_sha = report.get("git_sha")
    acceptance_sha = report.get("acceptance_sha")
    if not isinstance(report_sha, str) or len(report_sha) != 40:
        raise RecordError("acceptance report does not contain a full Git SHA")
    if acceptance_sha != report_sha:
        raise RecordError(
            "acceptance report start/end SHA pin does not match final Git SHA"
        )

    for tool in ("python", "rustc", "cargo"):
        value = report.get(tool)
        if not isinstance(value, str) or not value:
            raise RecordError(f"acceptance report is missing {tool} version evidence")

    checks = report.get("checks")
    if not isinstance(checks, list) or not checks:
        raise RecordError("acceptance report contains no checks")

    normalized: list[dict] = []
    names: set[str] = set()
    listed_fuzz_targets: list[str] | None = None
    smoked_fuzz_targets: set[str] = set()
    for index, check in enumerate(checks):
        if not isinstance(check, dict):
            raise RecordError(f"check {index} is not an object")
        name = check.get("name")
        status = check.get("status")
        if not isinstance(name, str) or not name:
            raise RecordError(f"check {index} is missing a name")
        if status != "pass":
            raise RecordError(f"check did not pass: {name}")
        if name in names:
            raise RecordError(f"duplicate check name in report: {name}")
        names.add(name)

        if name == "List fuzz targets":
            output = check.get("output")
            if not isinstance(output, str):
                raise RecordError("List fuzz targets check did not capture output")
            listed_fuzz_targets = [
                line.strip() for line in output.splitlines() if line.strip()
            ]
        elif name.startswith("Fuzz smoke "):
            target = name.removeprefix("Fuzz smoke ").strip()
            if not target:
                raise RecordError("empty fuzz smoke target name")
            smoked_fuzz_targets.add(target)

        normalized.append(
            {
                "name": name,
                "status": status,
                "seconds": check.get("seconds"),
                "command": check.get("command"),
                "detail": check.get("detail"),
                "output": check.get("output") if name == "List fuzz targets" else None,
            }
        )

    missing = sorted(REQUIRED_CHECKS - names)
    if missing:
        raise RecordError("acceptance report is missing checks: " + ", ".join(missing))
    if not listed_fuzz_targets:
        raise RecordError("acceptance report listed no fuzz targets")
    if len(set(listed_fuzz_targets)) != len(listed_fuzz_targets):
        raise RecordError("cargo fuzz list returned duplicate target names")

    listed = set(listed_fuzz_targets)
    missing_smoke = sorted(listed - smoked_fuzz_targets)
    unexpected_smoke = sorted(smoked_fuzz_targets - listed)
    if missing_smoke or unexpected_smoke:
        detail = []
        if missing_smoke:
            detail.append("missing smoke: " + ", ".join(missing_smoke))
        if unexpected_smoke:
            detail.append("unexpected smoke: " + ", ".join(unexpected_smoke))
        raise RecordError("fuzz target coverage mismatch (" + "; ".join(detail) + ")")

    return report_sha, normalized, listed_fuzz_targets


def verify_current_checkout(report_sha: str) -> tuple[str, str]:
    current = git_output("rev-parse", "HEAD")
    if current != report_sha:
        raise RecordError(
            f"report SHA {report_sha} does not match current HEAD {current}"
        )
    if git_output("status", "--porcelain"):
        raise RecordError("recording requires the same clean checkout used for acceptance")
    branch = git_output("branch", "--show-current")
    if not branch:
        raise RecordError("recording requires a named Git branch")
    return current, branch


def build_record(
    *,
    report: dict,
    report_raw: bytes,
    accepted_sha: str,
    branch: str,
    checks: list[dict],
    fuzz_targets: list[str],
) -> dict:
    return {
        "schema": "ucof-phase3-local-acceptance-v2",
        "accepted_sha": accepted_sha,
        "branch": branch,
        "source_report_sha256": hashlib.sha256(report_raw).hexdigest(),
        "verification_started_utc": report.get("started_utc"),
        "verification_completed_utc": report.get("completed_utc"),
        "offline": report.get("offline"),
        "tool_versions": {
            "python": report.get("python"),
            "rustc": report.get("rustc"),
            "cargo": report.get("cargo"),
        },
        "fuzz_targets": fuzz_targets,
        "checks": checks,
        "non_claims": [
            "This is deterministic repository-local mechanism evidence, not physical power-loss qualification.",
            "It does not qualify network filesystems or all local filesystems.",
            "It does not provide deletion/replay anti-rollback without an external trusted floor.",
            "It does not close the final same-UID identity-check-to-unlink race.",
            "It does not select EXP-0003 D1-D7 or allocate an experimental epoch.",
        ],
    }


def write_record(record: dict, accepted_sha: str) -> Path:
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)
    path = EVIDENCE_DIR / f"phase3-local-acceptance-{accepted_sha}.json"
    encoded = json.dumps(record, indent=2, sort_keys=True) + "\n"
    if path.exists():
        try:
            existing = path.read_text()
        except OSError as exc:
            raise RecordError(f"cannot read existing evidence record: {exc}") from exc
        if existing != encoded:
            raise RecordError(
                f"evidence record already exists with different content: {path}"
            )
        return path
    try:
        path.write_text(encoded)
    except OSError as exc:
        raise RecordError(f"cannot write evidence record: {exc}") from exc
    return path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    report_path = args.report if args.report.is_absolute() else ROOT / args.report
    try:
        raw, report = load_report(report_path)
        accepted_sha, checks, fuzz_targets = validate_report(report)
        _, branch = verify_current_checkout(accepted_sha)
        record = build_record(
            report=report,
            report_raw=raw,
            accepted_sha=accepted_sha,
            branch=branch,
            checks=checks,
            fuzz_targets=fuzz_targets,
        )
        path = write_record(record, accepted_sha)
    except RecordError as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1

    print(f"Phase 3 local acceptance record: {path}")
    print(f"accepted_sha={accepted_sha}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
