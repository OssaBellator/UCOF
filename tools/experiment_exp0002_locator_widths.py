#!/usr/bin/env python3
"""Compare authenticated leaf-locator widths for EXP-0002 Candidate 1."""

from __future__ import annotations

from dataclasses import dataclass
from math import ceil

PAGE_BYTES = 16 * 1024
PAGE_HEADER_BYTES = 64
INTERNAL_ENTRY_BYTES = 64
INTERNAL_FANOUT = (PAGE_BYTES - PAGE_HEADER_BYTES) // INTERNAL_ENTRY_BYTES
OBJECT_COUNTS = (1_000_000, 100_000_000)


@dataclass(frozen=True)
class Variant:
    name: str
    entry_bytes: int
    identifier_bits: int
    mirrored_kind: bool
    mirrored_logical_length: bool
    trailing_reserved_bytes: int


VARIANTS = (
    Variant("Candidate 1 baseline", 88, 64, True, True, 16),
    Variant("Tight same fields", 72, 64, True, True, 0),
    Variant("Minimal authenticated", 56, 64, False, False, 0),
    Variant("Minimal authenticated 128-bit ID", 64, 128, False, False, 0),
    Variant("Baseline fields 128-bit ID", 96, 128, True, True, 16),
)


@dataclass(frozen=True)
class Shape:
    leaf_capacity: int
    pages: int
    depth: int
    directory_bytes: int


def shape(objects: int, entry_bytes: int) -> Shape:
    capacity = (PAGE_BYTES - PAGE_HEADER_BYTES) // entry_bytes
    leaves = ceil(objects / capacity)
    pages = leaves
    depth = 1
    level = leaves
    while level > 1:
        level = ceil(level / INTERNAL_FANOUT)
        pages += level
        depth += 1
    return Shape(capacity, pages, depth, pages * PAGE_BYTES)


def gib(value: int) -> str:
    return f"{value / (1024 ** 3):.3f} GiB"


def main() -> None:
    print(
        "| Variant | Entry bytes | ID bits | Leaf capacity | "
        "1M directory | 100M directory | 100M depth |"
    )
    print("|---|---:|---:|---:|---:|---:|---:|")
    rows = []
    for variant in VARIANTS:
        one_million = shape(OBJECT_COUNTS[0], variant.entry_bytes)
        hundred_million = shape(OBJECT_COUNTS[1], variant.entry_bytes)
        rows.append((variant, one_million, hundred_million))
        print(
            f"| {variant.name} | {variant.entry_bytes} | {variant.identifier_bits} | "
            f"{one_million.leaf_capacity} | {gib(one_million.directory_bytes)} | "
            f"{gib(hundred_million.directory_bytes)} | {hundred_million.depth} |"
        )

    baseline = rows[0][2]
    tight = rows[1][2]
    minimal = rows[2][2]
    minimal_128 = rows[3][2]
    baseline_128 = rows[4][2]

    assert INTERNAL_FANOUT == 255
    assert baseline.leaf_capacity == 185
    assert baseline.pages == 542_671
    assert baseline.depth == 4
    assert tight.directory_bytes < baseline.directory_bytes
    assert minimal.directory_bytes < tight.directory_bytes
    assert minimal_128.directory_bytes < baseline.directory_bytes
    assert baseline_128.directory_bytes > baseline.directory_bytes
    assert baseline.directory_bytes - minimal.directory_bytes > 3 * 1024 ** 3
    assert all(row[2].depth == 4 for row in rows)


if __name__ == "__main__":
    main()
