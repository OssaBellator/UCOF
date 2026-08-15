#!/usr/bin/env python3
"""Self-tests for tools/verify_phase3_deployment_preflight.py."""

from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from tools import test_run_phase3_deployment_preflight_bundle as bundle_tests
from tools import verify_phase3_deployment_preflight as preflight


def load_tests(loader: unittest.TestLoader, tests: unittest.TestSuite, pattern: str | None):
    """Keep the shareable deployment-bundle tests acceptance-loaded with preflight tests."""
    tests.addTests(loader.loadTestsFromModule(bundle_tests))
    return tests


def valid_filesystem_report() -> dict:
    return {
        "schema": preflight.FILESYSTEM_SCHEMA,
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


def valid_key_report() -> dict:
    return {
        "schema": preflight.KEY_SCHEMA,
        "ok": True,
        "secret_material_reported": False,
        "claims": {
            "exact_width": True,
            "regular_file": True,
            "effective_uid_owned": True,
            "single_hard_link": True,
            "no_group_or_world_permissions": True,
            "parent_directory_effective_uid_owned": True,
            "parent_directory_not_group_or_world_writable": True,
            "distinct_files": True,
            "distinct_secret_bytes": True,
        },
    }


def valid_storage_report(required_inodes: int = 7) -> dict:
    return {
        "schema": preflight.STORAGE_SCHEMA,
        "ok": True,
        "reserved": False,
        "race_free": False,
        "observation": {
            "bytes_ok": True,
            "inodes_ok": True,
            "required_inodes": required_inodes,
        },
    }


class DeploymentPreflightEvidenceTests(unittest.TestCase):
    def test_valid_child_reports_pass(self) -> None:
        preflight.validate_filesystem_report(valid_filesystem_report())
        preflight.validate_key_report(valid_key_report())
        preflight.validate_storage_report(valid_storage_report())

    def test_child_schema_mismatch_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-deployment-preflight-") as directory:
            path = Path(directory) / "report.json"
            path.write_text(json.dumps({"schema": "wrong"}))
            with self.assertRaisesRegex(
                preflight.DeploymentPreflightError, "schema mismatch"
            ):
                preflight.read_json_report(path, preflight.KEY_SCHEMA)

    def test_filesystem_missing_or_false_mechanical_evidence_fails(self) -> None:
        report = valid_filesystem_report()
        del report["mechanical_smoke"]["file_fsync"]
        with self.assertRaisesRegex(
            preflight.DeploymentPreflightError, "missing mechanical checks"
        ):
            preflight.validate_filesystem_report(report)

        report = valid_filesystem_report()
        report["mechanical_smoke"]["published_inode_equal"] = False
        with self.assertRaisesRegex(
            preflight.DeploymentPreflightError, "failed mechanical checks"
        ):
            preflight.validate_filesystem_report(report)

    def test_key_report_requires_private_parent_claims(self) -> None:
        report = valid_key_report()
        report["claims"]["parent_directory_not_group_or_world_writable"] = False
        with self.assertRaisesRegex(
            preflight.DeploymentPreflightError, "missing required true claims"
        ):
            preflight.validate_key_report(report)

    def test_key_report_must_not_claim_secret_output(self) -> None:
        report = valid_key_report()
        report["secret_material_reported"] = True
        with self.assertRaisesRegex(
            preflight.DeploymentPreflightError, "secret material"
        ):
            preflight.validate_key_report(report)

    def test_storage_report_preserves_reservation_nonclaims(self) -> None:
        report = valid_storage_report()
        report["reserved"] = True
        with self.assertRaisesRegex(
            preflight.DeploymentPreflightError, "reservation/race non-claims"
        ):
            preflight.validate_storage_report(report)

    def test_storage_report_requires_both_headroom_dimensions(self) -> None:
        report = valid_storage_report()
        report["observation"]["inodes_ok"] = False
        with self.assertRaisesRegex(
            preflight.DeploymentPreflightError, "byte/inode headroom"
        ):
            preflight.validate_storage_report(report)

    def test_inode_requirement_is_derived_from_spill_run_limit(self) -> None:
        plan, effective = preflight.resolve_inode_requirement(1, 0)
        self.assertEqual(plan.required_additional_inodes, 7)
        self.assertEqual(effective, 7)

        plan, effective = preflight.resolve_inode_requirement(10, 0)
        self.assertEqual(plan.required_additional_inodes, 13)
        self.assertEqual(effective, 13)

    def test_operator_inode_floor_can_only_raise_requirement(self) -> None:
        plan, effective = preflight.resolve_inode_requirement(2, 20)
        self.assertEqual(plan.required_additional_inodes, 7)
        self.assertEqual(effective, 20)

        _, effective = preflight.resolve_inode_requirement(10, 2)
        self.assertEqual(effective, 13)

    def test_invalid_inode_planner_inputs_fail_closed(self) -> None:
        with self.assertRaisesRegex(
            preflight.DeploymentPreflightError, "max initial runs must be positive"
        ):
            preflight.resolve_inode_requirement(0, 0)
        with self.assertRaisesRegex(
            preflight.DeploymentPreflightError, "required inodes must be nonnegative"
        ):
            preflight.resolve_inode_requirement(1, -1)


if __name__ == "__main__":
    unittest.main()
