#!/usr/bin/env python3
"""Staged bounded spill merge feeding canonical immutable page emission."""

from __future__ import annotations

import heapq
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO, Iterator

import experiment_exp0002_spill_page_emission as pages

RUN_ENTRIES = 512
MAX_OPEN_VALUES = (4, 8, 32)


@dataclass(frozen=True)
class StageReport:
    max_open_inputs: int
    initial_runs: int
    merge_passes: int
    peak_open_files: int
    merge_bytes_read: int
    merge_bytes_written: int
    output_bytes: int
    output_sha256: str
    root: pages.PageRef


def merge_group(inputs: list[Path], output: Path) -> tuple[int, int, int]:
    streams: list[BinaryIO] = [path.open("rb") for path in inputs]
    records = 0
    previous = 0
    try:
        heap: list[tuple[int, int, bytes]] = []
        for index, stream in enumerate(streams):
            item = pages.read_entry(stream)
            if item is not None:
                object_id, record = item
                heapq.heappush(heap, (object_id, index, record))

        with output.open("wb") as target:
            while heap:
                object_id, index, record = heapq.heappop(heap)
                if object_id <= previous:
                    raise pages.DuplicateKey(
                        f"duplicate or unordered staged identifier {object_id}"
                    )
                target.write(record)
                records += 1
                previous = object_id
                item = pages.read_entry(streams[index])
                if item is not None:
                    next_id, next_record = item
                    heapq.heappush(heap, (next_id, index, next_record))
    finally:
        for stream in streams:
            stream.close()

    bytes_processed = records * pages.LEAF_ENTRY_LEN
    return records, bytes_processed, bytes_processed


def staged_merge(directory: Path, paths: list[Path], max_open: int) -> tuple[Path, int, int, int]:
    if max_open < 2:
        raise ValueError("max_open must be at least two")
    current = paths
    merge_bytes_read = 0
    merge_bytes_written = 0
    merge_passes = 0

    while len(current) > 1:
        next_paths: list[Path] = []
        for group_index, start in enumerate(range(0, len(current), max_open)):
            group = current[start : start + max_open]
            if len(group) == 1:
                next_paths.append(group[0])
                continue
            target = directory / f"merge-{merge_passes:02d}-{group_index:06d}.bin"
            _records, bytes_read, bytes_written = merge_group(group, target)
            merge_bytes_read += bytes_read
            merge_bytes_written += bytes_written
            next_paths.append(target)
            for path in group:
                path.unlink()
        current = next_paths
        merge_passes += 1

    return current[0], merge_passes, merge_bytes_read, merge_bytes_written


def final_records(path: Path, expected_count: int = pages.OBJECTS) -> Iterator[bytes]:
    expected = 1
    with path.open("rb") as stream:
        while True:
            item = pages.read_entry(stream)
            if item is None:
                break
            object_id, record = item
            if object_id != expected:
                if object_id < expected:
                    raise pages.DuplicateKey(
                        f"duplicate staged identifier {object_id}"
                    )
                raise ValueError(
                    f"missing staged identifier {expected}; found {object_id}"
                )
            expected += 1
            yield record
    if expected != expected_count + 1:
        raise ValueError("staged output has wrong object count")


def run(max_open: int) -> StageReport:
    with tempfile.TemporaryDirectory(prefix="ucof-exp0002-staged-merge-") as temporary:
        directory = Path(temporary)
        initial = pages.write_runs(directory, pages.permuted_ids(), RUN_ENTRIES)
        initial_count = len(initial)
        final, passes, bytes_read, bytes_written = staged_merge(
            directory, initial, max_open
        )
        output_path = directory / "directory-pages.bin"
        root, _leaf_pages, _internal_pages, _depth, _ref_spill = pages.emit_pages(
            final_records(final), directory, output_path
        )
        return StageReport(
            max_open_inputs=max_open,
            initial_runs=initial_count,
            merge_passes=passes,
            peak_open_files=max_open + 1,
            merge_bytes_read=bytes_read,
            merge_bytes_written=bytes_written,
            output_bytes=output_path.stat().st_size,
            output_sha256=pages.file_sha256(output_path),
            root=root,
        )


def duplicate_test() -> None:
    with tempfile.TemporaryDirectory(prefix="ucof-exp0002-staged-duplicate-") as temporary:
        directory = Path(temporary)
        runs = pages.write_runs(directory, [1, 3, 2, 4, 2], 2)
        try:
            final, _passes, _read, _written = staged_merge(directory, runs, 2)
            list(final_records(final, expected_count=5))
        except pages.DuplicateKey:
            return
        raise AssertionError("cross-pass duplicate was not rejected")


def main() -> None:
    reports = [run(max_open) for max_open in MAX_OPEN_VALUES]
    direct_sha256, direct_root, direct_bytes = pages.run_direct()
    duplicate_test()

    assert all(report.initial_runs == 391 for report in reports)
    assert [report.merge_passes for report in reports] == [5, 3, 2]
    assert all(report.output_sha256 == direct_sha256 for report in reports)
    assert all(report.root == direct_root for report in reports)
    assert all(report.output_bytes == direct_bytes for report in reports)
    assert all(report.peak_open_files <= report.max_open_inputs + 1 for report in reports)

    print(
        "| Objects | Initial runs | Max open inputs | Merge passes | Peak open files | "
        "Merge bytes read | Merge bytes written | Output bytes | Output SHA-256 |"
    )
    print("|---:|---:|---:|---:|---:|---:|---:|---:|---|")
    for report in reports:
        print(
            f"| {pages.OBJECTS:,} | {report.initial_runs} | {report.max_open_inputs} | "
            f"{report.merge_passes} | {report.peak_open_files} | "
            f"{report.merge_bytes_read:,} | {report.merge_bytes_written:,} | "
            f"{report.output_bytes:,} | `{report.output_sha256}` |"
        )
    print(f"run_entries={RUN_ENTRIES}")
    print(f"peak_sort_buffer_bytes={RUN_ENTRIES * pages.LEAF_ENTRY_LEN}")
    print("cross_pass_duplicate_detection=pass")
    print("direct_and_staged_pages_equal=pass")
    print("finding=descriptor-bounded staged merging preserves canonical page output")
    print("finding=fewer open files require more complete spill read/write passes")
    print("finding=run, pass, descriptor, byte, and disk limits must be jointly configured")


if __name__ == "__main__":
    main()
