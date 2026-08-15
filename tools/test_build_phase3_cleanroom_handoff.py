#!/usr/bin/env python3
"""Self-tests for tools/build_phase3_cleanroom_handoff.py."""

from __future__ import annotations

import copy
import json
from pathlib import Path
import tempfile
import unittest
import zipfile
from unittest import mock

from tools import build_phase3_cleanroom_handoff as builder


class CleanroomHandoffTests(unittest.TestCase):
    def test_implementation_paths_and_source_suffixes_are_rejected(self) -> None:
        with self.assertRaisesRegex(builder.CleanroomError, "forbidden"):
            builder.reject_implementation_path(Path("tools/example.md"))
        with self.assertRaisesRegex(builder.CleanroomError, "forbidden"):
            builder.reject_implementation_path(Path("docs/example.py"))
        with self.assertRaisesRegex(builder.CleanroomError, "forbidden"):
            builder.reject_implementation_path(Path("docs/review/internal.md"))

    def test_public_docs_and_vector_paths_are_allowed(self) -> None:
        builder.reject_implementation_path(Path("docs/FCP-0003.md"))
        builder.reject_implementation_path(Path("tests/vectors/exp-0003/example.bin"))

    def test_current_unselected_decisions_block_final_bundle(self) -> None:
        with self.assertRaisesRegex(builder.CleanroomError, "not all selected"):
            builder.decision_summary(require_all_selected=True)
        summary = builder.decision_summary(require_all_selected=False)
        self.assertFalse(summary["all_selected"])

    def test_manifest_is_sorted_and_hashes_exact_bytes(self) -> None:
        with tempfile.TemporaryDirectory(dir=builder.ROOT) as directory:
            root = Path(directory)
            first = root / "b.txt"
            second = root / "a.txt"
            first.write_bytes(b"bbb")
            second.write_bytes(b"aaa")
            with mock.patch.object(
                builder,
                "decision_summary",
                return_value={"selected": [], "unselected": builder.decisions.EXPECTED, "all_selected": False},
            ):
                manifest = builder.build_manifest([first, second], require_all_selected=False)
            paths = [record["path"] for record in manifest["files"]]
            self.assertEqual(paths, sorted(paths))
            records = {record["path"]: record for record in manifest["files"]}
            self.assertEqual(records[builder.repo_relative(first).as_posix()]["bytes"], 3)
            self.assertEqual(records[builder.repo_relative(second).as_posix()]["bytes"], 3)

    def test_zip_is_deterministic_and_self_verifying(self) -> None:
        with tempfile.TemporaryDirectory(dir=builder.ROOT) as directory:
            root = Path(directory)
            doc = root / "normative.txt"
            vector = root / "vector.bin"
            doc.write_text("normative\n")
            vector.write_bytes(bytes(range(16)))
            decision_state = {
                "selected": builder.decisions.EXPECTED,
                "unselected": [],
                "all_selected": True,
            }
            with mock.patch.object(builder, "decision_summary", return_value=decision_state), mock.patch.object(
                builder, "git_sha", return_value="abc123"
            ):
                manifest = builder.build_manifest([doc, vector], require_all_selected=True)
                first = root / "one.zip"
                second = root / "two.zip"
                builder.write_zip(first, [doc, vector], manifest)
                builder.write_zip(second, [doc, vector], manifest)
                builder.verify_zip(first, manifest)
                builder.verify_zip(second, manifest)
            self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_archive_manifest_tamper_is_detected(self) -> None:
        with tempfile.TemporaryDirectory(dir=builder.ROOT) as directory:
            root = Path(directory)
            doc = root / "normative.txt"
            doc.write_text("normative\n")
            with mock.patch.object(
                builder,
                "decision_summary",
                return_value={"selected": [], "unselected": builder.decisions.EXPECTED, "all_selected": False},
            ):
                manifest = builder.build_manifest([doc], require_all_selected=False)
            archive = root / "bundle.zip"
            builder.write_zip(archive, [doc], manifest)
            with zipfile.ZipFile(archive, "a") as handle:
                # A duplicate manifest entry changes member order/content and must fail.
                handle.writestr(builder.MANIFEST_NAME, b"{}")
            with self.assertRaises(builder.CleanroomError):
                builder.verify_zip(archive, manifest)

    def test_collect_inputs_deduplicates_and_sorts(self) -> None:
        with tempfile.TemporaryDirectory(dir=builder.ROOT) as directory:
            root = Path(directory)
            a = root / "a.txt"
            b = root / "b.bin"
            a.write_text("a")
            b.write_bytes(b"b")
            files = builder.collect_inputs([root, a])
            self.assertEqual(files, [a, b])


if __name__ == "__main__":
    unittest.main()
