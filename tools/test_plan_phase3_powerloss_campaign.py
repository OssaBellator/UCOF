#!/usr/bin/env python3
"""Self-tests for tools/plan_phase3_powerloss_campaign.py."""

from __future__ import annotations

import unittest

from tools import plan_phase3_powerloss_campaign as plan


class Phase3PowerlossPlanTests(unittest.TestCase):
    def test_case_ids_are_unique_and_complete(self) -> None:
        payload = plan.build_plan()
        ids = [case["case_id"] for case in payload["cases"]]
        self.assertEqual(len(ids), 14)
        self.assertEqual(len(ids), len(set(ids)))
        self.assertEqual(ids, [case.case_id for case in plan.CASES])

    def test_each_case_has_reboot_and_retry_contract(self) -> None:
        payload = plan.build_plan()
        for case in payload["cases"]:
            with self.subTest(case=case["case_id"]):
                self.assertTrue(case["subsystem"])
                self.assertTrue(case["cut"])
                self.assertTrue(case["precondition"])
                self.assertTrue(case["durable_operations_completed"])
                self.assertTrue(case["deliberately_not_completed"])
                self.assertTrue(case["reboot_observation"])
                self.assertTrue(case["required_retry"])
                self.assertTrue(case["safety_invariant"])

    def test_campaign_spans_compaction_publication_and_retirement(self) -> None:
        subsystems = {case.subsystem for case in plan.CASES}
        self.assertEqual(
            subsystems,
            {"0179-metadata-compaction", "durable-publication", "restart-retirement"},
        )

    def test_platform_metadata_captures_storage_stack_identity(self) -> None:
        required = set(plan.build_plan()["required_platform_metadata"])
        for field in (
            "kernel_version",
            "filesystem_type",
            "mount_options",
            "block_device_or_volume_type",
            "storage_controller_or_virtualization_layer",
            "write_cache_policy_if_known",
            "host_or_cloud_provider",
            "test_image_or_snapshot_identifier",
            "ucof_git_sha",
        ):
            self.assertIn(field, required)

    def test_plan_requires_external_destructive_execution(self) -> None:
        payload = plan.build_plan()
        self.assertTrue(payload["destructive_external_execution_required"])
        self.assertIn(
            "process-crash-only evidence is sufficient",
            payload["non_claims"],
        )


if __name__ == "__main__":
    unittest.main()
