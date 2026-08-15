#!/usr/bin/env python3
"""Self-tests for tools/compare_phase3_cleanroom_corpus.py."""

from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from tools import compare_phase3_cleanroom_corpus as compare


class CleanroomCorpusComparatorTests(unittest.TestCase):
    def test_exact_corpus_matches(self) -> None:
        with tempfile.TemporaryDirectory() as left_dir, tempfile.TemporaryDirectory() as right_dir:
            left = Path(left_dir)
            right = Path(right_dir)
            (left / "nested").mkdir()
            (right / "nested").mkdir()
            (left / "a.bin").write_bytes(b"alpha")
            (right / "a.bin").write_bytes(b"alpha")
            (left / "nested/b.bin").write_bytes(b"beta")
            (right / "nested/b.bin").write_bytes(b"beta")
            result = compare.compare(compare.inventory(left), compare.inventory(right))
            self.assertTrue(result["ok"])
            self.assertEqual(result["matched"], ["a.bin", "nested/b.bin"])
            self.assertEqual(result["missing"], [])
            self.assertEqual(result["extra"], [])
            self.assertEqual(result["mismatched"], [])

    def test_missing_extra_and_mismatch_are_distinct(self) -> None:
        with tempfile.TemporaryDirectory() as left_dir, tempfile.TemporaryDirectory() as right_dir:
            left = Path(left_dir)
            right = Path(right_dir)
            (left / "same").write_bytes(b"same")
            (right / "same").write_bytes(b"same")
            (left / "missing").write_bytes(b"reference-only")
            (right / "extra").write_bytes(b"candidate-only")
            (left / "changed").write_bytes(b"reference")
            (right / "changed").write_bytes(b"candidate")
            result = compare.compare(compare.inventory(left), compare.inventory(right))
            self.assertFalse(result["ok"])
            self.assertEqual(result["missing"], ["missing"])
            self.assertEqual(result["extra"], ["extra"])
            self.assertEqual([item["path"] for item in result["mismatched"]], ["changed"])
            self.assertEqual(result["matched"], ["same"])

    def test_equal_length_different_bytes_still_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as left_dir, tempfile.TemporaryDirectory() as right_dir:
            left = Path(left_dir)
            right = Path(right_dir)
            (left / "x.bin").write_bytes(b"abcd")
            (right / "x.bin").write_bytes(b"abce")
            result = compare.compare(compare.inventory(left), compare.inventory(right))
            self.assertFalse(result["ok"])
            mismatch = result["mismatched"][0]
            self.assertEqual(mismatch["reference"]["bytes"], 4)
            self.assertEqual(mismatch["candidate"]["bytes"], 4)
            self.assertNotEqual(mismatch["reference"]["sha256"], mismatch["candidate"]["sha256"])

    def test_symlink_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target.bin"
            target.write_bytes(b"target")
            link = root / "link.bin"
            try:
                link.symlink_to(target)
            except (OSError, NotImplementedError):
                self.skipTest("symlink unavailable")
            with self.assertRaisesRegex(compare.CorpusCompareError, "symlink"):
                compare.inventory(root)

    def test_empty_or_non_directory_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(compare.CorpusCompareError, "empty"):
                compare.inventory(root)
            file_path = root / "file"
            file_path.write_text("x")
            with self.assertRaisesRegex(compare.CorpusCompareError, "not a directory"):
                compare.inventory(file_path)


if __name__ == "__main__":
    unittest.main()
