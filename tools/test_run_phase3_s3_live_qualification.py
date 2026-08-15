#!/usr/bin/env python3
"""Self-tests for tools/run_phase3_s3_live_qualification.py."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

from tools import run_phase3_s3_live_qualification as live

CANDIDATE_SHA = "67beca9ce5d1242c5df839c9ef2d7b1ce5a8b774"
TOOL_SHA = "1" * 40


def successful_provider_report() -> dict:
    return {
        "schema": live.PROVIDER_SCHEMA,
        "ok": True,
        "bucket_versioning_status": "Enabled",
        "checks": {name: True for name in live.REQUIRED_PROVIDER_CHECKS},
        "cleanup": {"attempted": True, "complete": True, "remaining": []},
        "non_claims": {
            "iam_policy_matrix_exhaustive": False,
            "sts_refresh_lifecycle_qualified": False,
            "tls_proxy_policy_qualified": False,
            "provider_scale_limit_qualified": False,
            "network_fault_matrix_qualified": False,
            "production_accepted": False,
        },
    }


class LiveS3QualificationBundleTests(unittest.TestCase):
    def test_canonical_git_sha_requires_full_lowercase_hex(self) -> None:
        self.assertEqual(
            live.canonical_git_sha(CANDIDATE_SHA, "candidate git SHA"),
            CANDIDATE_SHA,
        )
        for malformed in (
            CANDIDATE_SHA[:-1],
            CANDIDATE_SHA.upper(),
            "g" * 40,
            "refs/heads/phase-3/restart-metadata-compaction",
        ):
            with self.subTest(malformed=malformed), self.assertRaisesRegex(
                live.S3QualificationBundleError,
                "40 lowercase hexadecimal",
            ):
                live.canonical_git_sha(malformed, "candidate git SHA")

    def test_successful_provider_report_requires_full_strict_surface(self) -> None:
        live.validate_successful_provider_report(successful_provider_report())

        report = successful_provider_report()
        report["checks"]["historical_range_read_exact"] = False
        with self.assertRaisesRegex(live.S3QualificationBundleError, "required successful checks"):
            live.validate_successful_provider_report(report)

        report = successful_provider_report()
        report["cleanup"]["complete"] = False
        with self.assertRaisesRegex(live.S3QualificationBundleError, "cleanup is incomplete"):
            live.validate_successful_provider_report(report)

        report = successful_provider_report()
        report["non_claims"]["production_accepted"] = True
        with self.assertRaisesRegex(live.S3QualificationBundleError, "non-claim"):
            live.validate_successful_provider_report(report)

    def test_unsuccessful_provider_report_cannot_be_promoted(self) -> None:
        report = successful_provider_report()
        report["ok"] = False
        with self.assertRaisesRegex(live.S3QualificationBundleError, "not successful"):
            live.validate_successful_provider_report(report)

    def test_recursive_redaction_removes_profile_and_environment_secrets(self) -> None:
        evidence = {
            "command": ["aws", "--profile", "private-profile"],
            "stderr_tail": "credential TEST-SECRET via private-profile",
            "nested": {"value": "prefix-TEST-SECRET-suffix"},
        }
        redacted = live.redact_sensitive_data(
            evidence,
            ("private-profile", "TEST-SECRET"),
        )
        rendered = repr(redacted)
        self.assertNotIn("private-profile", rendered)
        self.assertNotIn("TEST-SECRET", rendered)
        self.assertIn("<redacted>", rendered)

    def test_sanitized_invocation_never_persists_profile_name(self) -> None:
        args = argparse.Namespace(
            bucket="qualification-bucket",
            region="ap-southeast-2",
            profile="secret-profile-name",
            prefix="ucof-phase3-live/",
            payload_bytes=65536,
            allow_write=True,
            keep_objects=False,
        )
        evidence = live.sanitized_harness_invocation(args)
        self.assertNotIn("profile", evidence)
        self.assertNotIn("secret-profile-name", repr(evidence))
        self.assertTrue(evidence["profile_named"])
        self.assertEqual(evidence["bucket"], "qualification-bucket")
        self.assertEqual(evidence["payload_bytes"], 65536)

    def test_harness_command_receives_profile_but_bundle_view_does_not(self) -> None:
        args = argparse.Namespace(
            bucket="qualification-bucket",
            region="ap-southeast-2",
            profile="operator-profile",
            prefix="ucof-phase3-live/",
            payload_bytes=4096,
            allow_write=True,
            keep_objects=True,
        )
        command = live.build_harness_command(args, live.ROOT / "target" / "provider.json")
        self.assertIn("--profile", command)
        self.assertIn("operator-profile", command)
        self.assertIn("--allow-write", command)
        self.assertIn("--keep-objects", command)
        evidence = live.sanitized_harness_invocation(args)
        self.assertNotIn("operator-profile", repr(evidence))

    def test_main_binds_hash_and_redacts_fake_harness_report(self) -> None:
        fake_profile = "operator-private-profile"
        fake_secret = "FAKE-LIVE-S3-SECRET-DO-NOT-PERSIST"
        with tempfile.TemporaryDirectory(prefix="ucof-live-s3-wrapper-test-") as directory:
            root = Path(directory)
            harness = root / "fake_harness.py"
            output = root / "bundle.json"
            harness.write_text(
                """#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
p = argparse.ArgumentParser()
p.add_argument('--bucket', required=True)
p.add_argument('--prefix', required=True)
p.add_argument('--payload-bytes', required=True)
p.add_argument('--output', required=True)
p.add_argument('--region')
p.add_argument('--profile')
p.add_argument('--allow-write', action='store_true')
p.add_argument('--keep-objects', action='store_true')
a = p.parse_args()
checks = {
    'bucket_versioning_enabled': True,
    'write_qualification_executed': True,
    'distinct_version_ids_for_repeated_put': True,
    'historical_version_payload_immutable': True,
    'historical_range_read_exact': True,
    'current_read_tracks_latest_version': True,
    'nonexistent_version_rejected': True,
    'historical_version_survives_delete_marker': True,
}
report = {
    'schema': 'ucof-phase3-s3-versioned-source-qualification-v1',
    'ok': True,
    'bucket_versioning_status': 'Enabled',
    'checks': checks,
    'cleanup': {'attempted': True, 'complete': True, 'remaining': []},
    'commands': [{'command': ['aws', '--profile', a.profile], 'stderr_tail': os.environ.get('AWS_SECRET_ACCESS_KEY', '')}],
    'non_claims': {
        'iam_policy_matrix_exhaustive': False,
        'sts_refresh_lifecycle_qualified': False,
        'tls_proxy_policy_qualified': False,
        'provider_scale_limit_qualified': False,
        'network_fault_matrix_qualified': False,
        'production_accepted': False,
    },
}
Path(a.output).write_text(json.dumps(report, indent=2) + '\\n')
print('raw profile=' + str(a.profile))
print('raw secret=' + os.environ.get('AWS_SECRET_ACCESS_KEY', ''))
"""
            )
            argv = [
                "run_phase3_s3_live_qualification.py",
                "--candidate-git-sha",
                CANDIDATE_SHA,
                "--qualification-tool-git-sha",
                TOOL_SHA,
                "--bucket",
                "qualification-bucket",
                "--region",
                "ap-southeast-2",
                "--profile",
                fake_profile,
                "--allow-write",
                "--output",
                str(output),
            ]
            with (
                mock.patch.object(live, "HARNESS", harness),
                mock.patch.object(live, "ROOT", root),
                mock.patch.object(live, "aws_cli_version", return_value={"available": False, "executable": None, "version": None}),
                mock.patch.object(sys, "argv", argv),
                mock.patch.dict(os.environ, {"AWS_SECRET_ACCESS_KEY": fake_secret}, clear=False),
            ):
                self.assertEqual(live.main(), 0)

            bundle = json.loads(output.read_text())
            self.assertTrue(bundle["ok"])
            self.assertEqual(bundle["candidate_git_sha"], CANDIDATE_SHA)
            self.assertEqual(bundle["qualification_tool_git_sha"], TOOL_SHA)
            self.assertRegex(bundle["provider_report_sha256"], r"^[0-9a-f]{64}$")
            self.assertFalse(bundle["harness"]["raw_output_persisted"])
            self.assertGreater(bundle["harness"]["stdout_bytes"], 0)
            rendered = json.dumps(bundle, sort_keys=True)
            self.assertNotIn(fake_profile, rendered)
            self.assertNotIn(fake_secret, rendered)
            self.assertIn("<redacted>", rendered)


if __name__ == "__main__":
    unittest.main()
