#!/usr/bin/env python3
"""Bundle Phase 3 deployment-adjacent local preflights.

This requires the actual target filesystem path, actual local AES/HMAC key
files (when file-backed secrets are the deployment mechanism), and the exact
private byte requirement produced by the deterministic lifecycle planner.

A successful bundle is not production acceptance: it records mechanical
filesystem behavior, key-file hygiene, and current byte/inode headroom only.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


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
    parser.add_argument("--filesystem-path", type=Path, required=True)
    parser.add_argument("--aes-key", type=Path, required=True)
    parser.add_argument("--hmac-key", type=Path, required=True)
    parser.add_argument("--required-bytes", type=int, required=True)
    parser.add_argument("--required-inodes", type=int, default=0)
    parser.add_argument("--reserve-bytes", type=int, default=0)
    parser.add_argument("--reserve-inodes", type=int, default=0)
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "target" / "phase3-deployment-preflight.json",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    for label, value in (
        ("required bytes", args.required_bytes),
        ("required inodes", args.required_inodes),
        ("reserve bytes", args.reserve_bytes),
        ("reserve inodes", args.reserve_inodes),
    ):
        if value < 0:
            print(f"deployment preflight: FAIL: {label} must be nonnegative", file=sys.stderr)
            return 2

    output = args.output if args.output.is_absolute() else ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="ucof-deployment-preflight-") as directory:
        temp = Path(directory)
        fs_report = temp / "filesystem.json"
        key_report = temp / "keys.json"
        storage_report = temp / "storage.json"
        checks = [
            run(
                "filesystem mechanical qualification",
                [
                    sys.executable,
                    "tools/qualify_phase3_filesystem.py",
                    "--scratch-dir",
                    str(args.filesystem_path),
                    "--output",
                    str(fs_report),
                ],
            ),
            run(
                "key-material preflight",
                [
                    sys.executable,
                    "tools/qualify_phase3_key_material.py",
                    "--aes-key",
                    str(args.aes_key),
                    "--hmac-key",
                    str(args.hmac_key),
                    "--output",
                    str(key_report),
                ],
            ),
            run(
                "storage headroom observation",
                [
                    sys.executable,
                    "tools/check_phase3_storage_headroom.py",
                    "--path",
                    str(args.filesystem_path),
                    "--required-bytes",
                    str(args.required_bytes),
                    "--required-inodes",
                    str(args.required_inodes),
                    "--reserve-bytes",
                    str(args.reserve_bytes),
                    "--reserve-inodes",
                    str(args.reserve_inodes),
                    "--output",
                    str(storage_report),
                ],
            ),
        ]
        ok = all(check["status"] == "pass" for check in checks)

        def read_report(path: Path) -> dict | None:
            if not path.exists():
                return None
            try:
                payload = json.loads(path.read_text())
            except (OSError, json.JSONDecodeError):
                return None
            return payload if isinstance(payload, dict) else None

        report = {
            "schema": "ucof-phase3-deployment-preflight-v1",
            "recorded_utc": datetime.now(timezone.utc).isoformat(),
            "ok": ok,
            "inputs": {
                "filesystem_path": str(args.filesystem_path.resolve()),
                "aes_key_path": str(args.aes_key.resolve()),
                "hmac_key_path": str(args.hmac_key.resolve()),
                "required_bytes": args.required_bytes,
                "required_inodes": args.required_inodes,
                "reserve_bytes": args.reserve_bytes,
                "reserve_inodes": args.reserve_inodes,
            },
            "checks": checks,
            "filesystem": read_report(fs_report),
            "key_material": read_report(key_report),
            "storage_headroom": read_report(storage_report),
            "non_claims": {
                "production_accepted": False,
                "power_loss_qualified": False,
                "anti_rollback_qualified": False,
                "same_uid_unlink_race_closed": False,
                "free_space_reserved": False,
                "key_provisioning_or_rotation_qualified": False,
            },
        }
        output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    for check in checks:
        print(f"{check['status'].upper():4} {check['name']} ({check['elapsed_seconds']}s)")
        if check["status"] != "pass" and check["output"]:
            print(check["output"], file=sys.stderr)
    print(f"report={output.relative_to(ROOT) if output.is_relative_to(ROOT) else output}")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
