#!/usr/bin/env python3
"""Self-tests for tools/verify_phase3_journal_entry_lifecycle_model.py."""

from __future__ import annotations

import unittest

from tools import verify_phase3_journal_entry_lifecycle_model as model


class JournalEntryLifecycleModelTests(unittest.TestCase):
    def test_one_slot_short_rejects_before_generation_advance(self) -> None:
        state = model.State(max_entries=5)
        with self.assertRaisesRegex(
            model.JournalEntryModelError,
            "compacted publication retirement directory headroom",
        ):
            state.publish()
        self.assertEqual(state.entries(), 4)
        self.assertEqual(state.durable_generation, 1)

    def test_exact_two_slots_reach_terminal_and_reclaim(self) -> None:
        state = model.State(max_entries=6)
        state.publish()
        self.assertEqual(state.entries(), 5)
        state.prepare_retirement()
        self.assertEqual(state.entries(), 6)
        state.terminalize()
        self.assertEqual(state.entries(), 6)
        state.compact_terminal_lineage()
        self.assertEqual(state.entries(), 1)
        self.assertEqual(state.checkpoint_generation, 2)

    def test_prepared_recheck_observes_intervening_capacity_use(self) -> None:
        state = model.State(max_entries=7)
        state.publish()
        base_entries = state.entries
        state.entries = lambda: base_entries() + 2  # type: ignore[method-assign]
        with self.assertRaisesRegex(
            model.JournalEntryModelError,
            "compacted retirement Prepared directory headroom",
        ):
            state.prepare_retirement()
        self.assertEqual(state.prepared, 0)

    def test_terminal_reuses_manifest_slot(self) -> None:
        state = model.State(max_entries=6)
        state.publish()
        state.prepare_retirement()
        before = state.entries()
        state.terminalize()
        self.assertEqual(before, 6)
        self.assertEqual(state.entries(), 6)
        self.assertEqual(state.manifests, 0)
        self.assertEqual(state.terminal, 1)

    def test_fixed_cases_remain_complete(self) -> None:
        self.assertEqual(model.run_fixed_cases(), 3)


if __name__ == "__main__":
    unittest.main()
