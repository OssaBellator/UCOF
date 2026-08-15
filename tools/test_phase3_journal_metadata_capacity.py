#!/usr/bin/env python3
"""Static guards for Phase 3 journal metadata capacity enforcement."""

from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "crates/ucof-experiments/src/immutable_successor/bounded_end_to_end_candidate"
PARENT = BASE.parent / "bounded_end_to_end_candidate.rs"


class JournalMetadataCapacityGuards(unittest.TestCase):
    def test_shared_helper_and_regressions_are_wired(self) -> None:
        parent = PARENT.read_text()
        self.assertIn(
            'include!("bounded_end_to_end_candidate/journal_metadata_capacity.rs");',
            parent,
        )
        self.assertIn(
            'include!("bounded_end_to_end_candidate/journal_metadata_capacity_tests.rs");',
            parent,
        )
        self.assertIn(
            'include!("bounded_end_to_end_candidate/journal_metadata_authority_capacity_tests.rs");',
            parent,
        )
        self.assertIn(
            'include!("bounded_end_to_end_candidate/journal_metadata_primitive_capacity_tests.rs");',
            parent,
        )

    def test_source_set_writer_guards_before_create_new(self) -> None:
        text = (BASE / "restart_source_set_authority.rs").read_text()
        body = text.split("fn persist_restart_source_set_authority", 1)[1]
        guard = (
            'require_linux_nonce_journal_metadata_slots('
            'journal, 1, "restart source-set authority")?;'
        )
        self.assertIn(guard, body)
        self.assertLess(body.find(guard), body.find(".create_new(true)"))

    def test_ordinary_stage_manifest_guards_before_stage_and_manifest_create(self) -> None:
        text = (BASE / "linux_encrypted_stage_restart.rs").read_text()
        body = text.split("fn persist_sorted_encrypted_spill_restart_stage", 1)[1]
        body = body.split("\nfn ", 1)[0]
        guard = (
            'require_linux_nonce_journal_metadata_slots('
            'journal, 1, "encrypted stage manifest")'
        )
        self.assertEqual(body.count(guard), 2)
        first_guard = body.find(guard)
        first_create = body.find(".create_new(true)")
        second_guard = body.find(guard, first_guard + len(guard))
        second_create = body.find(".create_new(true)", first_create + 1)
        self.assertGreaterEqual(first_guard, 0)
        self.assertGreaterEqual(first_create, 0)
        self.assertGreaterEqual(second_guard, 0)
        self.assertGreaterEqual(second_create, 0)
        self.assertLess(first_guard, first_create)
        self.assertLess(first_create, second_guard)
        self.assertLess(second_guard, second_create)

    def test_ordinary_retirement_writer_guards_before_create_new(self) -> None:
        text = (BASE / "encrypted_restart_retirement.rs").read_text()
        body = text.split("fn persist_encrypted_retirement_record", 1)[1]
        body = body.split("\nfn ", 1)[0]
        guard = (
            'require_linux_nonce_journal_metadata_slots('
            'journal, 1, "encrypted retirement")?;'
        )
        self.assertIn(guard, body)
        self.assertLess(body.find(guard), body.find(".create_new(true)"))

    def test_ordinary_primitive_full_and_exact_cap_regressions_are_wired(self) -> None:
        text = (BASE / "journal_metadata_primitive_capacity_tests.rs").read_text()
        for token in (
            "ordinary_stage_manifest_rejects_full_journal_before_durable_stage_creation",
            "full journal must reject stage-manifest persistence before durable stage creation",
            "one free journal slot must permit exact stage-manifest persistence",
            "ordinary_prepared_retirement_respects_configured_journal_entry_capacity",
            "full journal must reject Prepared retirement authority",
            "one free journal slot must permit Prepared retirement authority",
            "manifest reclamation must free the slot needed by Terminal authority",
        ):
            self.assertIn(token, text)

    def test_compacted_publication_uses_shared_multi_slot_guard(self) -> None:
        text = (BASE / "compacted_source_bound_restart.rs").read_text()
        self.assertIn(
            'require_linux_nonce_journal_metadata_slots(journal, 2, "compacted publication")',
            text,
        )
        self.assertIn(
            'require_linux_nonce_journal_metadata_slots(journal, 1, "compacted retirement")',
            text,
        )
        prepare = text.split("fn prepare_compacted_encrypted_restart_retirement", 1)[1]
        self.assertLess(
            prepare.find("ensure_compacted_restart_prepared_directory_headroom(journal)?;"),
            prepare.find("persist_encrypted_retirement_record(journal, record)?;"),
        )

    def test_checkpoint_creation_keeps_its_separate_transient_rule(self) -> None:
        text = (BASE / "restart_metadata_compaction.rs").read_text()
        body = text.split("fn persist_nonce_compaction_checkpoint", 1)[1]
        body = body.split("\nfn ", 1)[0]
        self.assertNotIn("require_linux_nonce_journal_metadata_slots", body)


if __name__ == "__main__":
    unittest.main()
