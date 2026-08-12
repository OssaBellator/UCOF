#!/usr/bin/env python3
"""Model EXP-0003 identifier-width and compact-header trade-offs.

This script deliberately separates identifier namespace policy from locator density.
It compares the first 128-bit Draft layout with compact 128-bit and compact 64-bit
variants while keeping 16 KiB pages and SHA-256 locator authentication.
"""

from __future__ import annotations

import argparse
import json
import math
from dataclasses import asdict, dataclass

PAGE_SIZE = 16_384
OBJECT_COUNTS = (1_000_000, 100_000_000, 1_000_000_000)


@dataclass(frozen=True)
class Variant:
    name: str
    id_bits: int
    object_header_len: int
    page_header_len: int
    leaf_entry_len: int
    internal_entry_len: int


VARIANTS = (
    Variant(
        name="draft-128",
        id_bits=128,
        object_header_len=64,
        page_header_len=80,
        leaf_entry_len=64,
        internal_entry_len=72,
    ),
    Variant(
        name="compact-128",
        id_bits=128,
        object_header_len=56,
        page_header_len=64,
        leaf_entry_len=64,
        internal_entry_len=72,
    ),
    Variant(
        name="compact-64",
        id_bits=64,
        object_header_len=48,
        page_header_len=48,
        leaf_entry_len=56,
        internal_entry_len=56,
    ),
)


@dataclass(frozen=True)
class Row:
    variant: str
    id_bits: int
    object_count: int
    object_header_len: int
    page_header_len: int
    leaf_entry_len: int
    internal_entry_len: int
    leaf_capacity: int
    internal_fanout: int
    level_page_counts: list[int]
    tree_levels: int
    directory_bytes: int
    object_header_bytes: int
    structural_bytes: int
    structural_bytes_per_object: float
    path_bytes: int
    random_id_collision_probability: float


def ceil_div(value: int, divisor: int) -> int:
    return (value + divisor - 1) // divisor


def collision_probability(bits: int, count: int) -> float:
    """Birthday-bound probability for uniform independent random identifiers."""
    exponent = count * (count - 1) / (2 * (2**bits))
    return -math.expm1(-exponent)


def derive(variant: Variant, object_count: int) -> Row:
    leaf_capacity = (PAGE_SIZE - variant.page_header_len) // variant.leaf_entry_len
    internal_fanout = (PAGE_SIZE - variant.page_header_len) // variant.internal_entry_len
    if leaf_capacity < 2 or internal_fanout < 2:
        raise ValueError("variant produces unusable tree geometry")

    levels = [ceil_div(object_count, leaf_capacity)]
    while levels[-1] > 1:
        levels.append(ceil_div(levels[-1], internal_fanout))

    directory_bytes = sum(levels) * PAGE_SIZE
    object_header_bytes = object_count * variant.object_header_len
    structural_bytes = directory_bytes + object_header_bytes

    return Row(
        variant=variant.name,
        id_bits=variant.id_bits,
        object_count=object_count,
        object_header_len=variant.object_header_len,
        page_header_len=variant.page_header_len,
        leaf_entry_len=variant.leaf_entry_len,
        internal_entry_len=variant.internal_entry_len,
        leaf_capacity=leaf_capacity,
        internal_fanout=internal_fanout,
        level_page_counts=levels,
        tree_levels=len(levels),
        directory_bytes=directory_bytes,
        object_header_bytes=object_header_bytes,
        structural_bytes=structural_bytes,
        structural_bytes_per_object=structural_bytes / object_count,
        path_bytes=len(levels) * PAGE_SIZE,
        random_id_collision_probability=collision_probability(variant.id_bits, object_count),
    )


def rows() -> list[Row]:
    return [derive(variant, count) for variant in VARIANTS for count in OBJECT_COUNTS]


def print_csv(result_rows: list[Row]) -> None:
    print(
        "variant,id_bits,objects,object_header,page_header,leaf_entry,internal_entry,"
        "leaf_capacity,internal_fanout,levels,level_pages,directory_bytes,"
        "object_header_bytes,structural_bytes,structural_bytes_per_object,path_bytes,"
        "random_id_collision_probability"
    )
    for row in result_rows:
        print(
            f"{row.variant},{row.id_bits},{row.object_count},{row.object_header_len},"
            f"{row.page_header_len},{row.leaf_entry_len},{row.internal_entry_len},"
            f"{row.leaf_capacity},{row.internal_fanout},{row.tree_levels},"
            f"{'/'.join(str(value) for value in row.level_page_counts)},"
            f"{row.directory_bytes},{row.object_header_bytes},{row.structural_bytes},"
            f"{row.structural_bytes_per_object:.6f},{row.path_bytes},"
            f"{row.random_id_collision_probability:.12g}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    result_rows = rows()
    if args.json:
        print(json.dumps([asdict(row) for row in result_rows], indent=2, sort_keys=True))
    else:
        print_csv(result_rows)


if __name__ == "__main__":
    main()
