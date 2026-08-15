#!/usr/bin/env python3
"""Self-tests for tools/verify_phase3_powerloss_results.py."""

from __future__ import annotations

import unittest

from tools import plan_phase3_powerloss_campaign as plan
from tools import verify_phase3_powerloss_results as verify

CANDIDATE_SHA = "67beca9ce5d1242c5df839c9ef2d7b1ce5a8b774"
OTHER_SHA = "0" * 40


def valid_payload() -> dict:
    platform = {
        field: f"value-for-{field}"
        for field in plan.build_plan()["required_platform_metadata"]
    }
    platform["ucof_git_sha"] = CANDIDATE_SHA
    return {
        "schema": verify.SCHEMA,
        "plan_schema": plan.SCHEMA,
        "platform": platform,
        "campaign": {
            "operator": "qualification-operator",
            "started_utc": "2026-08-15T00:00:00Z",
            "completed_utc": "2026-08-15T01:00:00Z",
            "evidence_location": "external://qualification/evidence",
        },
        "cases": [
            {
                "case_id": case.case_id,
                "status": "pass",
                "cut_execution_reference": f"cut-{case.case_id}",
                "reboot_observation": "observed expected reboot state",
                "retry_result": "retry completed safely",
            }
            for case in plan.CASES
        ],
    }


class Phase3PowerlossResultValidatorTests(unittest.TestCase):
    def test_complete_all_pass_payload_is_valid(self) -> None:
        summary = verify.validate(valid_payload())
        self.assertTrue(summary["ok"])
        self.assertEqual(summary["case_count"], len(plan.CASES))
        self.assertEqual(summary["failed_cases"], [])
        self.assertEqual(summary["ucof_git_sha"], CANDIDATE_SHA)

    def test_exact_expected_candidate_sha_is_accepted(self) -> None:
        summary = verify.validate(valid_payload(), expected_git_sha=CANDIDATE_SHA)
        self.assertTrue(summary["ok"])
        self.assertEqual(summary["ucof_git_sha"], CANDIDATE_SHA)

    def test_mismatched_expected_candidate_sha_is_rejected(self) -> None:
        with self.assertRaisesRegex(verify.PowerlossResultError, "git SHA mismatch"):
            verify.validate(valid_payload(), expected_git_sha=OTHER_SHA)

    def test_expected_candidate_sha_must_be_canonical_full_lowercase_hex(self) -> None:
        for malformed in (
            CANDIDATE_SHA[:-1],
            CANDIDATE_SHA.upper(),
            "g" * 40,
            "refs/heads/phase-3/restart-metadata-compaction",
        ):
            with self.subTest(malformed=malformed), self.assertRaisesRegex(
                verify.PowerlossResultError,
                "40 lowercase hexadecimal",
            ):
                verify.validate(valid_payload(), expected_git_sha=malformed)

    def test_one_explicit_failure_is_well_formed_but_not_green(self) -> None:
        payload = valid_payload()
        payload["cases"][3]["status"] = "fail"
        summary = verify.validate(payload)
        self.assertFalse(summary["ok"])
        self.assertEqual(summary["failed_cases"], [plan.CASES[3].case_id])

    def test_skipped_case_is_rejected_not_treated_as_partial_success(self) -> None:
        payload = valid_payload()
        payload["cases"][0]["status"] = "skipped"
        with self.assertRaisesRegex(verify.PowerlossResultError, "pass or fail"):
            verify.validate(payload)

    def test_missing_case_or_reordered_case_is_rejected(self) -> None:
        payload = valid_payload()
        payload["cases"].pop()
        with self.assertRaisesRegex(verify.PowerlossResultError, "canonical plan order"):
            verify.validate(payload)

        payload = valid_payload()
        payload["cases"][0], payload["cases"][1] = payload["cases"][1], payload["cases"][0]
        with self.assertRaisesRegex(verify.PowerlossResultError, "canonical plan order"):
            verify.validate(payload)

    def test_platform_identity_must_be_complete(self) -> None:
        payload = valid_payload()
        payload["platform"]["mount_options"] = ""
        with self.assertRaisesRegex(verify.PowerlossResultError, "mount_options"):
            verify.validate(payload)

    def test_each_case_requires_cut_observation_and_retry_evidence(self) -> None:
        for field, fragment in (
            ("cut_execution_reference", "cut execution reference"),
            ("reboot_observation", "reboot observation"),
            ("retry_result", "retry result"),
        ):
            payload = valid_payload()
            payload["cases"][5][field] = ""
            with self.subTest(field=field), self.assertRaisesRegex(
                verify.PowerlossResultError, fragment
            ):
                verify.validate(payload)

    def test_campaign_metadata_is_mandatory(self) -> None:
        payload = valid_payload()
        payload["campaign"]["evidence_location"] = None
        with self.assertRaisesRegex(verify.PowerlossResultError, "evidence_location"):
            verify.validate(payload)

    def test_schema_and_plan_schema_are_pinned(self) -> None:
        payload = valid_payload()
        payload["schema"] = "wrong"
        with self.assertRaisesRegex(verify.PowerlossResultError, "schema"):
            verify.validate(payload)
        payload = valid_payload()
        payload["plan_schema"] = "wrong"
        with self.assertRaisesRegex(verify.PowerlossResultError, "plan schema"):
            verify.validate(payload)


if __name__ == "__main__":
    unittest.main()
