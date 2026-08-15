#!/usr/bin/env python3
"""Package-style self-tests for Phase 3 local preflight tools."""

from __future__ import annotations

import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from tools import apply_phase3_0179_directory_headroom_fix as headroom_patch
from tools import check_phase3_storage_headroom as headroom
from tools import qualify_phase3_filesystem as fsq
from tools import qualify_phase3_key_material as keyq
from tools import test_apply_phase3_0179_directory_headroom_fix as headroom_patch_tests
from tools import test_plan_phase3_private_inodes as inode_planner_tests
from tools import test_restart_metadata_headroom_model as headroom_model_tests
from tools import test_verify_phase3_deployment_preflight as deployment_tests
from tools import test_verify_phase3_qualification_local as qualification_tests


class FakeStatvfs:
    f_frsize = 4096
    f_bsize = 4096
    f_bavail = 100
    f_blocks = 200
    f_bfree = 120
    f_favail = 20
    f_files = 40
    f_ffree = 30


def write_key(path: Path, byte: int, mode: int = 0o600) -> None:
    path.write_bytes(bytes([byte]) * keyq.KEY_BYTES)
    os.chmod(path, mode)


def load_tests(loader: unittest.TestLoader, tests: unittest.TestSuite, pattern: str | None):
    """Include bundle/evidence/headroom/patch/inode validators in the acceptance module."""
    tests.addTests(loader.loadTestsFromModule(deployment_tests))
    tests.addTests(loader.loadTestsFromModule(qualification_tests))
    tests.addTests(loader.loadTestsFromModule(headroom_model_tests))
    tests.addTests(loader.loadTestsFromModule(headroom_patch_tests))
    tests.addTests(loader.loadTestsFromModule(inode_planner_tests))
    return tests


class Experiment0179SourceFixTests(unittest.TestCase):
    def test_directory_headroom_source_fix_is_complete(self) -> None:
        source = headroom_patch.DEFAULT_SOURCE
        self.assertTrue(source.is_file(), f"missing 0179 source: {source}")
        state = headroom_patch.inspect_source(source.read_text())
        self.assertTrue(
            state.complete,
            "Experiment 0179 directory-headroom source fixes are still pending; "
            "run tools/apply_phase3_0179_directory_headroom_fix.py --apply from a clean checkout",
        )


class FilesystemHarnessTests(unittest.TestCase):
    def test_mountinfo_unescape(self) -> None:
        self.assertEqual(
            fsq.unescape_mountinfo(r"/tmp/a\040b\011c\012d\134e"),
            "/tmp/a b\tc\nd\\e",
        )

    def test_current_mount_and_capacity_resolve(self) -> None:
        if fsq.sys.platform != "linux":
            self.skipTest("Linux-only mountinfo test")
        root = Path.cwd()
        mount = fsq.resolve_mount(root)
        self.assertTrue(Path(mount.mount_point).is_absolute())
        self.assertTrue(mount.filesystem_type)
        capacity = fsq.capacity(root)
        self.assertGreater(capacity.block_size, 0)
        self.assertGreaterEqual(capacity.total_bytes, capacity.free_bytes)

    def test_mechanical_smoke_is_no_overwrite_and_cleans_files(self) -> None:
        if fsq.sys.platform != "linux":
            self.skipTest("Linux-only filesystem mechanics")
        with tempfile.TemporaryDirectory(prefix="ucof-fs-harness-test-") as directory:
            root = Path(directory)
            evidence = fsq.mechanical_smoke(root)
            self.assertTrue(evidence["file_fsync"])
            self.assertTrue(evidence["hard_link_no_overwrite"])
            self.assertTrue(evidence["published_inode_equal"])
            self.assertEqual(list((root / "private").iterdir()), [])
            self.assertEqual(list((root / "publication").iterdir()), [])

    def test_network_filesystems_are_explicit(self) -> None:
        for name in ("nfs", "nfs4", "cifs", "ceph", "9p"):
            self.assertIn(name, fsq.NETWORK_FILESYSTEMS)
        self.assertNotIn("ext4", fsq.NETWORK_FILESYSTEMS)
        self.assertNotIn("xfs", fsq.NETWORK_FILESYSTEMS)


class KeyMaterialTests(unittest.TestCase):
    def test_distinct_private_exact_keys_pass_without_secret_output(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-key-preflight-") as directory:
            root = Path(directory)
            aes = root / "aes.key"
            hmac = root / "hmac.key"
            write_key(aes, 0x11)
            write_key(hmac, 0x22)
            report = keyq.qualify(aes, hmac)
            self.assertTrue(report["ok"])
            self.assertFalse(report["secret_material_reported"])
            self.assertEqual([entry["bytes"] for entry in report["keys"]], [32, 32])
            self.assertTrue(report["claims"]["parent_directory_effective_uid_owned"])
            self.assertTrue(
                report["claims"]["parent_directory_not_group_or_world_writable"]
            )
            self.assertFalse(report["non_claims"]["ancestor_path_pinning_qualified"])

    def test_wrong_width_permissions_same_bytes_and_hardlink_fail(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-key-preflight-") as directory:
            root = Path(directory)
            aes = root / "aes.key"
            hmac = root / "hmac.key"

            aes.write_bytes(b"x" * 31)
            os.chmod(aes, 0o600)
            write_key(hmac, 0x22)
            with self.assertRaisesRegex(keyq.KeyMaterialError, "exactly 32 bytes"):
                keyq.qualify(aes, hmac)

            aes.unlink()
            write_key(aes, 0x11, 0o640)
            with self.assertRaisesRegex(keyq.KeyMaterialError, "group/world"):
                keyq.qualify(aes, hmac)

            aes.unlink()
            write_key(aes, 0x33)
            hmac.unlink()
            write_key(hmac, 0x33)
            with self.assertRaisesRegex(keyq.KeyMaterialError, "material must be distinct"):
                keyq.qualify(aes, hmac)

            hmac.unlink()
            os.link(aes, hmac)
            with self.assertRaisesRegex(keyq.KeyMaterialError, "exactly one hard link"):
                keyq.qualify(aes, hmac)

    def test_group_or_world_writable_parent_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-key-preflight-") as directory:
            root = Path(directory)
            aes = root / "aes.key"
            hmac = root / "hmac.key"
            write_key(aes, 0x11)
            write_key(hmac, 0x22)
            os.chmod(root, 0o770)
            try:
                with self.assertRaisesRegex(
                    keyq.KeyMaterialError,
                    "parent directory must not be group/world writable",
                ):
                    keyq.qualify(aes, hmac)
            finally:
                os.chmod(root, 0o700)

    def test_symlink_fails_when_no_follow_is_available(self) -> None:
        if not hasattr(os, "O_NOFOLLOW"):
            self.skipTest("O_NOFOLLOW is unavailable")
        with tempfile.TemporaryDirectory(prefix="ucof-key-preflight-") as directory:
            root = Path(directory)
            real_aes = root / "real-aes.key"
            aes = root / "aes.key"
            hmac = root / "hmac.key"
            write_key(real_aes, 0x55)
            aes.symlink_to(real_aes)
            write_key(hmac, 0x66)
            with self.assertRaises(keyq.KeyMaterialError):
                keyq.qualify(aes, hmac)


class StorageHeadroomTests(unittest.TestCase):
    def test_exact_boundary_and_one_unit_short_cases(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-headroom-") as directory:
            path = Path(directory)
            with mock.patch.object(headroom.os, "statvfs", return_value=FakeStatvfs()):
                exact = headroom.observe(path, 300 * 1024, 10, 100 * 1024, 10)
                byte_short = headroom.observe(path, 300 * 1024 + 1, 10, 100 * 1024, 10)
                inode_short = headroom.observe(path, 300 * 1024, 11, 100 * 1024, 10)
            self.assertTrue(exact.bytes_ok and exact.inodes_ok)
            self.assertFalse(byte_short.bytes_ok)
            self.assertTrue(byte_short.inodes_ok)
            self.assertTrue(inode_short.bytes_ok)
            self.assertFalse(inode_short.inodes_ok)

    def test_negative_input_and_non_directory_fail(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-headroom-") as directory:
            path = Path(directory)
            with self.assertRaisesRegex(headroom.HeadroomError, "must be nonnegative"):
                headroom.observe(path, -1, 0, 0, 0)
            file_path = path / "file"
            file_path.write_text("x")
            with self.assertRaisesRegex(headroom.HeadroomError, "not a directory"):
                headroom.observe(file_path, 1, 0, 0, 0)


if __name__ == "__main__":
    unittest.main()
