#!/usr/bin/env python3
"""Run Phase 3 deployment preflight and emit a shareable exact-SHA-bound bundle.

The inner verify_phase3_deployment_preflight.py report remains authoritative for
filesystem/key/storage checks. This wrapper hashes that raw report, then redacts
operator-supplied local paths before persisting a bundle suitable for external
review. It does not add KMS/HSM or production-key claims.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PREFLIGHT = ROOT / "tools" / "verify_phase3_deployment_preflight.py"
SCHEMA = "ucof-phase3-deployment-preflight-bundle-v1"
INNER_SCHEMA = "ucof-phase3-deployment-preflight-v3"
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")


class DeploymentBundleError(RuntimeError):
    pass


def canonical_git_sha(value: str, label: str) -> str:
    if not GIT_SHA_RE.fullmatch(value):
        raise DeploymentBundleError(
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


def read_inner_report(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise DeploymentBundleError(f"cannot read deployment preflight report: {exc}") from exc
    if not isinstance(payload, dict):
        raise DeploymentBundleError("deployment preflight report must be a JSON object")
    if payload.get("schema") != INNER_SCHEMA:
        raise DeploymentBundleError("deployment preflight schema mismatch")
    return payload


def validate_successful_inner_report(report: dict) -> None:
    if report.get("ok") is not True:
        raise DeploymentBundleError("deployment preflight report is not successful")
    if report.get("child_evidence_valid") is not True:
        raise DeploymentBundleError("deployment child evidence is not valid")
    errors = report.get("child_validation_errors")
    if errors != []:
        raise DeploymentBundleError("deployment report contains child validation errors")
    if report.get("production_policy") not in {
        "local-mechanical-preflight-only",
        "unsupported-without-provider-qualification",
    }:
        raise DeploymentBundleError("deployment production policy is indeterminate")
    non_claims = report.get("non_claims")
    if not isinstance(non_claims, dict) or any(value is not False for value in non_claims.values()):
        raise DeploymentBundleError("deployment report changed a non-claim")
    key_material = report.get("key_material")
    if not isinstance(key_material, dict) or key_material.get("secret_material_reported") is not False:
        raise DeploymentBundleError("deployment key report does not preserve secret-material non-reporting")
    storage = report.get("storage_headroom")
    if not isinstance(storage, dict):
        raise DeploymentBundleError("deployment report lacks storage headroom evidence")
    if storage.get("reserved") is not False or storage.get("race_free") is not False:
        raise DeploymentBundleError("deployment storage report changed reservation/race non-claims")


def redaction_values(paths: tuple[Path, ...]) -> tuple[str, ...]:
    values: set[str] = set()
    for path in paths:
        values.add(str(path))
        try:
            values.add(str(path.resolve()))
        except OSError:
            pass
    return tuple(sorted((value for value in values if value), key=len, reverse=True))


def redact_paths(value: object, paths: tuple[str, ...]) -> object:
    if isinstance(value, str):
        redacted = value
        for path in paths:
            redacted = redacted.replace(path, "<redacted-local-path>")
        return redacted
    if isinstance(value, list):
        return [redact_paths(item, paths) for item in value]
    if isinstance(value, dict):
        return {key: redact_paths(item, paths) for key, item in value.items()}
    return value


def build_inner_command(args: argparse.Namespace, output: Path) -> list[str]:
    command = [
        sys.executable,
        str(PREFLIGHT),
        "--filesystem-path",
        str(args.filesystem_path),
        "--aes-key",
        str(args.aes_key),
        "--hmac-key",
        str(args.hmac_key),
        "--required-bytes",
        str(args.required_bytes),
        "--max-initial-runs",
        str(args.max_initial_runs),
        "--required-inodes",
        str(args.required_inodes),
        "--reserve-bytes",
        str(args.reserve_bytes),
        "--reserve-inodes",
        str(args.reserve_inodes),
        "--output",
        str(output),
    ]
    return command


def sanitized_invocation(args: argparse.Namespace) -> dict:
    return {
        "filesystem_path_supplied": True,
        "aes_key_path_supplied": True,
        "hmac_key_path_supplied": True,
        "required_bytes": args.required_bytes,
        "max_initial_runs": args.max_initial_runs,
        "required_inodes": args.required_inodes,
        "reserve_bytes": args.reserve_bytes,
        "reserve_inodes": args.reserve_inodes,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate-git-sha", required=True)
    parser.add_argument("--qualification-tool-git-sha", required=True)
    parser.add_argument("--filesystem-path", type=Path, required=True)
    parser.add_argument("--aes-key", type=Path, required=True)
    parser.add_argument("--hmac-key", type=Path, required=True)
    parser.add_argument("--required-bytes", type=int, required=True)
    parser.add_argument("--max-initial-runs", type=int, required=True)
    parser.add_argument("--required-inodes", type=int, default=0)
    parser.add_argument("--reserve-bytes", type=int, default=0)
    parser.add_argument("--reserve-inodes", type=int, default=0)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        candidate_sha = canonical_git_sha(args.candidate_git_sha, "candidate git SHA")
        tool_sha = canonical_git_sha(args.qualification_tool_git_sha, "qualification-tool git SHA")
        for label, value in (
            ("required bytes", args.required_bytes),
            ("required inodes", args.required_inodes),
            ("reserve bytes", args.reserve_bytes),
            ("reserve inodes", args.reserve_inodes),
        ):
            if value < 0:
                raise DeploymentBundleError(f"{label} must be nonnegative")
        if args.max_initial_runs <= 0:
            raise DeploymentBundleError("max initial runs must be positive")
    except DeploymentBundleError as exc:
        print(f"Phase 3 deployment bundle: FAIL: {exc}", file=sys.stderr)
        return 2

    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    started = datetime.now(timezone.utc).isoformat()
    inner_report: dict | None = None
    inner_sha256: str | None = None
    validation_error: str | None = None
    returncode: int | None = None
    stdout_bytes = 0
    stderr_bytes = 0
    paths = redaction_values((args.filesystem_path, args.aes_key, args.hmac_key))

    with tempfile.TemporaryDirectory(prefix="ucof-phase3-deployment-bundle-") as directory:
        inner_output = Path(directory) / "deployment-preflight.json"
        try:
            completed = subprocess.run(
                build_inner_command(args, inner_output),
                cwd=ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            returncode = completed.returncode
            stdout_bytes = len(completed.stdout.encode("utf-8", errors="replace"))
            stderr_bytes = len(completed.stderr.encode("utf-8", errors="replace"))
        except OSError as exc:
            validation_error = f"cannot execute deployment preflight: {exc}"
        if inner_output.exists():
            try:
                raw = read_inner_report(inner_output)
                inner_sha256 = file_sha256(inner_output)
                if returncode == 0:
                    validate_successful_inner_report(raw)
                redacted = redact_paths(raw, paths)
                if not isinstance(redacted, dict):
                    raise DeploymentBundleError("redacted deployment report changed type")
                inner_report = redacted
            except DeploymentBundleError as exc:
                validation_error = str(exc)
        elif validation_error is None:
            validation_error = "deployment preflight did not produce a report"

    ok = (
        returncode == 0
        and inner_report is not None
        and inner_report.get("ok") is True
        and validation_error is None
    )
    bundle = {
        "schema": SCHEMA,
        "started_utc": started,
        "completed_utc": datetime.now(timezone.utc).isoformat(),
        "ok": ok,
        "candidate_git_sha": candidate_sha,
        "qualification_tool_git_sha": tool_sha,
        "inner_report_sha256": inner_sha256,
        "preflight": {
            "path": "tools/verify_phase3_deployment_preflight.py",
            "schema": INNER_SCHEMA,
            "returncode": returncode,
            "stdout_bytes": stdout_bytes,
            "stderr_bytes": stderr_bytes,
            "raw_output_persisted": False,
        },
        "invocation": sanitized_invocation(args),
        "environment": {
            "python": sys.version.split()[0],
            "platform": platform.platform(),
        },
        "deployment_preflight": inner_report,
        "validation_error": validation_error,
        "non_claims": {
            "raw_local_paths_persisted": False,
            "key_material_persisted": False,
            "production_key_provisioning_qualified": False,
            "kms_hsm_backing_qualified": False,
            "key_rotation_revocation_qualified": False,
            "power_loss_qualified": False,
            "anti_rollback_qualified": False,
            "same_uid_unlink_race_closed": False,
            "free_space_or_inodes_reserved": False,
            "production_accepted": False,
        },
    }
    output.write_text(json.dumps(bundle, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "ok": ok,
        "report": str(output),
        "candidate_git_sha": candidate_sha,
        "qualification_tool_git_sha": tool_sha,
        "inner_report_sha256": inner_sha256,
    }, indent=2, sort_keys=True))
    if validation_error:
        print(f"Phase 3 deployment bundle: FAIL: {validation_error}", file=sys.stderr)
    return 0 if ok else (returncode if returncode in (1, 2) else 1)


if __name__ == "__main__":
    raise SystemExit(main())
