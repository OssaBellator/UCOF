#!/usr/bin/env python3
"""Self-tests for tools/verify_phase3_qualification_local.py."""

from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from tools import verify_phase3_qualification_local as qualification


def valid_filesystem_report() -> dict:
    return {
        "schema": qualification.FILESYSTEM_SCHEMA,
        "network_or_distributed_filesystem": False,
        "mechanical_smoke": {
            "file_fsync": True,
            "private_directory_fsync": True,
            "hard_link_no_overwrite": True,
            "publication_directory_fsync": True,
            "published_inode_equal": True,
            "private_unlink_directory_fsync": True,
            "publication_unlink_directory_fsync": True,
        },
    }


class LocalQualificationEvidenceTests(unittest.TestCase):
    def test_valid_filesystem_report_passes(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-qualification-") as directory:
            path = Path(directory) / "filesystem.json"
            expected = valid_filesystem_report()
            path.write_text(json.dumps(expected))
            self.assertEqual(qualification.read_filesystem_report(path), expected)

    def test_wrong_schema_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-qualification-") as directory:
            path = Path(directory) / "filesystem.json"
            report = valid_filesystem_report()
            report["schema"] = "wrong"
            path.write_text(json.dumps(report))
            with self.assertRaisesRegex(
                qualification.QualificationBundleError, "schema mismatch"
            ):
                qualification.read_filesystem_report(path)

    def test_missing_network_classification_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-qualification-") as directory:
            path = Path(directory) / "filesystem.json"
            report = valid_filesystem_report()
            del report["network_or_distributed_filesystem"]
            path.write_text(json.dumps(report))
            with self.assertRaisesRegex(
                qualification.QualificationBundleError, "network/distributed"
            ):
                qualification.read_filesystem_report(path)

    def test_missing_mechanical_check_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-qualification-") as directory:
            path = Path(directory) / "filesystem.json"
            report = valid_filesystem_report()
            del report["mechanical_smoke"]["file_fsync"]
            path.write_text(json.dumps(report))
            with self.assertRaisesRegex(
                qualification.QualificationBundleError, "missing mechanical checks"
            ):
                qualification.read_filesystem_report(path)

    def test_false_mechanical_check_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-qualification-") as directory:
            path = Path(directory) / "filesystem.json"
            report = valid_filesystem_report()
            report["mechanical_smoke"]["hard_link_no_overwrite"] = False
            path.write_text(json.dumps(report))
            with self.assertRaisesRegex(
                qualification.QualificationBundleError, "failed mechanical checks"
            ):
                qualification.read_filesystem_report(path)


if __name__ == "__main__":
    unittest.main()
