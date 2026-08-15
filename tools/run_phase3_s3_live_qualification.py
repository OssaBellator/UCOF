#!/usr/bin/env python3
"""Run the existing versioned-S3 harness and bind its evidence to exact UCOF/tool SHAs.

This wrapper does not weaken or replace qualify_phase3_s3_versioned_source.py.
It records provenance around that provider report so an externally executed
qualification can be reviewed against the exact candidate and qualification-
tool source that produced it.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
HARNESS = ROOT / "tools" / "qualify_phase3_s3_versioned_source.py"
SCHEMA = "ucof-phase3-s3-live-qualification-bundle-v1"
PROVIDER_SCHEMA = "ucof-phase3-s3-versioned-source-qualification-v1"
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
REQUIRED_PROVIDER_CHECKS = (
    "bucket_versioning_enabled",
    "write_qualification_executed",
    "distinct_version_ids_for_repeated_put",
    "historical_version_payload_immutable",
    "historical_range_read_exact",
    "current_read_tracks_latest_version",
    "nonexistent_version_rejected",
    "historical_version_survives_delete_marker",
)
SENSITIVE_ENV_NAMES = (
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_SECURITY_TOKEN",
)


class S3QualificationBundleError(RuntimeError):
    pass


def canonical_git_sha(value: str, label: str) -> str:
    if not GIT_SHA_RE.fullmatch(value):
        raise S3QualificationBundleError(
            f"{label} must be exactly 40 lowercase hexadecimal characters"
        )
    return value


def file_sha256(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        while True:
            block = source.read(1024 * 1024)
            if not block:
                break
            hasher.update(block)
    return hasher.hexdigest()


def read_provider_report(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise S3QualificationBundleError(f"cannot read provider report: {exc}") from exc
    if not isinstance(payload, dict):
        raise S3QualificationBundleError("provider report must be a JSON object")
    if payload.get("schema") != PROVIDER_SCHEMA:
        raise S3QualificationBundleError("provider report schema mismatch")
    return payload


def validate_successful_provider_report(report: dict) -> None:
    if report.get("ok") is not True:
        raise S3QualificationBundleError("provider report is not successful")
    if report.get("bucket_versioning_status") != "Enabled":
        raise S3QualificationBundleError("provider report lacks Enabled bucket versioning")
    checks = report.get("checks")
    if not isinstance(checks, dict):
        raise S3QualificationBundleError("provider report lacks checks")
    failed = [name for name in REQUIRED_PROVIDER_CHECKS if checks.get(name) is not True]
    if failed:
        raise S3QualificationBundleError(
            "provider report lacks required successful checks: " + ", ".join(failed)
        )
    cleanup = report.get("cleanup")
    if not isinstance(cleanup, dict) or cleanup.get("complete") is not True:
        raise S3QualificationBundleError("provider report cleanup is incomplete")
    non_claims = report.get("non_claims")
    if not isinstance(non_claims, dict):
        raise S3QualificationBundleError("provider report lacks non-claims")
    if any(value is not False for value in non_claims.values()):
        raise S3QualificationBundleError("provider report changed a qualification non-claim")


def sensitive_values(profile: str | None) -> tuple[str, ...]:
    values = [profile or ""]
    values.extend(os.environ.get(name, "") for name in SENSITIVE_ENV_NAMES)
    return tuple(value for value in values if value)


def redact_sensitive_data(value: object, secrets: tuple[str, ...]) -> object:
    if isinstance(value, str):
        redacted = value
        for secret in secrets:
            redacted = redacted.replace(secret, "<redacted>")
        return redacted
    if isinstance(value, list):
        return [redact_sensitive_data(item, secrets) for item in value]
    if isinstance(value, dict):
        return {
            key: redact_sensitive_data(item, secrets)
            for key, item in value.items()
        }
    return value


def aws_cli_version() -> dict:
    executable = shutil.which("aws")
    if not executable:
        return {"available": False, "executable": None, "version": None}
    completed = subprocess.run(
        [executable, "--version"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return {
        "available": completed.returncode == 0,
        "executable": executable,
        "version": completed.stdout.strip()[:512],
    }


def build_harness_command(args: argparse.Namespace, provider_output: Path) -> list[str]:
    command = [
        sys.executable,
        str(HARNESS),
        "--bucket",
        args.bucket,
        "--prefix",
        args.prefix,
        "--payload-bytes",
        str(args.payload_bytes),
        "--output",
        str(provider_output),
    ]
    if args.region:
        command += ["--region", args.region]
    if args.profile:
        command += ["--profile", args.profile]
    if args.allow_write:
        command.append("--allow-write")
    if args.keep_objects:
        command.append("--keep-objects")
    return command


def sanitized_harness_invocation(args: argparse.Namespace) -> dict:
    return {
        "bucket": args.bucket,
        "region": args.region,
        "profile_named": bool(args.profile),
        "prefix": args.prefix,
        "payload_bytes": args.payload_bytes,
        "allow_write": args.allow_write,
        "keep_objects": args.keep_objects,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate-git-sha", required=True)
    parser.add_argument("--qualification-tool-git-sha", required=True)
    parser.add_argument("--bucket", required=True)
    parser.add_argument("--region")
    parser.add_argument("--profile")
    parser.add_argument("--prefix", default="ucof-phase3-live-qualification/")
    parser.add_argument("--payload-bytes", type=int, default=1024 * 1024)
    parser.add_argument("--allow-write", action="store_true")
    parser.add_argument("--keep-objects", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        candidate_sha = canonical_git_sha(args.candidate_git_sha, "candidate git SHA")
        tool_sha = canonical_git_sha(
            args.qualification_tool_git_sha,
            "qualification-tool git SHA",
        )
        if args.payload_bytes <= 0:
            raise S3QualificationBundleError("payload bytes must be positive")
    except S3QualificationBundleError as exc:
        print(f"Phase 3 live S3 qualification bundle: FAIL: {exc}", file=sys.stderr)
        return 2

    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    started = datetime.now(timezone.utc).isoformat()
    provider_report: dict | None = None
    provider_report_sha256: str | None = None
    validation_error: str | None = None
    harness_stdout_bytes = 0
    harness_stderr_bytes = 0
    harness_returncode: int | None = None
    secrets = sensitive_values(args.profile)

    with tempfile.TemporaryDirectory(prefix="ucof-phase3-s3-live-") as directory:
        provider_output = Path(directory) / "provider-report.json"
        command = build_harness_command(args, provider_output)
        try:
            completed = subprocess.run(
                command,
                cwd=ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            harness_returncode = completed.returncode
            harness_stdout_bytes = len(completed.stdout.encode("utf-8", errors="replace"))
            harness_stderr_bytes = len(completed.stderr.encode("utf-8", errors="replace"))
        except OSError as exc:
            validation_error = f"cannot execute provider harness: {exc}"
        if provider_output.exists():
            try:
                raw_provider_report = read_provider_report(provider_output)
                provider_report_sha256 = file_sha256(provider_output)
                if harness_returncode == 0:
                    validate_successful_provider_report(raw_provider_report)
                redacted = redact_sensitive_data(raw_provider_report, secrets)
                if not isinstance(redacted, dict):
                    raise S3QualificationBundleError("redacted provider report changed type")
                provider_report = redacted
            except S3QualificationBundleError as exc:
                validation_error = str(exc)
        elif validation_error is None:
            validation_error = "provider harness did not produce a report"

    ok = (
        harness_returncode == 0
        and provider_report is not None
        and provider_report.get("ok") is True
        and validation_error is None
    )
    bundle = {
        "schema": SCHEMA,
        "started_utc": started,
        "completed_utc": datetime.now(timezone.utc).isoformat(),
        "ok": ok,
        "candidate_git_sha": candidate_sha,
        "qualification_tool_git_sha": tool_sha,
        "provider_report_sha256": provider_report_sha256,
        "harness": {
            "path": "tools/qualify_phase3_s3_versioned_source.py",
            "schema": PROVIDER_SCHEMA,
            "returncode": harness_returncode,
            "stdout_bytes": harness_stdout_bytes,
            "stderr_bytes": harness_stderr_bytes,
            "raw_output_persisted": False,
        },
        "invocation": sanitized_harness_invocation(args),
        "environment": {
            "python": sys.version.split()[0],
            "platform": platform.platform(),
            "aws_cli": aws_cli_version(),
            "aws_execution_environment_named": bool(os.environ.get("AWS_EXECUTION_ENV")),
        },
        "provider_report": provider_report,
        "validation_error": validation_error,
        "non_claims": {
            "candidate_binary_execution_proven_by_sha_field_alone": False,
            "aws_endpoint_authenticity_proven_by_wrapper": False,
            "iam_policy_matrix_exhaustive": False,
            "sts_refresh_lifecycle_qualified": False,
            "tls_proxy_policy_qualified": False,
            "provider_scale_limit_qualified": False,
            "network_fault_matrix_qualified": False,
            "production_accepted": False,
        },
    }
    output.write_text(json.dumps(bundle, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "ok": ok,
        "report": str(output),
        "candidate_git_sha": candidate_sha,
        "qualification_tool_git_sha": tool_sha,
        "provider_report_sha256": provider_report_sha256,
    }, indent=2, sort_keys=True))
    if validation_error:
        print(f"Phase 3 live S3 qualification bundle: FAIL: {validation_error}", file=sys.stderr)
    return 0 if ok else (harness_returncode if harness_returncode in (1, 2) else 1)


if __name__ == "__main__":
    raise SystemExit(main())
