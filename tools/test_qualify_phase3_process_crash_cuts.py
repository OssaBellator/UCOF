#!/usr/bin/env python3
"""Self-tests for tools/qualify_phase3_process_crash_cuts.py."""

from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile
import unittest

from tools import qualify_phase3_process_crash_cuts as crashq


@unittest.skipUnless(sys.platform == "linux", "Linux-only process-crash qualification")
class ProcessCrashCutHarnessTests(unittest.TestCase):
    def test_full_harness_exercises_four_restart_cuts_and_cleans_scratch(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-crash-cut-test-") as directory:
            scratch = Path(directory)
            before = set(scratch.iterdir())
            report = crashq.run_harness(scratch)
            self.assertTrue(report["ok"])
            self.assertEqual(len(report["cases"]), 4)
            self.assertEqual(
                [case["name"] for case in report["cases"]],
                [
                    "checkpoint-file-sync-before-dir-sync",
                    "publication-link-before-dir-sync",
                    "retirement-after-stage-unlink",
                    "retirement-after-both-unlinks",
                ],
            )
            self.assertTrue(all(case["child_returncode"] == crashq.CRASH_EXIT for case in report["cases"]))
            self.assertEqual(set(scratch.iterdir()), before)
            self.assertFalse(report["non_claims"]["physical_power_loss_simulated"])
            self.assertFalse(report["non_claims"]["filesystem_crash_consistency_proven"])

    def test_checkpoint_case_requires_expected_crash_code(self) -> None:
        completed = type("Completed", (), {"returncode": 0, "stderr": "", "stdout": ""})()
        with self.assertRaisesRegex(crashq.CrashCutError, "did not reach deliberate crash cut"):
            crashq.require_crash(completed, "test")

    def test_write_fsync_is_create_new(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-crash-write-") as directory:
            path = Path(directory) / "record.bin"
            crashq.write_fsync(path, b"one")
            with self.assertRaises(FileExistsError):
                crashq.write_fsync(path, b"two")
            self.assertEqual(path.read_bytes(), b"one")

    def test_retirement_stage_cut_restart_is_idempotent_for_absent_stage(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-crash-retire-") as directory:
            root = Path(directory)
            for name in ("private", "journal", "publication"):
                (root / name).mkdir()
            stage, manifest = crashq.prepare_retirement_fixture(root)
            stage.unlink()
            # Restart cleanup must tolerate the already-absent first target.
            if stage.exists():
                self.fail("stage unexpectedly reappeared")
            if manifest.exists():
                manifest.unlink()
            crashq.fsync_directory(root / "private")
            crashq.fsync_directory(root / "journal")
            self.assertFalse(stage.exists())
            self.assertFalse(manifest.exists())


if __name__ == "__main__":
    unittest.main()
