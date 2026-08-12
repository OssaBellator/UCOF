#!/usr/bin/env python3
"""Model EXP-0003 tree geometry with occupancy and latency/bandwidth envelopes.

This is a deterministic analytical model, not a benchmark. It combines:

- B-tree occupancy sensitivity (minimum, ln(2), 80%, packed);
- immutable-page directory geometry;
- a steady-growth split-rate approximation;
- copy-on-write page emission cost; and
- a latency/bandwidth-product break-even model for serial root-to-leaf reads.

The ln(2) occupancy scenario is a reference regime motivated by classical random
insertion analysis. Applying the same fill factor to internal levels is an explicit
sensitivity assumption, not a theorem about UCOF's exact workload.
"""

from __future__ import annotations

import argparse
import json
import math
from dataclasses import asdict, dataclass
from itertools import combinations

PAGE_SIZES = (4096, 16384, 65536)
OBJECT_COUNTS = (1_000_000, 100_000_000, 1_000_000_000)
LN2 = math.log(2.0)


@dataclass(frozen=True)
class Geometry:
    name: str
    page_header_len: int
    leaf_entry_len: int
    internal_entry_len: int


GEOMETRIES = (
    Geometry(
        name="draft-128",
        page_header_len=80,
        leaf_entry_len=64,
        internal_entry_len=72,
    ),
    Geometry(
        name="compact-128",
        page_header_len=64,
        leaf_entry_len=64,
        internal_entry_len=72,
    ),
)


@dataclass(frozen=True)
class OccupancyScenario:
    name: str
    fill: float | None
    use_exact_minimum: bool = False


OCCUPANCIES = (
    OccupancyScenario(name="minimum", fill=None, use_exact_minimum=True),
    OccupancyScenario(name="yao-random-insert", fill=LN2),
    OccupancyScenario(name="eighty-percent", fill=0.8),
    OccupancyScenario(name="packed", fill=1.0),
)


@dataclass(frozen=True)
class Row:
    geometry: str
    occupancy: str
    page_size: int
    object_count: int
    page_header_len: int
    leaf_capacity: int
    internal_fanout: int
    leaf_fill: float
    internal_fill: float
    effective_leaf_entries: float
    effective_internal_children: float
    level_page_counts: list[int]
    tree_levels: int
    total_directory_pages: int
    directory_bytes: int
    directory_bytes_per_object: float
    authenticated_path_page_reads: int
    authenticated_path_bytes: int
    split_rates_per_insert: list[float]
    expected_split_events_per_insert: float
    expected_cow_page_writes_per_insert: float
    expected_cow_page_bytes_per_insert: float


@dataclass(frozen=True)
class BreakEven:
    geometry: str
    occupancy: str
    object_count: int
    page_size_a: int
    page_size_b: int
    break_even_latency_bandwidth_product_bytes: float | None
    lower_product_winner: int
    higher_product_winner: int


def derive(geometry: Geometry, scenario: OccupancyScenario, page_size: int, object_count: int) -> Row:
    if object_count <= 0:
        raise ValueError("object_count must be positive")
    if page_size <= geometry.page_header_len:
        raise ValueError("page_size must exceed page header")

    leaf_capacity = (page_size - geometry.page_header_len) // geometry.leaf_entry_len
    internal_fanout = (page_size - geometry.page_header_len) // geometry.internal_entry_len
    if leaf_capacity < 2 or internal_fanout < 2:
        raise ValueError("page size produces unusable tree geometry")

    if scenario.use_exact_minimum:
        leaf_fill = math.ceil(leaf_capacity / 2) / leaf_capacity
        internal_fill = math.ceil(internal_fanout / 2) / internal_fanout
    else:
        assert scenario.fill is not None
        if not 0.0 < scenario.fill <= 1.0:
            raise ValueError("fill must be in (0, 1]")
        leaf_fill = scenario.fill
        internal_fill = scenario.fill

    effective_leaf_entries = leaf_fill * leaf_capacity
    effective_internal_children = internal_fill * internal_fanout

    level_page_counts = [math.ceil(object_count / effective_leaf_entries)]
    while level_page_counts[-1] > 1:
        level_page_counts.append(math.ceil(level_page_counts[-1] / effective_internal_children))

    tree_levels = len(level_page_counts)
    total_directory_pages = sum(level_page_counts)
    directory_bytes = total_directory_pages * page_size
    authenticated_path_bytes = tree_levels * page_size

    # Steady-growth approximation: when tree height is fixed, the long-run rate at
    # which a level gains pages is the derivative of its asymptotic page population
    # with respect to N. Since one split increases page count by one, this is also a
    # useful first-order split-event rate per insertion. Root-height transitions are
    # discrete and deliberately excluded.
    split_rates: list[float] = []
    denominator = effective_leaf_entries
    for level in range(max(0, tree_levels - 1)):
        if level > 0:
            denominator *= effective_internal_children
        split_rates.append(1.0 / denominator)

    expected_split_events = sum(split_rates)

    # A persistent point insertion rewrites one page on each existing root-to-leaf
    # level. A split emits one additional page at that level beyond the ordinary
    # replacement. Root-growth events are intentionally excluded from this smooth
    # approximation because they occur only at discrete height boundaries.
    expected_cow_page_writes = tree_levels + expected_split_events
    expected_cow_page_bytes = expected_cow_page_writes * page_size

    return Row(
        geometry=geometry.name,
        occupancy=scenario.name,
        page_size=page_size,
        object_count=object_count,
        page_header_len=geometry.page_header_len,
        leaf_capacity=leaf_capacity,
        internal_fanout=internal_fanout,
        leaf_fill=leaf_fill,
        internal_fill=internal_fill,
        effective_leaf_entries=effective_leaf_entries,
        effective_internal_children=effective_internal_children,
        level_page_counts=level_page_counts,
        tree_levels=tree_levels,
        total_directory_pages=total_directory_pages,
        directory_bytes=directory_bytes,
        directory_bytes_per_object=directory_bytes / object_count,
        authenticated_path_page_reads=tree_levels,
        authenticated_path_bytes=authenticated_path_bytes,
        split_rates_per_insert=split_rates,
        expected_split_events_per_insert=expected_split_events,
        expected_cow_page_writes_per_insert=expected_cow_page_writes,
        expected_cow_page_bytes_per_insert=expected_cow_page_bytes,
    )


def all_rows() -> list[Row]:
    return [
        derive(geometry, scenario, page_size, object_count)
        for geometry in GEOMETRIES
        for scenario in OCCUPANCIES
        for object_count in OBJECT_COUNTS
        for page_size in PAGE_SIZES
    ]


def break_even(a: Row, b: Row) -> BreakEven:
    if (a.geometry, a.occupancy, a.object_count) != (b.geometry, b.occupancy, b.object_count):
        raise ValueError("break-even rows must share geometry, occupancy, and object count")

    reads_a = a.authenticated_path_page_reads
    reads_b = b.authenticated_path_page_reads
    bytes_a = a.authenticated_path_bytes
    bytes_b = b.authenticated_path_bytes

    # Normalize serial lookup time by bandwidth:
    #
    #   T * bandwidth = transferred_bytes + requests * (latency * bandwidth)
    #
    # The quantity latency*bandwidth has units of bytes and is the communication
    # product at which saved requests and extra transfer bytes balance.
    if reads_a == reads_b:
        threshold = None
        lower_winner = a.page_size if bytes_a <= bytes_b else b.page_size
        higher_winner = lower_winner
    else:
        threshold = (bytes_b - bytes_a) / (reads_a - reads_b)
        if threshold <= 0:
            threshold = None
            cost_a_at_zero = bytes_a
            cost_b_at_zero = bytes_b
            lower_winner = a.page_size if cost_a_at_zero <= cost_b_at_zero else b.page_size
            higher_winner = a.page_size if reads_a <= reads_b else b.page_size
        else:
            lower_winner = a.page_size if bytes_a <= bytes_b else b.page_size
            higher_winner = a.page_size if reads_a <= reads_b else b.page_size

    return BreakEven(
        geometry=a.geometry,
        occupancy=a.occupancy,
        object_count=a.object_count,
        page_size_a=a.page_size,
        page_size_b=b.page_size,
        break_even_latency_bandwidth_product_bytes=threshold,
        lower_product_winner=lower_winner,
        higher_product_winner=higher_winner,
    )


def all_break_evens(rows: list[Row]) -> list[BreakEven]:
    grouped: dict[tuple[str, str, int], list[Row]] = {}
    for row in rows:
        grouped.setdefault((row.geometry, row.occupancy, row.object_count), []).append(row)

    result: list[BreakEven] = []
    for group_rows in grouped.values():
        for a, b in combinations(sorted(group_rows, key=lambda row: row.page_size), 2):
            result.append(break_even(a, b))
    return result


def self_check(rows: list[Row], break_evens: list[BreakEven]) -> None:
    lookup = {
        (row.geometry, row.occupancy, row.object_count, row.page_size): row
        for row in rows
    }
    compact_100m = {
        page_size: lookup[("compact-128", "yao-random-insert", 100_000_000, page_size)]
        for page_size in PAGE_SIZES
    }

    assert [compact_100m[size].tree_levels for size in PAGE_SIZES] == [6, 4, 3]
    assert math.isclose(
        compact_100m[16384].split_rates_per_insert[0],
        1.0 / (LN2 * 255),
        rel_tol=1e-12,
    )

    be_lookup = {
        (item.geometry, item.occupancy, item.object_count, item.page_size_a, item.page_size_b): item
        for item in break_evens
    }
    assert math.isclose(
        be_lookup[("compact-128", "yao-random-insert", 100_000_000, 4096, 16384)].break_even_latency_bandwidth_product_bytes or -1,
        20_480.0,
        rel_tol=0.0,
        abs_tol=1e-9,
    )
    assert math.isclose(
        be_lookup[("compact-128", "yao-random-insert", 100_000_000, 16384, 65536)].break_even_latency_bandwidth_product_bytes or -1,
        131_072.0,
        rel_tol=0.0,
        abs_tol=1e-9,
    )


def print_csv(rows: list[Row], break_evens: list[BreakEven]) -> None:
    print(
        "geometry,occupancy,page_size,objects,page_header,leaf_capacity,internal_fanout,"
        "leaf_fill,internal_fill,effective_leaf_entries,effective_internal_children,"
        "levels,level_pages,total_pages,directory_bytes,bytes_per_object,path_reads,"
        "path_bytes,split_events_per_insert,cow_page_writes_per_insert,cow_page_bytes_per_insert"
    )
    for row in rows:
        print(
            f"{row.geometry},{row.occupancy},{row.page_size},{row.object_count},"
            f"{row.page_header_len},{row.leaf_capacity},{row.internal_fanout},"
            f"{row.leaf_fill:.12g},{row.internal_fill:.12g},"
            f"{row.effective_leaf_entries:.12g},{row.effective_internal_children:.12g},"
            f"{row.tree_levels},{'/'.join(str(value) for value in row.level_page_counts)},"
            f"{row.total_directory_pages},{row.directory_bytes},{row.directory_bytes_per_object:.9f},"
            f"{row.authenticated_path_page_reads},{row.authenticated_path_bytes},"
            f"{row.expected_split_events_per_insert:.12g},"
            f"{row.expected_cow_page_writes_per_insert:.12g},"
            f"{row.expected_cow_page_bytes_per_insert:.6f}"
        )

    print("# break_even_latency_bandwidth_product_bytes")
    print(
        "geometry,occupancy,objects,page_a,page_b,break_even_product_bytes,"
        "lower_product_winner,higher_product_winner"
    )
    for item in break_evens:
        threshold = "" if item.break_even_latency_bandwidth_product_bytes is None else f"{item.break_even_latency_bandwidth_product_bytes:.9f}"
        print(
            f"{item.geometry},{item.occupancy},{item.object_count},{item.page_size_a},"
            f"{item.page_size_b},{threshold},{item.lower_product_winner},{item.higher_product_winner}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="emit one JSON object")
    args = parser.parse_args()

    rows = all_rows()
    break_evens = all_break_evens(rows)
    self_check(rows, break_evens)

    if args.json:
        print(
            json.dumps(
                {
                    "rows": [asdict(row) for row in rows],
                    "break_even": [asdict(item) for item in break_evens],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_csv(rows, break_evens)


if __name__ == "__main__":
    main()
