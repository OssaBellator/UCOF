#!/usr/bin/env python3
"""Qualify Phase 3 versioned-S3 immutable-source assumptions.

The harness shells out to an already-configured AWS CLI. It never installs
packages, changes IAM/bucket policy, or operates on caller data. Write mode uses
one unique object key beneath an explicit qualification prefix and cleanup only
targets exact versions/delete markers for that key.

This is provider qualification evidence, not wire-format or production policy.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, asdict
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

DEFAULT_PREFIX = "ucof-phase3-qualification/"
SCHEMA = "ucof-phase3-s3-versioned-source-qualification-v1"


class S3QualificationError(RuntimeError):
    pass


@dataclass
class CommandEvidence:
    name: str
    command: list[str]
    returncode: int | None
    elapsed_seconds: float
    ok: bool
    stdout_json: object | None
    stderr_tail: str


class AwsCli:
    def __init__(self, region: str | None, profile: str | None) -> None:
        executable = shutil.which("aws")
        if not executable:
            raise S3QualificationError("AWS CLI is not installed")
        self.executable = executable
        self.region = region
        self.profile = profile
        self.evidence: list[CommandEvidence] = []

    def command(self, args: list[str]) -> list[str]:
        command = [self.executable, *args, "--no-cli-pager"]
        if self.region:
            command += ["--region", self.region]
        if self.profile:
            command += ["--profile", self.profile]
        return command

    def run_json(
        self,
        name: str,
        args: list[str],
        *,
        allow_failure: bool = False,
    ) -> object | None:
        command = self.command(args + ["--output", "json"])
        started = time.monotonic()
        completed = subprocess.run(
            command,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        elapsed = time.monotonic() - started
        payload: object | None = None
        parse_error: str | None = None
        if completed.stdout.strip():
            try:
                payload = json.loads(completed.stdout)
            except json.JSONDecodeError as exc:
                parse_error = str(exc)
        stderr_tail = "\n".join(completed.stderr.splitlines()[-12:])
        ok = completed.returncode == 0 and parse_error is None
        self.evidence.append(
            CommandEvidence(
                name=name,
                command=sanitize_command(command),
                returncode=completed.returncode,
                elapsed_seconds=round(elapsed, 3),
                ok=ok,
                stdout_json=payload,
                stderr_tail=(stderr_tail if not ok else ""),
            )
        )
        if not ok and not allow_failure:
            detail = parse_error or stderr_tail or f"exit status {completed.returncode}"
            raise S3QualificationError(f"{name} failed: {detail}")
        return payload

    def run_binary_output(
        self,
        name: str,
        args: list[str],
        destination: Path,
        *,
        allow_failure: bool = False,
    ) -> bool:
        command = self.command(args + [str(destination)])
        started = time.monotonic()
        completed = subprocess.run(
            command,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        elapsed = time.monotonic() - started
        stderr_tail = "\n".join(completed.stderr.splitlines()[-12:])
        ok = completed.returncode == 0
        self.evidence.append(
            CommandEvidence(
                name=name,
                command=sanitize_command(command),
                returncode=completed.returncode,
                elapsed_seconds=round(elapsed, 3),
                ok=ok,
                stdout_json=None,
                stderr_tail=(stderr_tail if not ok else ""),
            )
        )
        if not ok and not allow_failure:
            raise S3QualificationError(
                f"{name} failed: {stderr_tail or f'exit status {completed.returncode}'}"
            )
        return ok


def sanitize_command(command: list[str]) -> list[str]:
    # The harness never supplies secrets on argv. Keep this defensive so a
    # future option cannot accidentally persist a credential-bearing value.
    redacted: list[str] = []
    hide_next = False
    sensitive = {"--secret-access-key", "--session-token", "--access-key-id"}
    for token in command:
        if hide_next:
            redacted.append("<redacted>")
            hide_next = False
        elif token in sensitive:
            redacted.append(token)
            hide_next = True
        else:
            redacted.append(token)
    return redacted


def require_dict(payload: object | None, label: str) -> dict:
    if not isinstance(payload, dict):
        raise S3QualificationError(f"{label} did not return a JSON object")
    return payload


def require_text(mapping: dict, key: str, label: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value:
        raise S3QualificationError(f"{label} did not return {key}")
    return value


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def deterministic_payload(size: int, variant: int) -> bytes:
    if size <= 0:
        raise S3QualificationError("payload size must be positive")
    seed = hashlib.sha256(f"UCOF-PHASE3-S3-{variant}".encode()).digest()
    data = bytearray()
    counter = 0
    while len(data) < size:
        data.extend(hashlib.sha256(seed + counter.to_bytes(8, "little")).digest())
        counter += 1
    return bytes(data[:size])


def normalize_prefix(prefix: str) -> str:
    if not prefix or prefix.startswith("/") or ".." in Path(prefix).parts:
        raise S3QualificationError("qualification prefix must be a relative non-empty key prefix")
    return prefix if prefix.endswith("/") else prefix + "/"


def exact_versions(payload: object | None, key: str) -> list[tuple[str, str]]:
    result = require_dict(payload, "list-object-versions")
    found: list[tuple[str, str]] = []
    for field, kind in (("Versions", "version"), ("DeleteMarkers", "delete-marker")):
        entries = result.get(field, [])
        if entries is None:
            continue
        if not isinstance(entries, list):
            raise S3QualificationError(f"list-object-versions {field} is not a list")
        for entry in entries:
            if not isinstance(entry, dict) or entry.get("Key") != key:
                continue
            version_id = entry.get("VersionId")
            if isinstance(version_id, str) and version_id:
                found.append((kind, version_id))
    return found


def cleanup_exact_key(cli: AwsCli, bucket: str, key: str) -> dict:
    listing = cli.run_json(
        "list qualification key versions for cleanup",
        ["s3api", "list-object-versions", "--bucket", bucket, "--prefix", key],
        allow_failure=True,
    )
    if listing is None:
        return {"attempted": True, "complete": False, "remaining": "listing-failed"}
    versions = exact_versions(listing, key)
    deleted: list[dict] = []
    for kind, version_id in versions:
        result = cli.run_json(
            f"cleanup {kind}",
            [
                "s3api",
                "delete-object",
                "--bucket",
                bucket,
                "--key",
                key,
                "--version-id",
                version_id,
            ],
            allow_failure=True,
        )
        deleted.append({"kind": kind, "version_id": version_id, "ok": result is not None})
    final = cli.run_json(
        "verify qualification key cleanup",
        ["s3api", "list-object-versions", "--bucket", bucket, "--prefix", key],
        allow_failure=True,
    )
    remaining = exact_versions(final, key) if final is not None else [("unknown", "unknown")]
    return {
        "attempted": True,
        "deleted": deleted,
        "complete": not remaining,
        "remaining": [{"kind": kind, "version_id": version_id} for kind, version_id in remaining],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bucket", required=True)
    parser.add_argument("--region")
    parser.add_argument("--profile")
    parser.add_argument("--prefix", default=DEFAULT_PREFIX)
    parser.add_argument("--payload-bytes", type=int, default=1024 * 1024)
    parser.add_argument("--allow-write", action="store_true")
    parser.add_argument("--keep-objects", action="store_true")
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    report: dict = {
        "schema": SCHEMA,
        "recorded_utc": datetime.now(timezone.utc).isoformat(),
        "bucket": args.bucket,
        "region": args.region,
        "profile_named": bool(args.profile),
        "write_mode": args.allow_write,
        "provider": "aws-s3-cli",
        "checks": {},
        "commands": [],
        "cleanup": {"attempted": False},
        "non_claims": {
            "iam_policy_matrix_exhaustive": False,
            "sts_refresh_lifecycle_qualified": False,
            "tls_proxy_policy_qualified": False,
            "provider_scale_limit_qualified": False,
            "network_fault_matrix_qualified": False,
            "production_accepted": False,
        },
    }
    key: str | None = None
    cli: AwsCli | None = None
    exit_code = 1
    try:
        if args.payload_bytes <= 0:
            raise S3QualificationError("--payload-bytes must be positive")
        prefix = normalize_prefix(args.prefix)
        key = f"{prefix}{uuid.uuid4().hex}/immutable-source.bin"
        report["key"] = key
        cli = AwsCli(args.region, args.profile)

        versioning = require_dict(
            cli.run_json(
                "read bucket versioning",
                ["s3api", "get-bucket-versioning", "--bucket", args.bucket],
            ),
            "get-bucket-versioning",
        )
        status = versioning.get("Status")
        report["checks"]["bucket_versioning_enabled"] = status == "Enabled"
        report["bucket_versioning_status"] = status
        if status != "Enabled":
            raise S3QualificationError("bucket versioning is not Enabled")

        identity = cli.run_json("read caller identity", ["sts", "get-caller-identity"], allow_failure=True)
        if isinstance(identity, dict):
            report["caller_identity"] = {
                "account": identity.get("Account"),
                "arn": identity.get("Arn"),
            }

        if not args.allow_write:
            report["checks"]["write_qualification_executed"] = False
            report["ok"] = False
            report["readiness"] = "versioning-observed-write-qualification-not-authorized"
            exit_code = 2
            return exit_code

        with tempfile.TemporaryDirectory(prefix="ucof-phase3-s3-") as directory:
            temp = Path(directory)
            payload_one = temp / "payload-one.bin"
            payload_two = temp / "payload-two.bin"
            payload_one.write_bytes(deterministic_payload(args.payload_bytes, 1))
            payload_two.write_bytes(deterministic_payload(args.payload_bytes, 2))
            expected_one = file_sha256(payload_one)
            expected_two = file_sha256(payload_two)

            put_one = require_dict(
                cli.run_json(
                    "put version one",
                    ["s3api", "put-object", "--bucket", args.bucket, "--key", key, "--body", str(payload_one)],
                ),
                "put version one",
            )
            version_one = require_text(put_one, "VersionId", "put version one")
            etag_one = put_one.get("ETag")

            # Put the *same* first payload again to demonstrate that immutable
            # provider identity is VersionId, not payload-equality/ETag.
            put_same = require_dict(
                cli.run_json(
                    "put same payload as distinct version",
                    ["s3api", "put-object", "--bucket", args.bucket, "--key", key, "--body", str(payload_one)],
                ),
                "put same payload",
            )
            version_same = require_text(put_same, "VersionId", "put same payload")
            etag_same = put_same.get("ETag")
            if version_same == version_one:
                raise S3QualificationError("repeated put did not produce a distinct VersionId")

            put_two = require_dict(
                cli.run_json(
                    "put version two",
                    ["s3api", "put-object", "--bucket", args.bucket, "--key", key, "--body", str(payload_two)],
                ),
                "put version two",
            )
            version_two = require_text(put_two, "VersionId", "put version two")
            if len({version_one, version_same, version_two}) != 3:
                raise S3QualificationError("qualification puts did not produce three distinct VersionIds")

            historical = temp / "historical.bin"
            cli.run_binary_output(
                "get historical version one",
                ["s3api", "get-object", "--bucket", args.bucket, "--key", key, "--version-id", version_one],
                historical,
            )
            if file_sha256(historical) != expected_one:
                raise S3QualificationError("historical VersionId read changed payload bytes")

            current = temp / "current.bin"
            cli.run_binary_output(
                "get current version",
                ["s3api", "get-object", "--bucket", args.bucket, "--key", key],
                current,
            )
            if file_sha256(current) != expected_two:
                raise S3QualificationError("current unversioned read is not latest payload")

            range_file = temp / "range.bin"
            range_end = min(args.payload_bytes, 4096) - 1
            cli.run_binary_output(
                "get historical version range",
                [
                    "s3api",
                    "get-object",
                    "--bucket",
                    args.bucket,
                    "--key",
                    key,
                    "--version-id",
                    version_one,
                    "--range",
                    f"bytes=0-{range_end}",
                ],
                range_file,
            )
            if range_file.read_bytes() != payload_one.read_bytes()[: range_end + 1]:
                raise S3QualificationError("historical ranged VersionId read returned wrong bytes")

            head_one = require_dict(
                cli.run_json(
                    "head historical version one",
                    ["s3api", "head-object", "--bucket", args.bucket, "--key", key, "--version-id", version_one],
                ),
                "head version one",
            )
            report["historical_head"] = {
                "content_length": head_one.get("ContentLength"),
                "version_id": head_one.get("VersionId"),
                "etag": head_one.get("ETag"),
            }

            fake_version = "ucof-phase3-nonexistent-version"
            missing_dest = temp / "missing.bin"
            missing_ok = cli.run_binary_output(
                "get nonexistent version",
                ["s3api", "get-object", "--bucket", args.bucket, "--key", key, "--version-id", fake_version],
                missing_dest,
                allow_failure=True,
            )
            if missing_ok:
                raise S3QualificationError("nonexistent VersionId unexpectedly succeeded")

            delete_marker = require_dict(
                cli.run_json(
                    "create delete marker",
                    ["s3api", "delete-object", "--bucket", args.bucket, "--key", key],
                ),
                "delete marker",
            )
            delete_marker_id = delete_marker.get("VersionId")
            report["delete_marker"] = {
                "delete_marker": delete_marker.get("DeleteMarker"),
                "version_id": delete_marker_id,
            }

            after_delete = temp / "historical-after-delete.bin"
            cli.run_binary_output(
                "get historical version after delete marker",
                ["s3api", "get-object", "--bucket", args.bucket, "--key", key, "--version-id", version_one],
                after_delete,
            )
            if file_sha256(after_delete) != expected_one:
                raise S3QualificationError("delete marker changed historical VersionId payload")

            report["versions"] = {
                "first": version_one,
                "same_payload_second_version": version_same,
                "latest": version_two,
                "all_distinct": True,
                "first_and_same_payload_etag_equal": etag_one == etag_same,
            }
            report["payloads"] = {
                "bytes": args.payload_bytes,
                "first_sha256": expected_one,
                "latest_sha256": expected_two,
            }
            report["checks"].update(
                {
                    "write_qualification_executed": True,
                    "distinct_version_ids_for_repeated_put": True,
                    "historical_version_payload_immutable": True,
                    "historical_range_read_exact": True,
                    "current_read_tracks_latest_version": True,
                    "nonexistent_version_rejected": True,
                    "historical_version_survives_delete_marker": True,
                }
            )
            report["ok"] = True
            exit_code = 0
    except (OSError, S3QualificationError, subprocess.SubprocessError) as exc:
        report["ok"] = False
        report["failure"] = str(exc)
        exit_code = 1
    finally:
        if cli is not None and key is not None and args.allow_write and not args.keep_objects:
            try:
                report["cleanup"] = cleanup_exact_key(cli, args.bucket, key)
                if report.get("ok") and not report["cleanup"].get("complete"):
                    report["ok"] = False
                    report["failure"] = "qualification succeeded but exact-key cleanup was incomplete"
                    exit_code = 1
            except Exception as exc:  # cleanup evidence must survive unexpected SDK/CLI shape
                report["cleanup"] = {
                    "attempted": True,
                    "complete": False,
                    "failure": str(exc),
                }
                if report.get("ok"):
                    report["ok"] = False
                    report["failure"] = "qualification succeeded but cleanup raised an exception"
                    exit_code = 1
        if cli is not None:
            report["commands"] = [asdict(entry) for entry in cli.evidence]
        report["completed_utc"] = datetime.now(timezone.utc).isoformat()
        encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
        if args.output:
            output = args.output if args.output.is_absolute() else Path.cwd() / args.output
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(encoded)
        print(encoded, end="")
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
