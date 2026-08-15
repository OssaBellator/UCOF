#!/usr/bin/env python3
"""Self-tests for tools/check_phase3_storage_headroom.py."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "tools" / "check_phase3_storage_headroom.py"
SPEC = importlib.util.spec_from_file_location("check_phase3_storage_headroom", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load storage headroom preflight")
headroom = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = headroom
SPEC.loader.exec_module(headroom)


class FakeStatvfs:
    f_frsize = 4096
    f_bsize = 4096
    f_bavail = 100
    f_favail = 20


class StorageHeadroomTests(unittest.TestCase):
    def test_exact_requirement_and_reserve_pass(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-headroom-") as directory:
            with mock.patch.object(headroom.os, "statvfs", return_value=FakeStatvfs()):
                result = headroom.observe(Path(directory), 300 * 1024, 10, 100 * 1024, 10)
            self.assertTrue(result.bytes_ok)
            self.assertTrue(result.inodes_ok)
            self.assertEqual(result.byte_headroom_after_requirement, 100 * 1024)
            self.assertEqual(result.inode_headroom_after_requirement, 10)

    def test_one_byte_short_reserve_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-headroom-") as directory:
            with mock.patch.object(headroom.os, "statvfs", return_value=FakeStatvfs()):
                result = headroom.observe(Path(directory), 300 * 1024 + 1, 10, 100 * 1024, 10)
            self.assertFalse(result.bytes_ok)
            self.assertTrue(result.inodes_ok)

    def test_one_inode_short_reserve_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-headroom-") as directory:
            with mock.patch.object(headroom.os, "statvfs", return_value=FakeStatvfs()):
                result = headroom.observe(Path(directory), 300 * 1024, 11, 100 * 1024, 10)
            self.assertTrue(result.bytes_ok)
            self.assertFalse(result.inodes_ok)

    def test_negative_inputs_fail(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-headroom-") as directory:
            with self.assertRaisesRegex(headroom.HeadroomError, "must be nonnegative"):
                headroom.observe(Path(directory), -1, 0, 0, 0)

    def test_non_directory_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-headroom-") as directory:
            file_path = Path(directory) / "not-a-directory"
            file_path.write_text("x")
            with self.assertRaisesRegex(headroom.HeadroomError, "not a directory"):
                headroom.observe(file_path, 1, 0, 0, 0)


if __name__ == "__main__":
    unittest.main()
