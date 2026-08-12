#!/usr/bin/env python3
"""Compare proposed EXP-0003 B+tree geometry across candidate page sizes.

This is a deterministic arithmetic model, not a latency benchmark. It derives leaf
and internal capacities from the Draft entry widths and reports page counts,
metadata bytes, and one authenticated root-to-leaf path cost.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass

PAGE_HEADER_LEN = 80
LEAF_ENTRY_LEN = 64
INTERNAL_ENTRY_LEN = 72
DEFAULT_PAGE_SIZES = (4096, 16384, 65536)
DEFAULT_OBJECT_COUNTS = (1_000_000, 100_000_000, 1_000_000_000)


@dataclass(frozen=True)
class Row:
    page_size: int
    object_count: int
    leaf_capacity: int
    leaf_minimum: int
    internal_fanout: int
    internal_minimum: int
    level_page_counts: list[int]
    tree_levels: int
    total_directory_pages: int
    directory_bytes: int
    directory_bytes_per_object: float
    authenticated_path_page_reads: int
    authenticated_path_bytes: int
    no_split_page_rewrite_bytes: int


def ceil_div(value: int, divisor: int) -> int:
    return (value + divisor - 1) // divisor


def derive(page_size: int, object_count: int) -> Row:
    if object_count <= 0:
        raise ValueError("object_count must be positive")
    if page_size <= PAGE_HEADER_LEN:
        raise ValueError("page_size must exceed page header")

    leaf_capacity = (page_size - PAGE_HEADER_LEN) // LEAF_ENTRY_LEN
    internal_fanout = (page_size - PAGE_HEADER_LEN) // INTERNAL_ENTRY_LEN
    if leaf_capacity < 2 or internal_fanout < 2:
        raise ValueError("page size produces unusable fanout")

    leaf_minimum = ceil_div(leaf_capacity, 2)
    internal_minimum = ceil_div(internal_fanout, 2)

    level_page_counts: list[int] = [ceil_div(object_count, leaf_capacity)]
    while level_page_counts[-1] > 1:
        level_page_counts.append(ceil_div(level_page_counts[-1], internal_fanout))

    total_pages = sum(level_page_counts)
    levels = len(level_page_counts)
    directory_bytes = total_pages * page_size
    path_bytes = levels * page_size

    return Row(
        page_size=page_size,
        object_count=object_count,
        leaf_capacity=leaf_capacity,
        leaf_minimum=leaf_minimum,
        internal_fanout=internal_fanout,
        internal_minimum=internal_minimum,
        level_page_counts=level_page_counts,
        tree_levels=levels,
        total_directory_pages=total_pages,
        directory_bytes=directory_bytes,
        directory_bytes_per_object=directory_bytes / object_count,
        authenticated_path_page_reads=levels,
        authenticated_path_bytes=path_bytes,
        no_split_page_rewrite_bytes=path_bytes,
    )


def rows(page_sizes: tuple[int, ...], object_counts: tuple[int, ...]) -> list[Row]:
    return [derive(page_size, count) for page_size in page_sizes for count in object_counts]


def print_table(result_rows: list[Row]) -> None:
    header = (
        "page_size,objects,leaf_capacity,leaf_minimum,internal_fanout,"
        "internal_minimum,levels,level_pages,total_pages,directory_bytes,"
        "bytes_per_object,path_reads,path_bytes,no_split_rewrite_page_bytes"
    )
    print(header)
    for row in result_rows:
        print(
            f"{row.page_size},{row.object_count},{row.leaf_capacity},{row.leaf_minimum},"
            f"{row.internal_fanout},{row.internal_minimum},{row.tree_levels},"
            f"{'/'.join(str(value) for value in row.level_page_counts)},"
            f"{row.total_directory_pages},{row.directory_bytes},"
            f"{row.directory_bytes_per_object:.6f},{row.authenticated_path_page_reads},"
            f"{row.authenticated_path_bytes},{row.no_split_page_rewrite_bytes}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="emit JSON instead of CSV")
    args = parser.parse_args()

    result_rows = rows(DEFAULT_PAGE_SIZES, DEFAULT_OBJECT_COUNTS)
    if args.json:
        print(json.dumps([asdict(row) for row in result_rows], indent=2, sort_keys=True))
    else:
        print_table(result_rows)


if __name__ == "__main__":
    main()
