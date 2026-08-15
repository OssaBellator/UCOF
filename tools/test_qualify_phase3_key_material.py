#!/usr/bin/env python3
"""Self-tests for tools/qualify_phase3_key_material.py."""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "tools" / "qualify_phase3_key_material.py"
SPEC = importlib.util.spec_from_file_location("qualify_phase3_key_material", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load key-material preflight")
keyq = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = keyq
SPEC.loader.exec_module(keyq)


def write_key(path: Path, byte: int, mode: int = 0o600) -> None:
    path.write_bytes(bytes([byte]) * keyq.KEY_BYTES)
    os.chmod(path, mode)


class KeyMaterialQualificationTests(unittest.TestCase):
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
            self.assertNotIn("11" * 32, str(report))
            self.assertNotIn("22" * 32, str(report))

    def test_wrong_width_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-key-preflight-") as directory:
            root = Path(directory)
            aes = root / "aes.key"
            hmac = root / "hmac.key"
            aes.write_bytes(b"x" * 31)
            os.chmod(aes, 0o600)
            write_key(hmac, 0x22)
            with self.assertRaisesRegex(keyq.KeyMaterialError, "exactly 32 bytes"):
                keyq.qualify(aes, hmac)

    def test_group_or_world_permissions_fail(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-key-preflight-") as directory:
            root = Path(directory)
            aes = root / "aes.key"
            hmac = root / "hmac.key"
            write_key(aes, 0x11, 0o640)
            write_key(hmac, 0x22)
            with self.assertRaisesRegex(keyq.KeyMaterialError, "group/world"):
                keyq.qualify(aes, hmac)

    def test_same_secret_bytes_fail_even_in_distinct_files(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-key-preflight-") as directory:
            root = Path(directory)
            aes = root / "aes.key"
            hmac = root / "hmac.key"
            write_key(aes, 0x33)
            write_key(hmac, 0x33)
            with self.assertRaisesRegex(keyq.KeyMaterialError, "material must be distinct"):
                keyq.qualify(aes, hmac)

    def test_hard_linked_keys_fail(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-key-preflight-") as directory:
            root = Path(directory)
            aes = root / "aes.key"
            hmac = root / "hmac.key"
            write_key(aes, 0x44)
            os.link(aes, hmac)
            with self.assertRaisesRegex(keyq.KeyMaterialError, "exactly one hard link"):
                keyq.qualify(aes, hmac)

    def test_symlink_key_fails_when_no_follow_is_available(self) -> None:
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


if __name__ == "__main__":
    unittest.main()
