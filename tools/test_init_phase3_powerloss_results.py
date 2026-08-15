#!/usr/bin/env python3
"""Self-tests for tools/init_phase3_powerloss_results.py."""

from __future__ import annotations

import unittest

from tools import init_phase3_powerloss_results as init
from tools import plan_phase3_powerloss_campaign as plan
from tools import verify_phase3_powerloss_results as verify

CANDIDATE_SHA = "67beca9ce5d1242c5df839c9ef2d7b1ce5a8b774"


class Phase3PowerlossResultTemplateTests(unittest.TestCase):
    def test_template_contains_every_canonical_case(self) -> None:
        payload = init.template()
        self.assertEqual(payload["schema"], verify.SCHEMA)
        self.assertEqual(payload["plan_schema"], plan.SCHEMA)
        self.assertEqual(
            [entry["case_id"] for entry in payload["cases"]],
            [case.case_id for case in plan.CASES],
        )

    def test_template_is_deliberately_not_valid_final_evidence(self) -> None:
        payload = init.template()
        with self.assertRaises(verify.PowerlossResultError):
            verify.validate(payload)
        self.assertTrue(all(entry["status"] == "unexecuted" for entry in payload["cases"]))

    def test_template_contains_every_required_platform_field(self) -> None:
        payload = init.template()
        self.assertEqual(
            list(payload["platform"]),
            plan.build_plan()["required_platform_metadata"],
        )
        self.assertTrue(all(value == "" for value in payload["platform"].values()))

    def test_template_can_prebind_exact_candidate_sha_without_becoming_evidence(self) -> None:
        payload = init.template(CANDIDATE_SHA)
        self.assertEqual(payload["platform"]["ucof_git_sha"], CANDIDATE_SHA)
        self.assertTrue(all(entry["status"] == "unexecuted" for entry in payload["cases"]))
        with self.assertRaises(verify.PowerlossResultError):
            verify.validate(payload, expected_git_sha=CANDIDATE_SHA)

    def test_template_rejects_malformed_candidate_sha(self) -> None:
        for malformed in (CANDIDATE_SHA[:-1], CANDIDATE_SHA.upper(), "g" * 40):
            with self.subTest(malformed=malformed), self.assertRaisesRegex(
                verify.PowerlossResultError,
                "40 lowercase hexadecimal",
            ):
                init.template(malformed)

    def test_template_has_no_fake_operator_or_evidence_reference(self) -> None:
        payload = init.template(CANDIDATE_SHA)
        for field in ("operator", "started_utc", "completed_utc", "evidence_location"):
            self.assertEqual(payload["campaign"][field], "")


if __name__ == "__main__":
    unittest.main()
