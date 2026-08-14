#!/usr/bin/env python3
"""Record a successful local Phase 3 acceptance report as SHA-bound evidence.

Run the expensive verifier first:

    python3 tools/verify_phase3_local.py --acceptance

Then, from the same clean Git checkout, run this script. It refuses partial,
skipped, dirty, stale-SHA, or model-only reports and writes a normalized JSON
record under docs/verification/ whose filename contains the exact accepted SHA.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REPORT = ROOT / "target" / "phase3-local-verification.json"
EVIDENCE_DIR = ROOT / "docs" / "verification"
REQUIRED_CHECKS = {
    "Pinned clean acceptance candidate",
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
    completed = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise RecordError(completed.stderr.strip() or f"git {' '.join(args)} failed")
    return completed.stdout.strip()


def load_report(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text())
    except FileNotFoundError as exc:
        raise RecordError(f"local acceptance report not found: {path}") from exc
    except (OSError, json.JSONDecodeError) as exc:
        raise RecordError(f"cannot read local acceptance report: {exc}") from exc
    if not isinstance(payload, dict):
        raise RecordError("local acceptance report must be a JSON object")
    return payload


def validate_report(report: dict) -> tuple[str, list[dict]]:
    if report.get("schema") != "ucof-phase3-local-verification-v1":
        raise RecordError("unexpected local verification schema")
    if report.get("mode") != "acceptance" or report.get("ok") is not True:
        raise RecordError("only a successful acceptance-mode report can be recorded")
    if report.get("skipped") not in ([], None):
        raise RecordError("acceptance report contains skipped checks")
    if report.get("dirty_worktree") is not False:
        raise RecordError("acceptance report was not produced from a clean worktree")
    sha = report.get("git_sha")
    if not isinstance(sha, str) or len(sha) != 40:
        raise RecordError("acceptance report does not contain a full Git SHA")
    current_sha = git_output("rev-parse", "HEAD")
    if sha != current_sha:
        raise RecordError(f"acceptance report SHA {sha} does not match current HEAD {current_sha}")
    if git_output("status", "--porcelain"):
        raise RecordError("current worktree must remain clean while recording acceptance")

    checks = report.get("checks")
    if not isinstance(checks, list):
        raise RecordError("acceptance report check list is missing")
    names: set[str] = set()
    for check in checks:
        if not isinstance(check, dict):
            raise RecordError("acceptance report contains a malformed check record")
        name = check.get("name")
        if not isinstance(name, str):
            raise RecordError("acceptance report contains an unnamed check")
        names.add(name)
        if check.get("status") != "pass":
            raise RecordError(f"acceptance check did not pass: {name}")
    missing = sorted(REQUIRED_CHECKS.difference(names))
    if missing:
        raise RecordError("acceptance report is missing required checks: " + ", ".join(missing))
    fuzz_smoke = sorted(name for name in names if name.startswith("Fuzz smoke "))
    if not fuzz_smoke:
        raise RecordError("acceptance report contains no executed fuzz smoke targets")
    return sha, checks


def normalized_record(report: dict, sha: str, checks: list[dict]) -> dict:
    return {
        "schema": "ucof-phase3-local-acceptance-record-v1",
        "experiment": "0179",
        "git_sha": sha,
        "git_branch": report.get("git_branch"),
        "verification_started_utc": report.get("started_utc"),
        "verification_completed_utc": report.get("completed_utc"),
        "recorded_utc": datetime.now(timezone.utc).isoformat(),
        "python": report.get("python"),
        "rustc": report.get("rustc"),
        "cargo": report.get("cargo"),
        "offline": report.get("offline"),
        "dirty_worktree": False,
        "skipped": [],
        "ok": True,
        "checks": checks,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--output-dir", type=Path, default=EVIDENCE_DIR)
    args = parser.parse_args()
    report_path = args.report if args.report.is_absolute() else ROOT / args.report
    output_dir = args.output_dir if args.output_dir.is_absolute() else ROOT / args.output_dir
    try:
        report = load_report(report_path)
        sha, checks = validate_report(report)
        record = normalized_record(report, sha, checks)
        output_dir.mkdir(parents=True, exist_ok=True)
        destination = output_dir / f"phase3-local-acceptance-{sha}.json"
        encoded = json.dumps(record, indent=2, sort_keys=True) + "\n"
        if destination.exists():
            if destination.read_text() != encoded:
                raise RecordError(f"acceptance record already exists with different contents: {destination}")
            print(destination.relative_to(ROOT))
            return 0
        destination.write_text(encoded)
        print(destination.relative_to(ROOT))
        return 0
    except RecordError as exc:
        print(f"record local acceptance: FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
