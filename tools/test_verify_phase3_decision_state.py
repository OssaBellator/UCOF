#!/usr/bin/env python3
"""Self-tests for tools/verify_phase3_decision_state.py."""

from __future__ import annotations

import copy
import json
import unittest

from tools import verify_phase3_decision_state as validator


def base_state() -> dict:
    return json.loads(validator.DEFAULT_STATE.read_text())


class Phase3DecisionStateTests(unittest.TestCase):
    def test_current_all_unselected_state_is_valid(self) -> None:
        summary = validator.validate(base_state())
        self.assertEqual(summary["selected"], [])
        self.assertEqual(summary["unselected"], validator.EXPECTED)
        self.assertFalse(summary["all_selected"])

    def test_unselected_entry_cannot_smuggle_selection(self) -> None:
        payload = base_state()
        payload["decisions"]["D2"]["selection"] = "implicit geometry"
        with self.assertRaisesRegex(
            validator.DecisionStateError, "unselected decision has selection"
        ):
            validator.validate(payload)

    def test_selected_entry_requires_full_review_fields(self) -> None:
        payload = base_state()
        payload["decisions"]["D4"]["status"] = "selected"
        payload["decisions"]["D4"]["selection"] = "candidate policy"
        with self.assertRaisesRegex(validator.DecisionStateError, "missing"):
            validator.validate(payload)

    def test_selected_entry_with_review_fields_is_valid(self) -> None:
        payload = base_state()
        entry = payload["decisions"]["D7"]
        entry.update(
            {
                "status": "selected",
                "selection": "canonical semantic inputs only",
                "rationale": "reviewed maintainer decision",
                "normative_locations": ["FCP-0003 determinism section"],
                "corpus_impact": "regenerate determinism corpus",
                "review_reference": "maintainer-review-placeholder",
            }
        )
        summary = validator.validate(payload)
        self.assertEqual(summary["selected"], ["D7"])
        self.assertFalse(summary["all_selected"])

    def test_selected_entry_requires_normative_location(self) -> None:
        payload = base_state()
        entry = payload["decisions"]["D1"]
        entry.update(
            {
                "status": "selected",
                "selection": "historical only",
                "rationale": "reviewed",
                "corpus_impact": "historical corpus only",
                "review_reference": "review",
            }
        )
        with self.assertRaisesRegex(validator.DecisionStateError, "no normative locations"):
            validator.validate(payload)

    def test_decision_ids_and_order_are_fixed(self) -> None:
        payload = base_state()
        decisions = payload["decisions"]
        payload["decisions"] = {
            "D2": decisions["D2"],
            "D1": decisions["D1"],
            **{key: decisions[key] for key in validator.EXPECTED[2:]},
        }
        with self.assertRaisesRegex(validator.DecisionStateError, "canonical order"):
            validator.validate(payload)

    def test_ledger_cannot_claim_normative_effect(self) -> None:
        payload = base_state()
        payload["normative_effect"] = True
        with self.assertRaisesRegex(validator.DecisionStateError, "normative effect"):
            validator.validate(payload)


if __name__ == "__main__":
    unittest.main()
