#!/usr/bin/env python3
"""Self-tests for tools/qualify_phase3_filesystem.py."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "tools" / "qualify_phase3_filesystem.py"
SPEC = importlib.util.spec_from_file_location("qualify_phase3_filesystem", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load filesystem qualification harness")
fsq = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fsq)


class FilesystemQualificationTests(unittest.TestCase):
    def test_mountinfo_unescape(self) -> None:
        self.assertEqual(
            fsq.unescape_mountinfo(r"/tmp/a\040b\011c\012d\134e"),
            "/tmp/a b\tc\nd\\e",
        )

    def test_current_directory_mount_resolves(self) -> None:
        if fsq.sys.platform != "linux":
            self.skipTest("Linux-only mountinfo test")
        mount = fsq.resolve_mount(ROOT)
        self.assertTrue(Path(mount.mount_point).is_absolute())
        self.assertTrue(mount.filesystem_type)
        self.assertTrue(mount.major_minor)

    def test_capacity_is_nonnegative(self) -> None:
        capacity = fsq.capacity(ROOT)
        self.assertGreater(capacity.block_size, 0)
        self.assertGreaterEqual(capacity.total_bytes, capacity.free_bytes)
        self.assertGreaterEqual(capacity.free_bytes, capacity.available_bytes)
        self.assertGreaterEqual(capacity.total_inodes, capacity.free_inodes)

    def test_mechanical_smoke_cleans_its_files(self) -> None:
        if fsq.sys.platform != "linux":
            self.skipTest("Linux-only filesystem mechanics")
        with tempfile.TemporaryDirectory(prefix="ucof-fs-harness-test-") as directory:
            root = Path(directory)
            evidence = fsq.mechanical_smoke(root)
            self.assertTrue(evidence["file_fsync"])
            self.assertTrue(evidence["hard_link_no_overwrite"])
            self.assertTrue(evidence["published_inode_equal"])
            self.assertEqual(sorted(path.name for path in root.iterdir()), ["private", "publication"])
            self.assertEqual(list((root / "private").iterdir()), [])
            self.assertEqual(list((root / "publication").iterdir()), [])

    def test_known_network_filesystems_are_not_silent_local_equivalents(self) -> None:
        for name in ("nfs", "nfs4", "cifs", "ceph", "9p"):
            self.assertIn(name, fsq.NETWORK_FILESYSTEMS)
        self.assertNotIn("ext4", fsq.NETWORK_FILESYSTEMS)
        self.assertNotIn("xfs", fsq.NETWORK_FILESYSTEMS)


if __name__ == "__main__":
    unittest.main()
