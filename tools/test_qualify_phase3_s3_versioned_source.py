#!/usr/bin/env python3
"""Self-tests for tools/qualify_phase3_s3_versioned_source.py."""

from __future__ import annotations

import hashlib
import unittest

from tools import qualify_phase3_s3_versioned_source as s3q


class FakeCli:
    def __init__(self, listings: list[object | None]) -> None:
        self.listings = list(listings)
        self.calls: list[list[str]] = []

    def run_json(self, name: str, args: list[str], *, allow_failure: bool = False):
        self.calls.append(args)
        if args[:2] == ["s3api", "list-object-versions"]:
            return self.listings.pop(0)
        if args[:2] == ["s3api", "delete-object"]:
            return {}
        raise AssertionError(f"unexpected fake CLI call: {args}")


class S3QualificationHarnessTests(unittest.TestCase):
    def test_prefix_is_relative_and_normalized(self) -> None:
        self.assertEqual(s3q.normalize_prefix("ucof-test"), "ucof-test/")
        self.assertEqual(s3q.normalize_prefix("ucof-test/"), "ucof-test/")
        for invalid in ("", "/absolute", "../escape", "a/../b"):
            with self.subTest(invalid=invalid), self.assertRaises(s3q.S3QualificationError):
                s3q.normalize_prefix(invalid)

    def test_deterministic_payload_is_repeatable_and_variant_separated(self) -> None:
        first = s3q.deterministic_payload(4097, 1)
        again = s3q.deterministic_payload(4097, 1)
        second = s3q.deterministic_payload(4097, 2)
        self.assertEqual(first, again)
        self.assertNotEqual(first, second)
        self.assertEqual(len(first), 4097)
        self.assertNotEqual(hashlib.sha256(first).digest(), hashlib.sha256(second).digest())

    def test_exact_versions_filters_other_prefix_matches(self) -> None:
        payload = {
            "Versions": [
                {"Key": "test/exact", "VersionId": "v1"},
                {"Key": "test/exact-extra", "VersionId": "other"},
            ],
            "DeleteMarkers": [
                {"Key": "test/exact", "VersionId": "d1"},
                {"Key": "test/exact/child", "VersionId": "child"},
            ],
        }
        self.assertEqual(
            s3q.exact_versions(payload, "test/exact"),
            [("version", "v1"), ("delete-marker", "d1")],
        )

    def test_cleanup_deletes_only_exact_key_versions_and_markers(self) -> None:
        initial = {
            "Versions": [
                {"Key": "safe/key", "VersionId": "v1"},
                {"Key": "safe/key-neighbor", "VersionId": "never-delete"},
            ],
            "DeleteMarkers": [{"Key": "safe/key", "VersionId": "d1"}],
        }
        final = {"Versions": [], "DeleteMarkers": []}
        cli = FakeCli([initial, final])
        result = s3q.cleanup_exact_key(cli, "bucket", "safe/key")
        self.assertTrue(result["complete"])
        delete_calls = [call for call in cli.calls if call[:2] == ["s3api", "delete-object"]]
        self.assertEqual(len(delete_calls), 2)
        joined = "\n".join(" ".join(call) for call in delete_calls)
        self.assertIn("v1", joined)
        self.assertIn("d1", joined)
        self.assertNotIn("never-delete", joined)

    def test_cleanup_reports_incomplete_when_exact_version_remains(self) -> None:
        initial = {"Versions": [{"Key": "safe/key", "VersionId": "v1"}]}
        final = {"Versions": [{"Key": "safe/key", "VersionId": "v1"}]}
        cli = FakeCli([initial, final])
        result = s3q.cleanup_exact_key(cli, "bucket", "safe/key")
        self.assertFalse(result["complete"])
        self.assertEqual(result["remaining"], [{"kind": "version", "version_id": "v1"}])

    def test_sensitive_argv_values_are_redacted(self) -> None:
        command = [
            "aws",
            "--access-key-id",
            "AKIAEXAMPLE",
            "--session-token",
            "TOKEN",
            "s3api",
            "list-buckets",
        ]
        redacted = s3q.sanitize_command(command)
        self.assertNotIn("AKIAEXAMPLE", redacted)
        self.assertNotIn("TOKEN", redacted)
        self.assertEqual(redacted[2], "<redacted>")
        self.assertEqual(redacted[4], "<redacted>")

    def test_payload_size_must_be_positive(self) -> None:
        with self.assertRaisesRegex(s3q.S3QualificationError, "positive"):
            s3q.deterministic_payload(0, 1)


if __name__ == "__main__":
    unittest.main()
