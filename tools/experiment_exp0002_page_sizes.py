#!/usr/bin/env python3
"""Compare provisional EXP-0002 page sizes using the concrete entry layout."""

from __future__ import annotations

from dataclasses import dataclass
from math import ceil

HEADER_BYTES = 64
LEAF_ENTRY_BYTES = 88
INTERNAL_ENTRY_BYTES = 64
PAGE_SIZES = (4 * 1024, 16 * 1024, 64 * 1024)
OBJECT_COUNTS = (1_000, 1_000_000, 100_000_000)


@dataclass(frozen=True)
class PageShape:
    page_bytes: int
    objects: int
    leaf_capacity: int
    internal_fanout: int
    leaf_pages: int
    internal_pages: int
    total_pages: int
    depth: int

    @property
    def directory_bytes(self) -> int:
        return self.total_pages * self.page_bytes

    @property
    def bytes_per_object(self) -> float:
        return self.directory_bytes / self.objects

    @property
    def authenticated_lookup_bytes(self) -> int:
        return self.depth * self.page_bytes

    @property
    def cow_single_update_bytes(self) -> int:
        return self.authenticated_lookup_bytes

    @property
    def full_rebuild_bytes(self) -> int:
        return self.directory_bytes


def capacities(page_bytes: int) -> tuple[int, int]:
    if page_bytes <= HEADER_BYTES:
        raise ValueError("page must exceed header")
    leaf = (page_bytes - HEADER_BYTES) // LEAF_ENTRY_BYTES
    internal = (page_bytes - HEADER_BYTES) // INTERNAL_ENTRY_BYTES
    if leaf < 1 or internal < 2:
        raise ValueError("page does not support required capacities")
    return leaf, internal


def shape(page_bytes: int, objects: int) -> PageShape:
    leaf_capacity, internal_fanout = capacities(page_bytes)
    leaf_pages = max(1, ceil(objects / leaf_capacity))
    current = leaf_pages
    internal_pages = 0
    depth = 1
    while current > 1:
        current = ceil(current / internal_fanout)
        internal_pages += current
        depth += 1
    return PageShape(
        page_bytes=page_bytes,
        objects=objects,
        leaf_capacity=leaf_capacity,
        internal_fanout=internal_fanout,
        leaf_pages=leaf_pages,
        internal_pages=internal_pages,
        total_pages=leaf_pages + internal_pages,
        depth=depth,
    )


def mib(value: int) -> str:
    return f"{value / (1024 * 1024):.2f}"


def kib(value: int) -> str:
    return f"{value / 1024:.2f}"


def main() -> None:
    print(
        "| Objects | Page KiB | Leaf capacity | Internal fanout | Depth | "
        "Directory MiB | Bytes/object | Lookup KiB | COW update KiB |"
    )
    print("|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    results = []
    for objects in OBJECT_COUNTS:
        for page_bytes in PAGE_SIZES:
            result = shape(page_bytes, objects)
            results.append(result)
            print(
                f"| {objects:,} | {page_bytes // 1024} | {result.leaf_capacity} | "
                f"{result.internal_fanout} | {result.depth} | "
                f"{mib(result.directory_bytes)} | {result.bytes_per_object:.2f} | "
                f"{kib(result.authenticated_lookup_bytes)} | "
                f"{kib(result.cow_single_update_bytes)} |"
            )

    assert capacities(4 * 1024) == (45, 63)
    assert capacities(16 * 1024) == (185, 255)
    assert capacities(64 * 1024) == (744, 1023)

    expected = {
        (4 * 1024, 100_000_000): (2_258_067, 5),
        (16 * 1024, 100_000_000): (542_671, 4),
        (64 * 1024, 100_000_000): (134_542, 3),
    }
    for result in results:
        key = (result.page_bytes, result.objects)
        if key in expected:
            assert (result.total_pages, result.depth) == expected[key]

    at_scale = {
        result.page_bytes: result
        for result in results
        if result.objects == 100_000_000
    }
    assert at_scale[4 * 1024].authenticated_lookup_bytes == 20 * 1024
    assert at_scale[16 * 1024].authenticated_lookup_bytes == 64 * 1024
    assert at_scale[64 * 1024].authenticated_lookup_bytes == 192 * 1024
    assert at_scale[64 * 1024].directory_bytes < at_scale[16 * 1024].directory_bytes
    assert at_scale[16 * 1024].directory_bytes < at_scale[4 * 1024].directory_bytes


if __name__ == "__main__":
    main()
