#!/usr/bin/env python3
"""Self-tests for tools/record_phase3_local_acceptance.py."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "tools" / "record_phase3_local_acceptance.py"
SPEC = importlib.util.spec_from_file_location("record_phase3_local_acceptance", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load local acceptance recorder")
record = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(record)


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


def successful_report(sha: str, fuzz_targets: list[str] | None = None) -> dict:
    targets = fuzz_targets or ["restart_metadata", "immutable_parser"]
    checks = []
    for name in sorted(record.REQUIRED_CHECKS):
        item = {"name": name, "status": "pass", "command": None, "seconds": 0.0}
        if name == "List fuzz targets":
            item["output"] = "\n".join(targets) + "\n"
        checks.append(item)
    for target in targets:
        checks.append(
            {
                "name": f"Fuzz smoke {target}",
                "status": "pass",
                "command": ["cargo", "fuzz", "run", target],
                "seconds": 0.1,
            }
        )
    return {
        "schema": "ucof-phase3-local-verification-v1",
        "mode": "acceptance",
        "offline": True,
        "started_utc": "2026-08-15T00:00:00+00:00",
        "completed_utc": "2026-08-15T00:01:00+00:00",
        "ok": True,
        "failure": None,
        "git_sha": sha,
        "git_branch": "phase-3/test",
        "dirty_worktree": False,
        "python": "3.test",
        "rustc": "rustc test",
        "cargo": "cargo test",
        "skipped": [],
        "checks": checks,
    }


class AcceptanceRecorderTests(unittest.TestCase):
    def test_valid_report_requires_every_listed_fuzz_target(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-acceptance-recorder-") as directory:
            repo = Path(directory)
            sha = init_repo(repo)
            report = successful_report(sha, ["a", "b", "c"])
            with mock.patch.object(record, "ROOT", repo):
                actual_sha, checks, fuzz_targets = record.validate_report(report)
            self.assertEqual(actual_sha, sha)
            self.assertEqual(fuzz_targets, ["a", "b", "c"])
            self.assertGreater(len(checks), len(record.REQUIRED_CHECKS))

    def test_missing_fuzz_smoke_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-acceptance-recorder-") as directory:
            repo = Path(directory)
            sha = init_repo(repo)
            report = successful_report(sha, ["a", "b"])
            report["checks"] = [
                item for item in report["checks"] if item.get("name") != "Fuzz smoke b"
            ]
            with mock.patch.object(record, "ROOT", repo):
                with self.assertRaisesRegex(record.RecordError, "did not smoke every"):
                    record.validate_report(report)

    def test_dirty_current_worktree_fails_even_if_report_says_clean(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-acceptance-recorder-") as directory:
            repo = Path(directory)
            sha = init_repo(repo)
            report = successful_report(sha)
            (repo / "dirty.txt").write_text("dirty\n")
            with mock.patch.object(record, "ROOT", repo):
                with self.assertRaisesRegex(record.RecordError, "worktree must remain clean"):
                    record.validate_report(report)

    def test_stale_sha_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-acceptance-recorder-") as directory:
            repo = Path(directory)
            init_repo(repo)
            report = successful_report("0" * 40)
            with mock.patch.object(record, "ROOT", repo):
                with self.assertRaisesRegex(record.RecordError, "does not match current HEAD"):
                    record.validate_report(report)

    def test_duplicate_check_name_fails(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-acceptance-recorder-") as directory:
            repo = Path(directory)
            sha = init_repo(repo)
            report = successful_report(sha)
            report["checks"].append(dict(report["checks"][0]))
            with mock.patch.object(record, "ROOT", repo):
                with self.assertRaisesRegex(record.RecordError, "duplicate check"):
                    record.validate_report(report)

    def test_normalized_record_never_promotes_skips_or_dirty_state(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ucof-acceptance-recorder-") as directory:
            repo = Path(directory)
            sha = init_repo(repo)
            report = successful_report(sha)
            with mock.patch.object(record, "ROOT", repo):
                actual_sha, checks, fuzz_targets = record.validate_report(report)
            normalized = record.normalized_record(
                report,
                actual_sha,
                checks,
                fuzz_targets,
                "2026-08-15T00:02:00+00:00",
            )
            self.assertTrue(normalized["ok"])
            self.assertFalse(normalized["dirty_worktree"])
            self.assertEqual(normalized["skipped"], [])
            self.assertEqual(normalized["git_sha"], sha)
            self.assertEqual(normalized["fuzz_targets"], fuzz_targets)


if __name__ == "__main__":
    unittest.main()
