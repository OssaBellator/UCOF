#!/usr/bin/env python3
"""Self-tests for tools/apply_phase3_0179_directory_headroom_fix.py."""

from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from tools import apply_phase3_0179_directory_headroom_fix as patcher


PREFIX = "// prefix\n"
SUFFIX = "// suffix\n"


def old_source() -> str:
    return PREFIX + patcher.OLD_CEILING + "\n" + patcher.OLD_POST_REPLAY + SUFFIX


def fixed_source() -> str:
    return PREFIX + patcher.NEW_CEILING + "\n" + patcher.NEW_POST_REPLAY + SUFFIX


class DirectoryHeadroomPatchTests(unittest.TestCase):
    def test_old_source_is_recognized_as_untouched(self) -> None:
        state = patcher.inspect_source(old_source())
        self.assertTrue(state.untouched)
        self.assertFalse(state.complete)

    def test_apply_transforms_only_the_two_exact_regions(self) -> None:
        before = old_source()
        after, state = patcher.apply_text(before)
        self.assertEqual(after, fixed_source())
        self.assertTrue(state.complete)
        self.assertEqual(after.count(PREFIX), 1)
        self.assertEqual(after.count(SUFFIX), 1)

    def test_apply_is_idempotent_after_complete_fix(self) -> None:
        before = fixed_source()
        after, state = patcher.apply_text(before)
        self.assertEqual(after, before)
        self.assertTrue(state.complete)

    def test_partial_application_fails_closed(self) -> None:
        partial = PREFIX + patcher.NEW_CEILING + patcher.OLD_POST_REPLAY + SUFFIX
        with self.assertRaisesRegex(patcher.PatchError, "partially applied"):
            patcher.inspect_source(partial)

    def test_drifted_ceiling_shape_fails_closed(self) -> None:
        drifted = old_source().replace(".checked_add(1)", ".checked_add(2)")
        with self.assertRaisesRegex(patcher.PatchError, "ceiling source shape"):
            patcher.inspect_source(drifted)

    def test_drifted_replay_shape_fails_closed(self) -> None:
        drifted = old_source().replace(
            'return Err("compacted nonce generation/lease gap".into());',
            'return Err("changed".into());',
        )
        with self.assertRaisesRegex(patcher.PatchError, "post-replay source shape"):
            patcher.inspect_source(drifted)

    def test_cli_apply_updates_file_then_check_passes(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-0179-headroom-patch-") as directory:
            path = Path(directory) / "restart_metadata_compaction.rs"
            path.write_text(old_source())
            updated, state = patcher.apply_text(path.read_text())
            path.write_text(updated)
            self.assertTrue(state.complete)
            self.assertTrue(patcher.inspect_source(path.read_text()).complete)


if __name__ == "__main__":
    unittest.main()
