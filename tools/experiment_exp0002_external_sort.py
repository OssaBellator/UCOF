#!/usr/bin/env python3
"""Bounded deterministic external sort for Candidate 1 leaf entries."""

from __future__ import annotations

import hashlib
import heapq
import math
import struct
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO, Iterable, Iterator

ENTRY_BYTES = 88
ENTRY_STRUCT = struct.Struct("<Q80s")
OBJECTS = 200_003  # prime, allowing a deterministic affine permutation
PERMUTATION_MULTIPLIER = 65_537
PERMUTATION_OFFSET = 17_171
RUN_SIZES = (4096, 7777)


@dataclass(frozen=True)
class SortReport:
    objects: int
    run_entries: int
    runs: int
    peak_run_bytes: int
    spill_bytes: int
    output_bytes: int
    output_sha256: str


class DuplicateKey(ValueError):
    pass


def entry_bytes(object_id: int) -> bytes:
    digest = hashlib.sha256(f"object:{object_id}".encode("ascii")).digest()
    body = bytearray(80)
    struct.pack_into("<H", body, 0, 1)
    struct.pack_into("<Q", body, 8, object_id * 128)
    struct.pack_into("<Q", body, 16, 128)
    struct.pack_into("<Q", body, 24, 80)
    body[32:64] = digest
    return ENTRY_STRUCT.pack(object_id, bytes(body))


def permuted_ids(count: int = OBJECTS) -> Iterator[int]:
    if math.gcd(PERMUTATION_MULTIPLIER, count) != 1:
        raise ValueError("permutation multiplier is not coprime with object count")
    for index in range(count):
        yield ((PERMUTATION_MULTIPLIER * index + PERMUTATION_OFFSET) % count) + 1


def read_entry(stream: BinaryIO) -> tuple[int, bytes] | None:
    data = stream.read(ENTRY_BYTES)
    if not data:
        return None
    if len(data) != ENTRY_BYTES:
        raise ValueError("truncated spill entry")
    object_id, _body = ENTRY_STRUCT.unpack(data)
    return object_id, data


def write_runs(directory: Path, identifiers: Iterable[int], run_entries: int) -> list[Path]:
    paths: list[Path] = []
    run: list[tuple[int, bytes]] = []
    for object_id in identifiers:
        run.append((object_id, entry_bytes(object_id)))
        if len(run) == run_entries:
            paths.append(write_run(directory, len(paths), run))
            run.clear()
    if run:
        paths.append(write_run(directory, len(paths), run))
    return paths


def write_run(directory: Path, index: int, run: list[tuple[int, bytes]]) -> Path:
    run.sort(key=lambda item: item[0])
    if any(left[0] >= right[0] for left, right in zip(run, run[1:])):
        raise DuplicateKey("duplicate identifier inside spill run")
    path = directory / f"run-{index:06d}.bin"
    with path.open("wb") as stream:
        for _object_id, data in run:
            stream.write(data)
    return path


def merge_runs(paths: list[Path]) -> tuple[int, str]:
    streams = [path.open("rb") for path in paths]
    try:
        heap: list[tuple[int, int, bytes]] = []
        for index, stream in enumerate(streams):
            item = read_entry(stream)
            if item is not None:
                object_id, data = item
                heapq.heappush(heap, (object_id, index, data))

        expected = 1
        output_bytes = 0
        digest = hashlib.sha256()
        previous = 0
        while heap:
            object_id, index, data = heapq.heappop(heap)
            if object_id <= previous:
                raise DuplicateKey(f"duplicate or unordered identifier {object_id}")
            if object_id != expected:
                raise ValueError(f"missing identifier {expected}; found {object_id}")
            digest.update(data)
            output_bytes += len(data)
            previous = object_id
            expected += 1
            item = read_entry(streams[index])
            if item is not None:
                next_id, next_data = item
                heapq.heappush(heap, (next_id, index, next_data))

        if expected != OBJECTS + 1:
            raise ValueError("merged output has the wrong object count")
        return output_bytes, digest.hexdigest()
    finally:
        for stream in streams:
            stream.close()


def run_sort(run_entries: int) -> SortReport:
    with tempfile.TemporaryDirectory(prefix="ucof-exp0002-sort-") as temporary:
        directory = Path(temporary)
        paths = write_runs(directory, permuted_ids(), run_entries)
        spill_bytes = sum(path.stat().st_size for path in paths)
        output_bytes, output_sha256 = merge_runs(paths)
    return SortReport(
        objects=OBJECTS,
        run_entries=run_entries,
        runs=len(paths),
        peak_run_bytes=run_entries * ENTRY_BYTES,
        spill_bytes=spill_bytes,
        output_bytes=output_bytes,
        output_sha256=output_sha256,
    )


def duplicate_test() -> None:
    with tempfile.TemporaryDirectory(prefix="ucof-exp0002-duplicate-") as temporary:
        directory = Path(temporary)
        paths = write_runs(directory, [3, 1, 2, 2], 2)
        try:
            merge_runs(paths)
        except DuplicateKey:
            return
        raise AssertionError("duplicate object identifier was not rejected")


def main() -> None:
    reports = [run_sort(size) for size in RUN_SIZES]
    duplicate_test()

    assert all(report.output_bytes == OBJECTS * ENTRY_BYTES for report in reports)
    assert all(report.spill_bytes == OBJECTS * ENTRY_BYTES for report in reports)
    assert reports[0].output_sha256 == reports[1].output_sha256
    assert reports[0].runs == math.ceil(OBJECTS / RUN_SIZES[0])
    assert reports[1].runs == math.ceil(OBJECTS / RUN_SIZES[1])
    assert reports[0].peak_run_bytes < 400 * 1024
    assert reports[1].peak_run_bytes < 700 * 1024

    print(
        "| Objects | Entries/run | Runs | Peak run bytes | Spill bytes | "
        "Output bytes | Output SHA-256 |"
    )
    print("|---:|---:|---:|---:|---:|---:|---|")
    for report in reports:
        print(
            f"| {report.objects:,} | {report.run_entries:,} | {report.runs} | "
            f"{report.peak_run_bytes:,} | {report.spill_bytes:,} | "
            f"{report.output_bytes:,} | `{report.output_sha256}` |"
        )
    print("duplicate_detection=pass")
    print("chunk_size_independent_output=pass")


if __name__ == "__main__":
    main()
