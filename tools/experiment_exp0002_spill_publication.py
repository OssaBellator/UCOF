#!/usr/bin/env python3
"""Failure-safe private spill lifecycle and create-new atomic publication."""

from __future__ import annotations

import os
import shutil
import tempfile
from dataclasses import dataclass
from pathlib import Path

import experiment_exp0002_spill_page_emission as pages
import experiment_exp0002_staged_spill_merge as staged

OBJECTS = 4_097
RUN_ENTRIES = 257
MAX_OPEN = 4
STAGE_PREFIX = ".ucof-stage-"


class InjectedFailure(RuntimeError):
    pass


class DiskBudgetExceeded(RuntimeError):
    pass


@dataclass(frozen=True)
class ExpectedArtifact:
    sha256: str
    size: int
    root: pages.PageRef


@dataclass(frozen=True)
class PublishReport:
    failpoint: str | None
    final_exists: bool
    final_valid: bool
    outcome: str
    peak_disk_bytes: int
    merge_passes: int
    peak_open_files: int


def directory_bytes(directory: Path) -> int:
    return sum(
        path.stat().st_size
        for path in directory.rglob("*")
        if path.is_file()
    )


def check_private(directory: Path) -> None:
    if directory.stat().st_mode & 0o077:
        raise AssertionError("staging directory is not private")
    for path in directory.rglob("*"):
        if path.is_file() and path.stat().st_mode & 0o077:
            raise AssertionError(f"spill file is not private: {path.name}")


def make_private_file(path: Path) -> None:
    path.chmod(0o600)


def check_budget(directory: Path, maximum: int) -> int:
    used = directory_bytes(directory)
    if used > maximum:
        raise DiskBudgetExceeded("staging disk budget")
    return used


def cleanup_abandoned(parent: Path) -> int:
    removed = 0
    for path in parent.glob(f"{STAGE_PREFIX}*"):
        if path.is_dir():
            shutil.rmtree(path)
            removed += 1
    return removed


def build_expected(parent: Path) -> ExpectedArtifact:
    directory = Path(
        tempfile.mkdtemp(prefix="expected-", dir=parent)
    )
    directory.chmod(0o700)
    try:
        output = directory / "pages.bin"
        root, _leaves, _internals, _depth, _spill = pages.emit_pages(
            (pages.entry_bytes(object_id) for object_id in range(1, OBJECTS + 1)),
            directory,
            output,
        )
        make_private_file(output)
        return ExpectedArtifact(
            pages.file_sha256(output), output.stat().st_size, root
        )
    finally:
        shutil.rmtree(directory)


def validate_final(path: Path, expected: ExpectedArtifact) -> bool:
    return (
        path.is_file()
        and path.stat().st_size == expected.size
        and pages.file_sha256(path) == expected.sha256
    )


def publish(
    parent: Path,
    final: Path,
    expected: ExpectedArtifact,
    *,
    failpoint: str | None = None,
    disk_budget: int = 64 * 1024 * 1024,
    cleanup_on_failure: bool = True,
) -> PublishReport:
    stage = Path(
        tempfile.mkdtemp(prefix=STAGE_PREFIX, dir=parent)
    )
    stage.chmod(0o700)
    linked = False
    peak_disk = 0
    merge_passes = 0
    peak_open_files = 0
    try:
        runs = pages.write_runs(
            stage, pages.permuted_ids(OBJECTS), RUN_ENTRIES
        )
        for path in runs:
            make_private_file(path)
        peak_disk = max(peak_disk, check_budget(stage, disk_budget))
        check_private(stage)
        if failpoint == "after-runs":
            raise InjectedFailure(failpoint)

        merged, merge_passes, _read, _written = staged.staged_merge(
            stage, runs, MAX_OPEN
        )
        make_private_file(merged)
        peak_open_files = MAX_OPEN + 1
        peak_disk = max(peak_disk, check_budget(stage, disk_budget))
        check_private(stage)
        if failpoint == "after-merge":
            raise InjectedFailure(failpoint)

        artifact = stage / "directory-pages.bin"
        root, _leaves, _internals, _depth, _reference_spill = pages.emit_pages(
            staged.final_records(merged, expected_count=OBJECTS),
            stage,
            artifact,
        )
        make_private_file(artifact)
        if root != expected.root:
            raise AssertionError("staged root differs from direct root")
        if not validate_final(artifact, expected):
            raise AssertionError("staged artifact differs from direct output")
        peak_disk = max(peak_disk, check_budget(stage, disk_budget))
        check_private(stage)
        if failpoint == "after-pages":
            raise InjectedFailure(failpoint)

        with artifact.open("rb") as stream:
            os.fsync(stream.fileno())
        if failpoint == "after-file-fsync":
            raise InjectedFailure(failpoint)

        # Hard-link publication is create-new and atomic on one filesystem. It
        # fails rather than replacing an existing destination.
        os.link(artifact, final)
        linked = True
        if failpoint == "after-link":
            raise InjectedFailure(failpoint)

        directory_fd = os.open(parent, os.O_RDONLY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
        if not validate_final(final, expected):
            raise AssertionError("published artifact is invalid")
        return PublishReport(
            failpoint,
            True,
            True,
            "published",
            peak_disk,
            merge_passes,
            peak_open_files,
        )
    except (InjectedFailure, DiskBudgetExceeded, FileExistsError):
        if cleanup_on_failure:
            shutil.rmtree(stage, ignore_errors=True)
        final_exists = final.exists()
        final_valid = validate_final(final, expected) if final_exists else False
        outcome = "indeterminate-published" if linked else "not-published"
        return PublishReport(
            failpoint,
            final_exists,
            final_valid,
            outcome,
            peak_disk,
            merge_passes,
            peak_open_files,
        )
    finally:
        if stage.exists() and (cleanup_on_failure or linked):
            shutil.rmtree(stage, ignore_errors=True)


def main() -> None:
    with tempfile.TemporaryDirectory(
        prefix="ucof-exp0002-publication-"
    ) as temporary:
        parent = Path(temporary)
        expected = build_expected(parent)

        successful_final = parent / "published.bin"
        success = publish(parent, successful_final, expected)
        assert success.outcome == "published"
        assert success.final_exists and success.final_valid
        assert not list(parent.glob(f"{STAGE_PREFIX}*"))

        before_link_reports: list[PublishReport] = []
        for failpoint in (
            "after-runs",
            "after-merge",
            "after-pages",
            "after-file-fsync",
        ):
            final = parent / f"{failpoint}.bin"
            report = publish(parent, final, expected, failpoint=failpoint)
            before_link_reports.append(report)
            assert report.outcome == "not-published"
            assert not report.final_exists
            assert not final.exists()

        after_link_final = parent / "after-link.bin"
        after_link = publish(
            parent,
            after_link_final,
            expected,
            failpoint="after-link",
        )
        assert after_link.outcome == "indeterminate-published"
        assert after_link.final_exists and after_link.final_valid

        existing = parent / "existing.bin"
        existing.write_bytes(b"existing destination")
        existing_before = existing.read_bytes()
        no_overwrite = publish(parent, existing, expected)
        assert no_overwrite.outcome == "not-published"
        assert existing.read_bytes() == existing_before

        low_budget_final = parent / "low-budget.bin"
        low_budget = publish(
            parent,
            low_budget_final,
            expected,
            disk_budget=1024,
        )
        assert low_budget.outcome == "not-published"
        assert not low_budget_final.exists()

        abandoned_final = parent / "abandoned.bin"
        abandoned = publish(
            parent,
            abandoned_final,
            expected,
            failpoint="after-merge",
            cleanup_on_failure=False,
        )
        assert abandoned.outcome == "not-published"
        assert not abandoned_final.exists()
        abandoned_directories = list(parent.glob(f"{STAGE_PREFIX}*"))
        assert len(abandoned_directories) == 1
        check_private(abandoned_directories[0])
        removed = cleanup_abandoned(parent)
        assert removed == 1
        assert not list(parent.glob(f"{STAGE_PREFIX}*"))

        print(f"objects={OBJECTS:,}")
        print(f"run_entries={RUN_ENTRIES}")
        print(f"artifact_bytes={expected.size:,}")
        print(f"artifact_sha256={expected.sha256}")
        print(f"merge_passes={success.merge_passes}")
        print(f"peak_open_files={success.peak_open_files}")
        print(f"peak_staging_disk_bytes={success.peak_disk_bytes:,}")
        print(
            f"before_link_failpoints={tuple(report.failpoint for report in before_link_reports)}"
        )
        print(f"after_link_outcome={after_link.outcome}")
        print(f"abandoned_directories_removed={removed}")
        print("private_staging_permissions=pass")
        print("disk_budget_failure_before_publication=pass")
        print("create_new_no_overwrite=pass")
        print("atomic_link_publication=pass")
        print("post_link_indeterminate_outcome_is_valid_artifact=pass")
        print("finding=publication state changes at the atomic filesystem operation, not at function return")
        print("finding=after-link failure cannot honestly be reported as not published")
        print("finding=abandoned private staging requires explicit startup cleanup policy")


if __name__ == "__main__":
    main()
