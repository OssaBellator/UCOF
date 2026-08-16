#!/usr/bin/env python3
"""Self-tests for non-Cargo guardrails in tools/verify_phase3_local.py."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "tools" / "verify_phase3_local.py"
SPEC = importlib.util.spec_from_file_location("verify_phase3_local", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load local Phase 3 verifier")
verify = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verify)


def git(repo: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=repo,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    return completed.stdout.strip()


def init_repo(path: Path) -> str:
    git(path, "init", "-q")
    git(path, "config", "user.email", "phase3-test@example.invalid")
    git(path, "config", "user.name", "Phase3 Test")
    (path / "seed.txt").write_text("seed\n")
    git(path, "add", "seed.txt")
    git(path, "commit", "-q", "-m", "seed")
    return git(path, "rev-parse", "HEAD")


class FakeRunner:
    def __init__(self) -> None:
        self.static: list[tuple[str, str]] = []
        self.acceptance_sha: str | None = None

    def record_static(self, name: str, detail: str) -> None:
        self.static.append((name, detail))


def synthetic_wiring(root: Path) -> Path:
    parent = (
        root
        / "crates/ucof-experiments/src/immutable_successor/bounded_end_to_end_candidate.rs"
    )
    base = parent.parent / "bounded_end_to_end_candidate"
    base.mkdir(parents=True)
    required = [
        "restart_metadata_compaction.rs",
        "compacted_restart_classification.rs",
        "compacted_source_bound_restart.rs",
        "compacted_private_lifecycle_quota.rs",
        "restart_metadata_compaction_tests.rs",
        "restart_metadata_compaction_retry_tests.rs",
        "restart_metadata_compaction_graph_tests.rs",
        "restart_metadata_compaction_checkpoint_consistency_tests.rs",
        "restart_metadata_compaction_property_tests.rs",
        "restart_metadata_compaction_accounting_tests.rs",
        "compacted_restart_retry_tests.rs",
        "compacted_private_lifecycle_quota_tests.rs",
    ]
    parent.write_text(
        "\n".join(
            f'include!("bounded_end_to_end_candidate/{name}");'
            for name in required
        )
        + "\n"
    )
    for name in required:
        (base / name).write_text("// synthetic wired file\n")

    (base / "restart_metadata_compaction.rs").write_text(
        "\n".join(
            [
                'return Err("compacted nonce filename generation".into());',
                'return Err("compaction retirement context".into());',
                'return Err("compaction source-set context".into());',
                "nonce_record.generation != manifest.generation",
                "for checkpoint in checkpoints.iter().copied() {",
                "validate_compacted_directory_entry_count",
                "ensure_compacted_nonce_commit_directory_headroom",
                "saw_unrecognized_entry",
                "AfterSourceSetPruneBeforeRetirementPrune",
                "AfterPreparedRetirementPruneBeforeTerminalPrune",
                "fn compact_restart_metadata(",
                "for (name, record) in nonce_records {",
                "for (name, record) in &metadata.source_sets {",
                "record.state == EncryptedRetirementState::Prepared",
                "record.state == EncryptedRetirementState::Terminal",
                "for (name, old_checkpoint) in old_checkpoints {",
            ]
        )
        + "\n"
    )
    (base / "linux_durable_nonce_journal.rs").write_text(
        "CompactedNonceJournal::new(self)\n"
        "LinuxNonceJournalError::CompactedAuthority\n"
    )
    (base / "restart_metadata_compaction_tests.rs").write_text(
        "legacy_allocator_accepts_checkpoint_when_ordinary_history_still_matches\n"
        "legacy_allocator_rejects_checkpoint_only_authority_after_prune\n"
    )
    (base / "compacted_source_bound_restart.rs").write_text(
        'return Err("compacted restart manifest/nonce context".into());\n'
    )
    (base / "restart_metadata_compaction_graph_tests.rs").write_text(
        "\n".join(
            [
                "compacted_scan_rejects_authenticated_record_replayed_under_wrong_generation_name",
                "compaction_rejects_authenticated_retirement_from_foreign_journal_context",
                "compaction_rejects_authenticated_source_set_from_foreign_journal_context",
            ]
        )
        + "\n"
    )
    (base / "restart_metadata_compaction_checkpoint_consistency_tests.rs").write_text(
        "\n".join(
            [
                "newer_checkpoint_cannot_mask_older_checkpoint_below_surviving_record",
                "newer_checkpoint_cannot_mask_older_same_generation_record_mismatch",
            ]
        )
        + "\n"
    )
    (base / "restart_metadata_compaction_retry_tests.rs").write_text(
        "\n".join(
            [
                "retry_after_terminal_source_set_prune_before_retirement_prune_completes",
                "retry_after_prepared_retirement_prune_keeps_terminal_authority",
            ]
        )
        + "\n"
    )
    (base / "restart_metadata_compaction_accounting_tests.rs").write_text(
        "\n".join(
            [
                "authenticated_checkpoint_gets_exactly_one_transient_directory_entry_at_ceiling",
                "unrelated_extra_directory_entry_does_not_receive_checkpoint_headroom",
                "compacted_nonce_commit_reserves_one_directory_slot_for_next_checkpoint",
                "checkpoint_does_not_lend_transient_headroom_to_unknown_entry",
            ]
        )
        + "\n"
    )
    (base / "compacted_restart_retry_tests.rs").write_text(
        "\n".join(
            [
                "compacted_restart_survives_pruned_burn_then_publishes_retires_and_reclaims",
                "compacted_destination_exists_burn_can_be_pruned_and_retried",
            ]
        )
        + "\n"
    )
    (base / "compacted_private_lifecycle_quota_tests.rs").write_text(
        "\n".join(
            [
                "checkpointed_source_bound_restart_quota_preserves_pre_side_effect_rejection",
                "checkpointed_publication_quota_rejects_before_nonce_or_backend_side_effects",
            ]
        )
        + "\n"
    )
    return parent


class LocalPhase3VerifierGuardrailTests(unittest.TestCase):
    def test_acceptance_candidate_requires_clean_git_head_and_pins_sha(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            repo = Path(directory)
            sha = init_repo(repo)
            runner = FakeRunner()
            with mock.patch.object(verify, "ROOT", repo):
                verify.verify_acceptance_candidate(runner)
            self.assertEqual(runner.acceptance_sha, sha)
            self.assertEqual(
                runner.static,
                [("Pinned clean acceptance candidate", sha)],
            )

            (repo / "dirty.txt").write_text("dirty\n")
            with mock.patch.object(verify, "ROOT", repo):
                with self.assertRaisesRegex(
                    verify.VerificationFailure, "clean worktree"
                ):
                    verify.verify_acceptance_candidate(FakeRunner())

    def test_acceptance_candidate_must_remain_same_clean_head(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            repo = Path(directory)
            init_repo(repo)
            runner = FakeRunner()
            with mock.patch.object(verify, "ROOT", repo):
                verify.verify_acceptance_candidate(runner)
                verify.verify_acceptance_candidate_unchanged(runner)
                (repo / "dirty.txt").write_text("dirty\n")
                with self.assertRaisesRegex(
                    verify.VerificationFailure, "worktree changed"
                ):
                    verify.verify_acceptance_candidate_unchanged(runner)

    def test_wiring_guard_accepts_complete_synthetic_graph(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            root = Path(directory)
            synthetic_wiring(root)
            runner = FakeRunner()
            with mock.patch.object(verify, "ROOT", root):
                verify.verify_wiring(runner)
            self.assertEqual(
                [name for name, _ in runner.static],
                [
                    "Experiment 0179 legacy allocation guard",
                    "Experiment 0179 checkpoint history consistency",
                    "Experiment 0179 directory headroom",
                    "Experiment 0179 prune order",
                    "Experiment 0179 wiring",
                ],
            )

    def test_wiring_guard_rejects_missing_include(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            root = Path(directory)
            parent = synthetic_wiring(root)
            text = parent.read_text().replace(
                'include!("bounded_end_to_end_candidate/'
                'restart_metadata_compaction_checkpoint_consistency_tests.rs");\n',
                "",
            )
            parent.write_text(text)
            with mock.patch.object(verify, "ROOT", root):
                with self.assertRaisesRegex(verify.VerificationFailure, "not wired"):
                    verify.verify_wiring(FakeRunner())

    def test_wiring_guard_rejects_missing_checkpoint_history_token(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            root = Path(directory)
            synthetic_wiring(root)
            source = (
                root
                / "crates/ucof-experiments/src/immutable_successor/"
                "bounded_end_to_end_candidate/restart_metadata_compaction.rs"
            )
            source.write_text(
                source.read_text().replace(
                    "for checkpoint in checkpoints.iter().copied() {\n", ""
                )
            )
            with mock.patch.object(verify, "ROOT", root):
                with self.assertRaisesRegex(
                    verify.VerificationFailure, "fail-closed coverage"
                ):
                    verify.verify_wiring(FakeRunner())

    def test_wiring_guard_rejects_stale_api_reference(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            root = Path(directory)
            parent = synthetic_wiring(root)
            stale = (
                parent.parent
                / "bounded_end_to_end_candidate/restart_metadata_compaction_tests.rs"
            )
            stale.write_text(stale.read_text() + "let _ = fixture.publication;\n")
            with mock.patch.object(verify, "ROOT", root):
                with self.assertRaisesRegex(
                    verify.VerificationFailure, "stale 0179 API"
                ):
                    verify.verify_wiring(FakeRunner())

    def test_wiring_guard_rejects_obsolete_actions_coordinator(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            root = Path(directory)
            synthetic_wiring(root)
            workflow = (
                root
                / ".github/workflows/one-shot-accept-restart-metadata-compaction.yml"
            )
            workflow.parent.mkdir(parents=True)
            workflow.write_text("name: obsolete\n")
            with mock.patch.object(verify, "ROOT", root):
                with self.assertRaisesRegex(
                    verify.VerificationFailure, "obsolete Actions"
                ):
                    verify.verify_wiring(FakeRunner())

    def test_real_tree_wires_nonce_capacity_regressions(self) -> None:
        parent = (
            ROOT
            / "crates/ucof-experiments/src/immutable_successor/bounded_end_to_end_candidate.rs"
        )
        capacity = (
            parent.parent
            / "bounded_end_to_end_candidate/linux_durable_nonce_journal_capacity_tests.rs"
        )
        self.assertTrue(capacity.is_file())
        self.assertIn(
            'include!("bounded_end_to_end_candidate/'
            'linux_durable_nonce_journal_capacity_tests.rs");',
            parent.read_text(),
        )
        source = capacity.read_text()
        for token in (
            "legacy_commit_rejects_exact_generation_capacity_before_creating_next_record",
            "legacy_commit_rejects_exact_journal_byte_capacity_before_creating_next_record",
            "legacy_commit_rejects_exact_directory_capacity_before_creating_next_record",
            "compaction_restores_ordinary_generation_capacity_for_future_commit",
            "compaction_restores_ordinary_byte_capacity_for_future_commit",
        ):
            self.assertIn(token, source)

    def test_model_only_and_acceptance_are_mutually_exclusive(self) -> None:
        with mock.patch.object(
            sys,
            "argv",
            ["verify_phase3_local.py", "--model-only", "--acceptance"],
        ):
            with self.assertRaises(SystemExit) as context:
                verify.parse_args()
        self.assertEqual(context.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
