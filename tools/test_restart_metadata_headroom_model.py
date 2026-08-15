#!/usr/bin/env python3
"""Self-tests for tools/verify_restart_metadata_headroom_model.py."""

from __future__ import annotations

import unittest

from tools import verify_restart_metadata_headroom_model as model


class RestartMetadataHeadroomModelTests(unittest.TestCase):
    def test_at_or_below_limit_does_not_need_checkpoint(self) -> None:
        model.validate_transient_checkpoint_headroom(
            max_entries=4,
            entries=4,
            latest_checkpoint_generation=None,
            durable_generation=7,
        )

    def test_current_checkpoint_lends_exactly_one_transient_slot(self) -> None:
        model.validate_transient_checkpoint_headroom(
            max_entries=4,
            entries=5,
            latest_checkpoint_generation=7,
            durable_generation=7,
        )

    def test_stale_checkpoint_cannot_lend_transient_slot(self) -> None:
        with self.assertRaisesRegex(model.HeadroomModelError, "stale checkpoint"):
            model.validate_transient_checkpoint_headroom(
                max_entries=4,
                entries=5,
                latest_checkpoint_generation=6,
                durable_generation=7,
            )

    def test_unrecognized_entry_cannot_borrow_transient_slot(self) -> None:
        with self.assertRaisesRegex(model.HeadroomModelError, "unrecognized entry"):
            model.validate_transient_checkpoint_headroom(
                max_entries=4,
                entries=5,
                latest_checkpoint_generation=7,
                durable_generation=7,
                unrecognized_entries=1,
            )

    def test_two_entries_over_limit_always_fail(self) -> None:
        with self.assertRaisesRegex(model.HeadroomModelError, "directory entry limit"):
            model.validate_transient_checkpoint_headroom(
                max_entries=4,
                entries=6,
                latest_checkpoint_generation=8,
                durable_generation=8,
            )

    def test_fixed_cli_cases_remain_complete(self) -> None:
        self.assertEqual(model.run_fixed_cases(), 6)


if __name__ == "__main__":
    unittest.main()
