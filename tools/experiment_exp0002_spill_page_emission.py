#!/usr/bin/env python3
"""Bounded external sort feeding deterministic immutable-page emission."""

from __future__ import annotations

import hashlib
import heapq
import math
import os
from math import ceil
import struct
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO, Iterable, Iterator

PAGE_SIZE = 16 * 1024
PAGE_HEADER_LEN = 64
LEAF_ENTRY_LEN = 88
INTERNAL_ENTRY_LEN = 64
LEAF_CAPACITY = (PAGE_SIZE - PAGE_HEADER_LEN) // LEAF_ENTRY_LEN
INTERNAL_FANOUT = (PAGE_SIZE - PAGE_HEADER_LEN) // INTERNAL_ENTRY_LEN

PAGE_MAGIC = b"UCPGIM02"
PAGE_DOMAIN = b"UCOF-IMMUTABLE-PAGE\x00"
PAGE_HEADER = struct.Struct("<8sBBHIIQQ28s")
ENTRY_STRUCT = struct.Struct("<Q80s")
INTERNAL_ENTRY = struct.Struct("<QQQQ32s")
PAGE_REF = struct.Struct("<QQQB7s32s")

OBJECTS = 200_003
PERMUTATION_MULTIPLIER = 65_537
PERMUTATION_OFFSET = 17_171
RUN_SIZES = (4_096, 7_777)


class DuplicateKey(ValueError):
    pass


@dataclass(frozen=True)
class PageRef:
    minimum: int
    maximum: int
    offset: int
    level: int
    digest: bytes


@dataclass(frozen=True)
class EmissionReport:
    run_entries: int
    run_count: int
    peak_sort_buffer_bytes: int
    max_open_runs: int
    locator_spill_bytes: int
    reference_spill_bytes: int
    leaf_pages: int
    internal_pages: int
    depth: int
    output_bytes: int
    output_sha256: str
    root: PageRef


def digest(domain: bytes, data: bytes) -> bytes:
    return hashlib.sha256(domain + data).digest()


def entry_bytes(object_id: int) -> bytes:
    object_digest = hashlib.sha256(f"object:{object_id}".encode("ascii")).digest()
    body = bytearray(80)
    struct.pack_into("<H", body, 0, 1)
    struct.pack_into("<Q", body, 8, object_id * 128)
    struct.pack_into("<Q", body, 16, 128)
    struct.pack_into("<Q", body, 24, 80)
    body[32:64] = object_digest
    return ENTRY_STRUCT.pack(object_id, bytes(body))


def permuted_ids(count: int = OBJECTS) -> Iterator[int]:
    if math.gcd(PERMUTATION_MULTIPLIER, count) != 1:
        raise ValueError("permutation multiplier is not coprime with object count")
    for index in range(count):
        yield ((PERMUTATION_MULTIPLIER * index + PERMUTATION_OFFSET) % count) + 1


def write_run(directory: Path, index: int, records: list[tuple[int, bytes]]) -> Path:
    records.sort(key=lambda item: item[0])
    if any(left[0] >= right[0] for left, right in zip(records, records[1:])):
        raise DuplicateKey("duplicate identifier inside spill run")
    path = directory / f"locator-run-{index:06d}.bin"
    with path.open("wb") as stream:
        for _object_id, record in records:
            stream.write(record)
    return path


def write_runs(directory: Path, identifiers: Iterable[int], run_entries: int) -> list[Path]:
    paths: list[Path] = []
    records: list[tuple[int, bytes]] = []
    for object_id in identifiers:
        records.append((object_id, entry_bytes(object_id)))
        if len(records) == run_entries:
            paths.append(write_run(directory, len(paths), records))
            records.clear()
    if records:
        paths.append(write_run(directory, len(paths), records))
    return paths


def read_entry(stream: BinaryIO) -> tuple[int, bytes] | None:
    data = stream.read(LEAF_ENTRY_LEN)
    if not data:
        return None
    if len(data) != LEAF_ENTRY_LEN:
        raise ValueError("truncated locator spill entry")
    object_id, _body = ENTRY_STRUCT.unpack(data)
    return object_id, data


def merged_entries(paths: list[Path], expected_count: int = OBJECTS) -> Iterator[bytes]:
    streams = [path.open("rb") for path in paths]
    try:
        heap: list[tuple[int, int, bytes]] = []
        for index, stream in enumerate(streams):
            item = read_entry(stream)
            if item is not None:
                object_id, record = item
                heapq.heappush(heap, (object_id, index, record))

        expected = 1
        previous = 0
        while heap:
            object_id, index, record = heapq.heappop(heap)
            if object_id <= previous:
                raise DuplicateKey(f"duplicate or unordered identifier {object_id}")
            if object_id != expected:
                raise ValueError(f"missing identifier {expected}; found {object_id}")
            yield record
            previous = object_id
            expected += 1
            item = read_entry(streams[index])
            if item is not None:
                next_id, next_record = item
                heapq.heappush(heap, (next_id, index, next_record))

        if expected != expected_count + 1:
            raise ValueError("merged locator count mismatch")
    finally:
        for stream in streams:
            stream.close()


def entry_id(record: bytes) -> int:
    return struct.unpack_from("<Q", record, 0)[0]


def encode_leaf(records: list[bytes]) -> bytes:
    if not records or len(records) > LEAF_CAPACITY:
        raise ValueError("invalid leaf record count")
    identifiers = [entry_id(record) for record in records]
    if any(left >= right for left, right in zip(identifiers, identifiers[1:])):
        raise DuplicateKey("leaf identifiers are not strictly ordered")
    page = bytearray(PAGE_SIZE)
    PAGE_HEADER.pack_into(
        page,
        0,
        PAGE_MAGIC,
        1,
        0,
        0,
        len(records),
        LEAF_ENTRY_LEN,
        identifiers[0],
        identifiers[-1],
        bytes(28),
    )
    for index, record in enumerate(records):
        page[PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN : PAGE_HEADER_LEN + (index + 1) * LEAF_ENTRY_LEN] = record
    return bytes(page)


def encode_internal(children: list[PageRef], level: int) -> bytes:
    if not children or len(children) > INTERNAL_FANOUT or level == 0:
        raise ValueError("invalid internal child count")
    if any(left.maximum >= right.minimum for left, right in zip(children, children[1:])):
        raise ValueError("internal child ranges overlap")
    if any(child.level + 1 != level for child in children):
        raise ValueError("internal child level mismatch")
    page = bytearray(PAGE_SIZE)
    PAGE_HEADER.pack_into(
        page,
        0,
        PAGE_MAGIC,
        2,
        level,
        0,
        len(children),
        INTERNAL_ENTRY_LEN,
        children[0].minimum,
        children[-1].maximum,
        bytes(28),
    )
    for index, child in enumerate(children):
        INTERNAL_ENTRY.pack_into(
            page,
            PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN,
            child.minimum,
            child.maximum,
            child.offset,
            PAGE_SIZE,
            child.digest,
        )
    return bytes(page)


def write_page(stream: BinaryIO, page: bytes) -> PageRef:
    if len(page) != PAGE_SIZE:
        raise ValueError("wrong page size")
    magic, kind, level, reserved, count, entry_size, minimum, maximum, tail = PAGE_HEADER.unpack_from(page)
    expected_size = LEAF_ENTRY_LEN if kind == 1 else INTERNAL_ENTRY_LEN
    if magic != PAGE_MAGIC or kind not in (1, 2) or reserved != 0 or any(tail):
        raise ValueError("invalid emitted page header")
    if count == 0 or entry_size != expected_size:
        raise ValueError("invalid emitted page shape")
    offset = stream.tell()
    stream.write(page)
    return PageRef(minimum, maximum, offset, level, digest(PAGE_DOMAIN, page))


def write_page_ref(stream: BinaryIO, reference: PageRef) -> None:
    stream.write(
        PAGE_REF.pack(
            reference.minimum,
            reference.maximum,
            reference.offset,
            reference.level,
            bytes(7),
            reference.digest,
        )
    )


def read_page_ref(stream: BinaryIO) -> PageRef | None:
    raw = stream.read(PAGE_REF.size)
    if not raw:
        return None
    if len(raw) != PAGE_REF.size:
        raise ValueError("truncated page-reference spill")
    minimum, maximum, offset, level, reserved, page_digest = PAGE_REF.unpack(raw)
    if any(reserved) or minimum > maximum:
        raise ValueError("invalid page-reference spill")
    return PageRef(minimum, maximum, offset, level, page_digest)


def emit_pages(sorted_records: Iterable[bytes], directory: Path, output_path: Path) -> tuple[PageRef, int, int, int, int]:
    reference_spill_bytes = 0
    leaf_pages = 0
    internal_pages = 0
    current_refs = directory / "refs-level-0.bin"

    with output_path.open("wb") as output, current_refs.open("wb") as refs:
        batch: list[bytes] = []
        previous = 0
        for record in sorted_records:
            object_id = entry_id(record)
            if object_id <= previous:
                raise DuplicateKey("page emission received unordered identifier")
            previous = object_id
            batch.append(record)
            if len(batch) == LEAF_CAPACITY:
                write_page_ref(refs, write_page(output, encode_leaf(batch)))
                leaf_pages += 1
                batch.clear()
        if batch:
            write_page_ref(refs, write_page(output, encode_leaf(batch)))
            leaf_pages += 1

    current_count = leaf_pages
    reference_spill_bytes += current_refs.stat().st_size
    level = 1
    while current_count > 1:
        next_refs = directory / f"refs-level-{level}.bin"
        next_count = 0
        with output_path.open("ab") as output, current_refs.open("rb") as source, next_refs.open("wb") as target:
            children: list[PageRef] = []
            while True:
                reference = read_page_ref(source)
                if reference is None:
                    break
                children.append(reference)
                if len(children) == INTERNAL_FANOUT:
                    write_page_ref(target, write_page(output, encode_internal(children, level)))
                    internal_pages += 1
                    next_count += 1
                    children.clear()
            if children:
                write_page_ref(target, write_page(output, encode_internal(children, level)))
                internal_pages += 1
                next_count += 1

        reference_spill_bytes += next_refs.stat().st_size
        current_refs.unlink()
        current_refs = next_refs
        current_count = next_count
        level += 1

    with current_refs.open("rb") as root_stream:
        root = read_page_ref(root_stream)
        if root is None or read_page_ref(root_stream) is not None:
            raise ValueError("root reference spill does not contain exactly one entry")
    current_refs.unlink()
    return root, leaf_pages, internal_pages, level, reference_spill_bytes


def file_sha256(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(64 * 1024):
            hasher.update(block)
    return hasher.hexdigest()


def run_spill(run_entries: int) -> EmissionReport:
    with tempfile.TemporaryDirectory(prefix="ucof-exp0002-spill-pages-") as temporary:
        directory = Path(temporary)
        runs = write_runs(directory, permuted_ids(), run_entries)
        locator_spill_bytes = sum(path.stat().st_size for path in runs)
        output_path = directory / "directory-pages.bin"
        root, leaf_pages, internal_pages, depth, reference_spill_bytes = emit_pages(
            merged_entries(runs), directory, output_path
        )
        output_bytes = output_path.stat().st_size
        output_sha256 = file_sha256(output_path)
        return EmissionReport(
            run_entries=run_entries,
            run_count=len(runs),
            peak_sort_buffer_bytes=run_entries * LEAF_ENTRY_LEN,
            max_open_runs=len(runs),
            locator_spill_bytes=locator_spill_bytes,
            reference_spill_bytes=reference_spill_bytes,
            leaf_pages=leaf_pages,
            internal_pages=internal_pages,
            depth=depth,
            output_bytes=output_bytes,
            output_sha256=output_sha256,
            root=root,
        )


def run_direct() -> tuple[str, PageRef, int]:
    with tempfile.TemporaryDirectory(prefix="ucof-exp0002-direct-pages-") as temporary:
        directory = Path(temporary)
        output_path = directory / "directory-pages.bin"
        root, _leaf_pages, _internal_pages, _depth, _reference_spill_bytes = emit_pages(
            (entry_bytes(object_id) for object_id in range(1, OBJECTS + 1)),
            directory,
            output_path,
        )
        return file_sha256(output_path), root, output_path.stat().st_size


def duplicate_test() -> None:
    with tempfile.TemporaryDirectory(prefix="ucof-exp0002-spill-duplicate-") as temporary:
        directory = Path(temporary)
        paths = write_runs(directory, [1, 2, 2, 3], 2)
        try:
            list(merged_entries(paths, expected_count=4))
        except DuplicateKey:
            return
        raise AssertionError("duplicate identifier was not rejected across spill runs")


def main() -> None:
    reports = [run_spill(run_entries) for run_entries in RUN_SIZES]
    direct_sha256, direct_root, direct_bytes = run_direct()
    duplicate_test()

    expected_leaf_pages = ceil(OBJECTS / LEAF_CAPACITY)
    expected_level_one = ceil(expected_leaf_pages / INTERNAL_FANOUT)
    expected_level_two = ceil(expected_level_one / INTERNAL_FANOUT)
    expected_internal_pages = expected_level_one + expected_level_two
    expected_pages = expected_leaf_pages + expected_internal_pages

    assert expected_leaf_pages == 1_082
    assert expected_level_one == 5
    assert expected_level_two == 1
    assert expected_pages == 1_088
    assert all(report.leaf_pages == expected_leaf_pages for report in reports)
    assert all(report.internal_pages == expected_internal_pages for report in reports)
    assert all(report.depth == 3 for report in reports)
    assert all(report.output_bytes == expected_pages * PAGE_SIZE for report in reports)
    assert all(report.locator_spill_bytes == OBJECTS * LEAF_ENTRY_LEN for report in reports)
    assert all(report.peak_sort_buffer_bytes < 700 * 1024 for report in reports)
    assert reports[0].output_sha256 == reports[1].output_sha256 == direct_sha256
    assert reports[0].root == reports[1].root == direct_root
    assert reports[0].output_bytes == reports[1].output_bytes == direct_bytes

    print(
        "| Objects | Entries/run | Runs | Peak sort bytes | Max open runs | "
        "Locator spill | Ref spill | Pages | Output bytes | Root digest | Output SHA-256 |"
    )
    print("|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|")
    for report in reports:
        print(
            f"| {OBJECTS:,} | {report.run_entries:,} | {report.run_count} | "
            f"{report.peak_sort_buffer_bytes:,} | {report.max_open_runs} | "
            f"{report.locator_spill_bytes:,} | {report.reference_spill_bytes:,} | "
            f"{report.leaf_pages + report.internal_pages:,} | {report.output_bytes:,} | "
            f"`{report.root.digest.hex()}` | `{report.output_sha256}` |"
        )
    print(f"direct_output_sha256={direct_sha256}")
    print(f"leaf_buffer_bytes={LEAF_CAPACITY * LEAF_ENTRY_LEN}")
    print(f"internal_group_bytes={INTERNAL_FANOUT * PAGE_REF.size}")
    print("duplicate_detection=pass")
    print("run_size_independent_pages=pass")
    print("direct_and_spill_output_equal=pass")
    print("finding=bounded locator sort can stream directly into canonical immutable pages")
    print("finding=large run counts still require staged merge and descriptor policy")


if __name__ == "__main__":
    main()
