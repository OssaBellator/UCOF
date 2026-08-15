#!/usr/bin/env python3
"""Self-tests for tools/run_phase3_s3_live_qualification.py."""

from __future__ import annotations

import argparse
import unittest

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


if __name__ == "__main__":
    unittest.main()
