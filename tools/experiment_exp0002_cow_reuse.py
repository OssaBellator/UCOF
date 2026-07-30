#!/usr/bin/env python3
"""Quantify Candidate 1 full rebuild versus path-copy page reuse."""

from __future__ import annotations

from dataclasses import dataclass
from math import ceil

PAGE_BYTES = 16 * 1024
PAGE_HEADER_BYTES = 64
LEAF_ENTRY_BYTES = 88
INTERNAL_ENTRY_BYTES = 64
LEAF_CAPACITY = (PAGE_BYTES - PAGE_HEADER_BYTES) // LEAF_ENTRY_BYTES
INTERNAL_FANOUT = (PAGE_BYTES - PAGE_HEADER_BYTES) // INTERNAL_ENTRY_BYTES
OBJECT_COUNTS = (1_000, 1_000_000, 100_000_000)


@dataclass(frozen=True)
class Shape:
    objects: int
    level_page_counts: tuple[int, ...]
    rightmost_occupancy: tuple[int, ...]

    @property
    def depth(self) -> int:
        return len(self.level_page_counts)

    @property
    def total_pages(self) -> int:
        return sum(self.level_page_counts)

    @property
    def rebuild_bytes(self) -> int:
        return self.total_pages * PAGE_BYTES

    @property
    def replacement_pages(self) -> int:
        return self.depth

    @property
    def replacement_bytes(self) -> int:
        return self.replacement_pages * PAGE_BYTES

    @property
    def replacement_amplification(self) -> float:
        return self.rebuild_bytes / self.replacement_bytes


def build_shape(objects: int) -> Shape:
    if objects < 1:
        raise ValueError("object count must be positive")

    leaf_pages = ceil(objects / LEAF_CAPACITY)
    leaf_occupancy = objects - (leaf_pages - 1) * LEAF_CAPACITY
    page_counts = [leaf_pages]
    rightmost = [leaf_occupancy]
    current = leaf_pages
    while current > 1:
        pages = ceil(current / INTERNAL_FANOUT)
        occupancy = current - (pages - 1) * INTERNAL_FANOUT
        page_counts.append(pages)
        rightmost.append(occupancy)
        current = pages
    return Shape(objects, tuple(page_counts), tuple(rightmost))


def right_edge_insert_pages(shape: Shape) -> tuple[int, bool]:
    """Return newly written pages and whether the root height increases."""

    propagation = shape.rightmost_occupancy[0] == LEAF_CAPACITY
    written = 2 if propagation else 1

    for occupancy in shape.rightmost_occupancy[1:]:
        if propagation:
            propagation = occupancy == INTERNAL_FANOUT
            written += 2 if propagation else 1
        else:
            written += 1

    root_split = propagation
    if root_split:
        written += 1
    return written, root_split


def mib(value: int) -> str:
    return f"{value / (1024 * 1024):.2f}"


def kib(value: int) -> str:
    return f"{value / 1024:.2f}"


def main() -> None:
    print(
        "| Objects | Depth | Reachable pages | Full rebuild MiB | "
        "Replacement pages | Replacement KiB | Rebuild/replacement | "
        "Right-edge insert pages | Insert KiB | Root split |"
    )
    print("|---:|---:|---:|---:|---:|---:|---:|---:|---:|:---:|")

    results = []
    for objects in OBJECT_COUNTS:
        shape = build_shape(objects)
        insert_pages, root_split = right_edge_insert_pages(shape)
        results.append((shape, insert_pages, root_split))
        print(
            f"| {objects:,} | {shape.depth} | {shape.total_pages:,} | "
            f"{mib(shape.rebuild_bytes)} | {shape.replacement_pages} | "
            f"{kib(shape.replacement_bytes)} | "
            f"{shape.replacement_amplification:,.1f}x | {insert_pages} | "
            f"{kib(insert_pages * PAGE_BYTES)} | {'yes' if root_split else 'no'} |"
        )

    assert LEAF_CAPACITY == 185
    assert INTERNAL_FANOUT == 255

    thousand = build_shape(1_000)
    assert thousand.level_page_counts == (6, 1)
    assert thousand.replacement_pages == 2

    million = build_shape(1_000_000)
    assert million.level_page_counts == (5_406, 22, 1)
    assert million.total_pages == 5_429
    assert million.replacement_bytes == 48 * 1024
    assert million.replacement_amplification > 1_800

    hundred_million = build_shape(100_000_000)
    assert hundred_million.level_page_counts == (540_541, 2_120, 9, 1)
    assert hundred_million.total_pages == 542_671
    assert hundred_million.replacement_bytes == 64 * 1024
    assert hundred_million.replacement_amplification > 135_000

    for shape, insert_pages, _ in results:
        assert shape.depth <= insert_pages <= 2 * shape.depth + 1
        assert insert_pages * PAGE_BYTES < shape.rebuild_bytes


if __name__ == "__main__":
    main()
