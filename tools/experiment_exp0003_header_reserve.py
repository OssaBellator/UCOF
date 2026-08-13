#!/usr/bin/env python3
"""Model EXP-0003 fixed-header reserve and alignment trade-offs.

The experiment distinguishes per-object reserve, which consumes file bytes once
per object, from page-header reserve, which only moves zero bytes between the
header and fixed-size page padding while capacity remains on the same plateau.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass

PAGE_SIZE = 16_384
OBJECT_COUNTS = (1_000_000, 100_000_000, 1_000_000_000)


@dataclass(frozen=True)
class Layout:
    name: str
    id_bits: int
    object_header_len: int
    page_header_len: int
    leaf_entry_len: int
    internal_entry_len: int


LAYOUTS = (
    Layout("draft-128", 128, 64, 80, 64, 72),
    Layout("compact-128", 128, 56, 64, 64, 72),
    Layout("tight-128", 128, 48, 56, 64, 72),
    Layout("compact-64", 64, 48, 48, 56, 56),
    Layout("tight-64", 64, 40, 40, 56, 56),
)


@dataclass(frozen=True)
class Row:
    layout: str
    id_bits: int
    objects: int
    object_header_len: int
    page_header_len: int
    leaf_capacity: int
    internal_fanout: int
    level_page_counts: list[int]
    directory_bytes: int
    object_header_bytes: int
    structural_bytes: int
    structural_bytes_per_object: float


def ceil_div(value: int, divisor: int) -> int:
    return (value + divisor - 1) // divisor


def derive(layout: Layout, objects: int) -> Row:
    leaf_capacity = (PAGE_SIZE - layout.page_header_len) // layout.leaf_entry_len
    internal_fanout = (PAGE_SIZE - layout.page_header_len) // layout.internal_entry_len
    pages = [ceil_div(objects, leaf_capacity)]
    while pages[-1] > 1:
        pages.append(ceil_div(pages[-1], internal_fanout))
    directory_bytes = sum(pages) * PAGE_SIZE
    object_header_bytes = objects * layout.object_header_len
    structural_bytes = directory_bytes + object_header_bytes
    return Row(
        layout=layout.name,
        id_bits=layout.id_bits,
        objects=objects,
        object_header_len=layout.object_header_len,
        page_header_len=layout.page_header_len,
        leaf_capacity=leaf_capacity,
        internal_fanout=internal_fanout,
        level_page_counts=pages,
        directory_bytes=directory_bytes,
        object_header_bytes=object_header_bytes,
        structural_bytes=structural_bytes,
        structural_bytes_per_object=structural_bytes / objects,
    )


def rows() -> list[Row]:
    return [derive(layout, objects) for layout in LAYOUTS for objects in OBJECT_COUNTS]


def validate() -> None:
    by_name = {layout.name: layout for layout in LAYOUTS}

    # Exact semantic field sizes before alignment/reserve.
    assert 8 + 2 + 2 + 4 + 16 + 8 + 8 == 48
    assert 8 + 2 + 2 + 4 + 8 + 8 + 8 == 40
    assert 8 + 1 + 1 + 2 + 4 + 2 + 2 + 16 + 16 == 52
    assert 8 + 1 + 1 + 2 + 4 + 2 + 2 + 8 + 8 == 36

    # Tight page headers add only four zero bytes to keep entry starts 8-byte aligned.
    assert by_name["tight-128"].page_header_len == 56
    assert by_name["tight-64"].page_header_len == 40

    # Removing the remaining page-header reserve does not change capacity plateaus.
    compact_128 = derive(by_name["compact-128"], 100_000_000)
    tight_128 = derive(by_name["tight-128"], 100_000_000)
    compact_64 = derive(by_name["compact-64"], 100_000_000)
    tight_64 = derive(by_name["tight-64"], 100_000_000)
    assert (compact_128.leaf_capacity, compact_128.internal_fanout) == (255, 226)
    assert (tight_128.leaf_capacity, tight_128.internal_fanout) == (255, 226)
    assert (compact_64.leaf_capacity, compact_64.internal_fanout) == (291, 291)
    assert (tight_64.leaf_capacity, tight_64.internal_fanout) == (291, 291)
    assert compact_128.directory_bytes == tight_128.directory_bytes
    assert compact_64.directory_bytes == tight_64.directory_bytes

    # The remaining 8-byte object reserve costs exactly eight bytes per object.
    assert compact_128.structural_bytes - tight_128.structural_bytes == 800_000_000
    assert compact_64.structural_bytes - tight_64.structural_bytes == 800_000_000

    # Tight 64-bit keeps the particularly simple equal leaf/internal capacity.
    assert tight_64.leaf_capacity == tight_64.internal_fanout == 291


def print_csv(result_rows: list[Row]) -> None:
    print(
        "layout,id_bits,objects,object_header,page_header,leaf_capacity,internal_fanout,"
        "level_pages,directory_bytes,object_header_bytes,structural_bytes,"
        "structural_bytes_per_object"
    )
    for row in result_rows:
        print(
            f"{row.layout},{row.id_bits},{row.objects},{row.object_header_len},"
            f"{row.page_header_len},{row.leaf_capacity},{row.internal_fanout},"
            f"{'/'.join(str(value) for value in row.level_page_counts)},"
            f"{row.directory_bytes},{row.object_header_bytes},{row.structural_bytes},"
            f"{row.structural_bytes_per_object:.6f}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    validate()
    result_rows = rows()
    if args.json:
        print(json.dumps([asdict(row) for row in result_rows], indent=2, sort_keys=True))
    else:
        print_csv(result_rows)


if __name__ == "__main__":
    main()
