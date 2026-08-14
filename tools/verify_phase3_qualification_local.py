#!/usr/bin/env python3
"""Run Phase 3 environment/cleanup qualification adjunct checks locally.

This intentionally does not replace tools/verify_phase3_local.py --acceptance.
The acceptance runner verifies deterministic repository code. This adjunct
collects environment-specific filesystem mechanics and separately models the
Terminal-last metadata reclamation proof order.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REPORT = ROOT / "target" / "phase3-local-qualification.json"


def run(name: str, command: list[str]) -> dict:
    started = time.monotonic()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return {
        "name": name,
        "command": command,
        "status": "pass" if completed.returncode == 0 else "fail",
        "returncode": completed.returncode,
        "elapsed_seconds": round(time.monotonic() - started, 3),
        "output": completed.stdout,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--scratch-dir",
        type=Path,
        default=ROOT,
        help="filesystem location to smoke-test (default: repository filesystem)",
    )
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--prune-campaigns", type=int, default=10000)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.prune_campaigns <= 0:
        print("qualification: FAIL: prune campaigns must be positive", file=sys.stderr)
        return 1

    started = datetime.now(timezone.utc).isoformat()
    fs_report = ROOT / "target" / "phase3-filesystem-smoke.json"
    checks = [
        run(
            "filesystem qualification harness self-test",
            [sys.executable, "tools/test_qualify_phase3_filesystem.py"],
        ),
        run(
            "Terminal-last prune-order independent model",
            [
                sys.executable,
                "tools/verify_restart_metadata_prune_order.py",
                "--campaigns",
                str(args.prune_campaigns),
            ],
        ),
        run(
            "filesystem mechanical smoke",
            [
                sys.executable,
                "tools/qualify_phase3_filesystem.py",
                "--scratch-dir",
                str(args.scratch_dir),
                "--output",
                str(fs_report),
            ],
        ),
    ]
    ok = all(check["status"] == "pass" for check in checks)
    report = {
        "schema": "ucof-phase3-local-qualification-v1",
        "started_utc": started,
        "completed_utc": datetime.now(timezone.utc).isoformat(),
        "scratch_dir": str(args.scratch_dir.resolve()),
        "ok": ok,
        "checks": checks,
        "filesystem_report": str(fs_report.relative_to(ROOT)),
        "non_claims": {
            "power_loss_qualified": False,
            "anti_rollback_qualified": False,
            "same_uid_unlink_race_closed": False,
            "free_space_reserved": False,
        },
    }
    output = args.report if args.report.is_absolute() else ROOT / args.report
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    for check in checks:
        print(f"{check['status'].upper():4} {check['name']} ({check['elapsed_seconds']}s)")
        if check["status"] != "pass" and check["output"]:
            print(check["output"], file=sys.stderr)
    print(f"report={output.relative_to(ROOT)}")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
