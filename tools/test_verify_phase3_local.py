#!/usr/bin/env python3
"""Self-tests for non-Cargo guardrails in tools/verify_phase3_local.py."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
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

    def record_static(self, name: str, detail: str) -> None:
        self.static.append((name, detail))


def synthetic_wiring(root: Path) -> Path:
    parent = root / "crates/ucof-experiments/src/immutable_successor/bounded_end_to_end_candidate.rs"
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
        "restart_metadata_compaction_property_tests.rs",
        "restart_metadata_compaction_accounting_tests.rs",
        "compacted_restart_retry_tests.rs",
        "compacted_private_lifecycle_quota_tests.rs",
    ]
    parent.write_text(
        "\n".join(
            f'include!("bounded_end_to_end_candidate/{name}");' for name in required
        )
        + "\n"
    )
    for name in required:
        (base / name).write_text("// synthetic wired file\n")
    return parent


class LocalPhase3VerifierGuardrailTests(unittest.TestCase):
    def test_acceptance_candidate_requires_clean_git_head(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            repo = Path(directory)
            sha = init_repo(repo)
            runner = FakeRunner()
            with mock.patch.object(verify, "ROOT", repo):
                verify.verify_acceptance_candidate(runner)
            self.assertEqual(runner.static, [("Pinned clean acceptance candidate", sha)])

            (repo / "dirty.txt").write_text("dirty\n")
            with mock.patch.object(verify, "ROOT", repo):
                with self.assertRaisesRegex(verify.VerificationFailure, "clean worktree"):
                    verify.verify_acceptance_candidate(FakeRunner())

    def test_wiring_guard_accepts_complete_synthetic_graph(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            root = Path(directory)
            synthetic_wiring(root)
            runner = FakeRunner()
            with mock.patch.object(verify, "ROOT", root):
                verify.verify_wiring(runner)
            self.assertEqual(len(runner.static), 1)
            self.assertEqual(runner.static[0][0], "Experiment 0179 wiring")

    def test_wiring_guard_rejects_missing_include(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            root = Path(directory)
            parent = synthetic_wiring(root)
            text = parent.read_text().replace(
                'include!("bounded_end_to_end_candidate/restart_metadata_compaction.rs");\n',
                "",
            )
            parent.write_text(text)
            with mock.patch.object(verify, "ROOT", root):
                with self.assertRaisesRegex(verify.VerificationFailure, "not wired"):
                    verify.verify_wiring(FakeRunner())

    def test_wiring_guard_rejects_stale_api_reference(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            root = Path(directory)
            parent = synthetic_wiring(root)
            stale = parent.parent / "bounded_end_to_end_candidate/restart_metadata_compaction_tests.rs"
            stale.write_text("let _ = fixture.publication;\n")
            with mock.patch.object(verify, "ROOT", root):
                with self.assertRaisesRegex(verify.VerificationFailure, "stale 0179 API"):
                    verify.verify_wiring(FakeRunner())

    def test_wiring_guard_rejects_obsolete_actions_coordinator(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-local-verify-") as directory:
            root = Path(directory)
            synthetic_wiring(root)
            workflow = root / ".github/workflows/one-shot-accept-restart-metadata-compaction.yml"
            workflow.parent.mkdir(parents=True)
            workflow.write_text("name: obsolete\n")
            with mock.patch.object(verify, "ROOT", root):
                with self.assertRaisesRegex(verify.VerificationFailure, "obsolete Actions"):
                    verify.verify_wiring(FakeRunner())


if __name__ == "__main__":
    unittest.main()
