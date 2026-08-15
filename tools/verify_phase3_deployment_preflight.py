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

FILESYSTEM_SCHEMA = "ucof-phase3-filesystem-smoke-v1"
KEY_SCHEMA = "ucof-phase3-key-material-preflight-v1"
STORAGE_SCHEMA = "ucof-phase3-storage-headroom-v1"


class DeploymentPreflightError(RuntimeError):
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


def read_json_report(path: Path, expected_schema: str) -> dict:
    try:
        payload = json.loads(path.read_text())
    except FileNotFoundError as exc:
        raise DeploymentPreflightError(f"missing child report: {path}") from exc
    except (OSError, json.JSONDecodeError) as exc:
        raise DeploymentPreflightError(f"invalid child report {path}: {exc}") from exc
    if not isinstance(payload, dict):
        raise DeploymentPreflightError(f"child report is not a JSON object: {path}")
    if payload.get("schema") != expected_schema:
        raise DeploymentPreflightError(
            f"child report schema mismatch for {path}: {payload.get('schema')!r}"
        )
    return payload


def validate_filesystem_report(report: dict) -> None:
    if not isinstance(report.get("network_or_distributed_filesystem"), bool):
        raise DeploymentPreflightError(
            "filesystem report is missing network/distributed classification"
        )
    smoke = report.get("mechanical_smoke")
    if not isinstance(smoke, dict):
        raise DeploymentPreflightError("filesystem report is missing mechanical smoke evidence")
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
        raise DeploymentPreflightError(
            "filesystem report is missing mechanical checks: " + ", ".join(missing)
        )
    false_checks = sorted(name for name in required if smoke.get(name) is not True)
    if false_checks:
        raise DeploymentPreflightError(
            "filesystem report contains failed mechanical checks: "
            + ", ".join(false_checks)
        )


def validate_key_report(report: dict) -> None:
    if report.get("ok") is not True:
        raise DeploymentPreflightError("key-material child report is not successful")
    if report.get("secret_material_reported") is not False:
        raise DeploymentPreflightError("key-material report claims secret material was reported")
    claims = report.get("claims")
    if not isinstance(claims, dict):
        raise DeploymentPreflightError("key-material report is missing claims")
    required_true = {
        "exact_width",
        "regular_file",
        "effective_uid_owned",
        "single_hard_link",
        "no_group_or_world_permissions",
        "parent_directory_effective_uid_owned",
        "parent_directory_not_group_or_world_writable",
        "distinct_files",
        "distinct_secret_bytes",
    }
    missing = sorted(name for name in required_true if claims.get(name) is not True)
    if missing:
        raise DeploymentPreflightError(
            "key-material report is missing required true claims: " + ", ".join(missing)
        )


def validate_storage_report(report: dict) -> None:
    if report.get("ok") is not True:
        raise DeploymentPreflightError("storage-headroom child report is not successful")
    if report.get("reserved") is not False or report.get("race_free") is not False:
        raise DeploymentPreflightError(
            "storage report must preserve reservation/race non-claims"
        )
    observation = report.get("observation")
    if not isinstance(observation, dict):
        raise DeploymentPreflightError("storage report is missing observation")
    if observation.get("bytes_ok") is not True or observation.get("inodes_ok") is not True:
        raise DeploymentPreflightError("storage observation does not satisfy byte/inode headroom")


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
    validation_errors: list[str] = []
    filesystem: dict | None = None
    key_material: dict | None = None
    storage_headroom: dict | None = None

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

        if all(check["status"] == "pass" for check in checks):
            try:
                filesystem = read_json_report(fs_report, FILESYSTEM_SCHEMA)
                validate_filesystem_report(filesystem)
                key_material = read_json_report(key_report, KEY_SCHEMA)
                validate_key_report(key_material)
                storage_headroom = read_json_report(storage_report, STORAGE_SCHEMA)
                validate_storage_report(storage_headroom)
            except DeploymentPreflightError as exc:
                validation_errors.append(str(exc))

        subprocess_ok = all(check["status"] == "pass" for check in checks)
        evidence_ok = not validation_errors and all(
            report is not None for report in (filesystem, key_material, storage_headroom)
        )
        ok = subprocess_ok and evidence_ok
        network_filesystem = (
            filesystem.get("network_or_distributed_filesystem")
            if isinstance(filesystem, dict)
            else None
        )

        report = {
            "schema": "ucof-phase3-deployment-preflight-v2",
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
            "child_evidence_valid": evidence_ok,
            "child_validation_errors": validation_errors,
            "filesystem": filesystem,
            "key_material": key_material,
            "storage_headroom": storage_headroom,
            "network_or_distributed_filesystem": network_filesystem,
            "production_policy": (
                "unsupported-without-provider-qualification"
                if network_filesystem is True
                else "local-mechanical-preflight-only"
                if network_filesystem is False
                else "indeterminate"
            ),
            "non_claims": {
                "production_accepted": False,
                "power_loss_qualified": False,
                "anti_rollback_qualified": False,
                "same_uid_unlink_race_closed": False,
                "free_space_reserved": False,
                "key_provisioning_or_rotation_qualified": False,
                "ancestor_key_path_pinning_qualified": False,
            },
        }
        output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    for check in checks:
        print(f"{check['status'].upper():4} {check['name']} ({check['elapsed_seconds']}s)")
        if check["status"] != "pass" and check["output"]:
            print(check["output"], file=sys.stderr)
    for error in validation_errors:
        print(f"FAIL child evidence: {error}", file=sys.stderr)
    print(f"report={output.relative_to(ROOT) if output.is_relative_to(ROOT) else output}")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
