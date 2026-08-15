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
FILESYSTEM_SCHEMA = "ucof-phase3-filesystem-smoke-v1"


class QualificationBundleError(RuntimeError):
    pass


def run(name: str, command: list[str]) -> dict:
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
    except OSError as exc:
        return {
            "name": name,
            "command": command,
            "status": "fail",
            "returncode": None,
            "elapsed_seconds": round(time.monotonic() - started, 3),
            "output": str(exc),
        }
    return {
        "name": name,
        "command": command,
        "status": "pass" if completed.returncode == 0 else "fail",
        "returncode": completed.returncode,
        "elapsed_seconds": round(time.monotonic() - started, 3),
        "output": completed.stdout,
    }


def read_filesystem_report(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text())
    except FileNotFoundError as exc:
        raise QualificationBundleError(f"filesystem report missing: {path}") from exc
    except (OSError, json.JSONDecodeError) as exc:
        raise QualificationBundleError(f"filesystem report invalid: {exc}") from exc
    if not isinstance(payload, dict):
        raise QualificationBundleError("filesystem report is not a JSON object")
    if payload.get("schema") != FILESYSTEM_SCHEMA:
        raise QualificationBundleError(
            f"filesystem report schema mismatch: {payload.get('schema')!r}"
        )
    if not isinstance(payload.get("network_or_distributed_filesystem"), bool):
        raise QualificationBundleError(
            "filesystem report lacks network/distributed classification"
        )
    smoke = payload.get("mechanical_smoke")
    if not isinstance(smoke, dict):
        raise QualificationBundleError("filesystem report lacks mechanical smoke evidence")
    required = {
        "file_fsync",
        "private_directory_fsync",
        "hard_link_no_overwrite",
        "publication_directory_fsync",
        "published_inode_equal",
        "private_unlink_directory_fsync",
        "publication_unlink_directory_fsync",
    }
    missing = sorted(required - set(smoke))
    if missing:
        raise QualificationBundleError(
            "filesystem report missing mechanical checks: " + ", ".join(missing)
        )
    failed = sorted(name for name in required if smoke.get(name) is not True)
    if failed:
        raise QualificationBundleError(
            "filesystem report contains failed mechanical checks: " + ", ".join(failed)
        )
    return payload


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

    validation_errors: list[str] = []
    filesystem_evidence: dict | None = None
    subprocess_ok = all(check["status"] == "pass" for check in checks)
    if subprocess_ok:
        try:
            filesystem_evidence = read_filesystem_report(fs_report)
        except QualificationBundleError as exc:
            validation_errors.append(str(exc))
    evidence_ok = filesystem_evidence is not None and not validation_errors
    ok = subprocess_ok and evidence_ok
    network_filesystem = (
        filesystem_evidence.get("network_or_distributed_filesystem")
        if filesystem_evidence is not None
        else None
    )

    report = {
        "schema": "ucof-phase3-local-qualification-v2",
        "started_utc": started,
        "completed_utc": datetime.now(timezone.utc).isoformat(),
        "scratch_dir": str(args.scratch_dir.resolve()),
        "ok": ok,
        "checks": checks,
        "filesystem_evidence_valid": evidence_ok,
        "filesystem_validation_errors": validation_errors,
        "filesystem_report": filesystem_evidence,
        "network_or_distributed_filesystem": network_filesystem,
        "production_policy": (
            "unsupported-without-provider-qualification"
            if network_filesystem is True
            else "local-filesystem-mechanical-smoke-only"
            if network_filesystem is False
            else "indeterminate"
        ),
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
    for error in validation_errors:
        print(f"FAIL filesystem evidence: {error}", file=sys.stderr)
    display = output.relative_to(ROOT) if output.is_relative_to(ROOT) else output
    print(f"report={display}")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
