#!/usr/bin/env python3
"""Static acceptance guards for Experiment 0179 publication journal headroom."""

from __future__ import annotations

from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
BASE = (
    ROOT
    / "crates/ucof-experiments/src/immutable_successor/bounded_end_to_end_candidate"
)
PARENT = BASE.parent / "bounded_end_to_end_candidate.rs"
SOURCE = BASE / "compacted_source_bound_restart.rs"
TESTS = BASE / "compacted_publication_headroom_tests.rs"


class Experiment0179PublicationHeadroomGuards(unittest.TestCase):
    def test_regressions_are_wired(self) -> None:
        parent = PARENT.read_text()
        self.assertIn(
            'include!("bounded_end_to_end_candidate/compacted_publication_headroom_tests.rs");',
            parent,
        )
        tests = TESTS.read_text()
        for token in (
            "compacted_publication_one_journal_slot_short_rejects_before_nonce_or_backend_side_effects",
            "compacted_publication_exact_two_journal_slots_reaches_terminal_and_reclaims",
            "compacted_prepared_retirement_rechecks_journal_headroom_after_publication",
        ):
            self.assertIn(token, tests)

    def test_publication_reserves_nonce_and_prepared_slots_before_fresh_lease(self) -> None:
        source = SOURCE.read_text()
        fn = source.split(
            "fn stage_and_publish_compacted_source_bound_encrypted_tree_restart", 1
        )[1].split("fn prepare_compacted_encrypted_restart_retirement", 1)[0]
        guard = fn.find("ensure_compacted_restart_publication_directory_headroom(journal)?;")
        prepare = fn.find("prepare_compacted_source_bound_encrypted_tree_restart(")
        self.assertGreaterEqual(guard, 0)
        self.assertGreaterEqual(prepare, 0)
        self.assertLess(guard, prepare)
        self.assertIn(
            'require_linux_nonce_journal_metadata_slots(journal, 2, "compacted publication")',
            source,
        )
        self.assertIn(
            '"compacted publication retirement directory headroom".to_owned()',
            source,
        )

    def test_prepared_retirement_rechecks_before_persist(self) -> None:
        source = SOURCE.read_text()
        fn = source.split("fn prepare_compacted_encrypted_restart_retirement", 1)[1]
        guard = fn.find("ensure_compacted_restart_prepared_directory_headroom(journal)?;")
        persist = fn.find("persist_encrypted_retirement_record(journal, record)?;")
        self.assertGreaterEqual(guard, 0)
        self.assertGreaterEqual(persist, 0)
        self.assertLess(guard, persist)
        self.assertIn(
            'require_linux_nonce_journal_metadata_slots(journal, 1, "compacted retirement")',
            source,
        )
        self.assertIn(
            '"compacted retirement Prepared directory headroom".to_owned()',
            source,
        )


if __name__ == "__main__":
    unittest.main()
