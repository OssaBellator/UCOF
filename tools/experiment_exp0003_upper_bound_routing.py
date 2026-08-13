#!/usr/bin/env python3
"""Model EXP-0003 upper-bound-only internal routing.

The experiment separates four questions:

1. Is insertion routing by strictly increasing child maxima equivalent to the
   Draft's current explicit [child_min, child_max] gap-routing rule?
2. Does upper-bound lookup produce the same result on a valid sparse tree, and
   how often does it need one extra child-page authentication for a gap absence
   that explicit ranges can prove at the parent?
3. Can strict recursive validation reconstruct omitted minima from authenticated
   child headers and enforce the same non-overlap invariant?
4. What density/height benefit does removing the duplicated child minimum buy
   under compact 128-bit and 64-bit geometry?

This is evidence only. It does not change the EXP-0003 Draft.
"""

from __future__ import annotations

import argparse
import json
import random
from dataclasses import asdict, dataclass

PAGE_SIZE = 16_384
OBJECT_COUNTS = (10_000_000, 100_000_000, 1_000_000_000, 10_000_000_000)
RANDOM_SEED = 0x0136_0003


@dataclass(frozen=True)
class Layout:
    name: str
    id_bits: int
    page_header_len: int
    leaf_entry_len: int
    full_internal_entry_len: int
    upper_internal_entry_len: int


LAYOUTS = (
    Layout(
        name="compact-128",
        id_bits=128,
        page_header_len=64,
        leaf_entry_len=64,
        full_internal_entry_len=72,
        upper_internal_entry_len=56,
    ),
    Layout(
        name="compact-64",
        id_bits=64,
        page_header_len=48,
        leaf_entry_len=56,
        full_internal_entry_len=56,
        upper_internal_entry_len=48,
    ),
)


@dataclass(frozen=True)
class GeometryRow:
    layout: str
    id_bits: int
    mode: str
    object_count: int
    leaf_capacity: int
    internal_fanout: int
    tree_levels: int
    level_page_counts: list[int]
    directory_bytes: int
    four_level_capacity: int


@dataclass(frozen=True)
class RoutingSummary:
    generated_parents: int
    query_checks: int
    in_range_lookup_checks: int
    gap_absence_checks: int
    outside_parent_absence_checks: int
    lookup_result_mismatches: int
    in_range_selected_child_mismatches: int
    insertion_mismatches: int
    full_range_parent_gap_shortcuts: int
    upper_bound_extra_child_reads_for_gaps: int
    valid_full_strict_failures: int
    valid_upper_strict_failures: int
    malformed_overlap_cases: int
    full_parent_local_overlap_detections: int
    upper_parent_local_overlap_detections: int
    full_strict_overlap_detections: int
    upper_strict_overlap_detections: int


def ceil_div(value: int, divisor: int) -> int:
    return (value + divisor - 1) // divisor


def capacity(page_header_len: int, entry_len: int) -> int:
    return (PAGE_SIZE - page_header_len) // entry_len


def derive_geometry(layout: Layout, object_count: int, upper_only: bool) -> GeometryRow:
    leaf_capacity = capacity(layout.page_header_len, layout.leaf_entry_len)
    entry_len = (
        layout.upper_internal_entry_len if upper_only else layout.full_internal_entry_len
    )
    fanout = capacity(layout.page_header_len, entry_len)

    pages = [ceil_div(object_count, leaf_capacity)]
    while pages[-1] > 1:
        pages.append(ceil_div(pages[-1], fanout))

    return GeometryRow(
        layout=layout.name,
        id_bits=layout.id_bits,
        mode="upper-bound" if upper_only else "full-range",
        object_count=object_count,
        leaf_capacity=leaf_capacity,
        internal_fanout=fanout,
        tree_levels=len(pages),
        level_page_counts=pages,
        directory_bytes=sum(pages) * PAGE_SIZE,
        four_level_capacity=leaf_capacity * fanout**3,
    )


def geometry_rows() -> list[GeometryRow]:
    return [
        derive_geometry(layout, object_count, upper_only)
        for layout in LAYOUTS
        for upper_only in (False, True)
        for object_count in OBJECT_COUNTS
    ]


def full_range_insertion_route(ranges: list[tuple[int, int]], query: int) -> int:
    """Route insertion using the Draft's explicit range/gap rule."""
    if not ranges:
        raise ValueError("parent must contain at least one child")

    for index, (minimum, maximum) in enumerate(ranges):
        if minimum <= query <= maximum:
            return index
        if query < minimum:
            return index
    return len(ranges) - 1


def upper_bound_route(maxima: list[int], query: int, *, insertion: bool) -> int | None:
    """Route by the first child upper bound >= query."""
    if not maxima:
        raise ValueError("parent must contain at least one child")

    for index, maximum in enumerate(maxima):
        if query <= maximum:
            return index
    return len(maxima) - 1 if insertion else None


def full_range_lookup_child(ranges: list[tuple[int, int]], query: int) -> int | None:
    """Return a child only when explicit parent ranges contain the query.

    This matches the existing authenticated EXP-0002 lookup behavior: a query
    in an inter-child gap can be returned absent from the authenticated parent
    without opening another child page.
    """
    if not ranges:
        raise ValueError("parent must contain at least one child")
    if query < ranges[0][0] or query > ranges[-1][1]:
        return None
    for index, (minimum, maximum) in enumerate(ranges):
        if minimum <= query <= maximum:
            return index
    return None


def upper_bound_lookup_result(
    ranges: list[tuple[int, int]], maxima: list[int], query: int
) -> tuple[int | None, int]:
    """Return final child selection plus extra child-header reads.

    Parent min/max reject queries outside the parent range. Inside that range,
    max-only metadata must descend to the first child with max >= query. If the
    authenticated child header then says query < child.min, the lookup is absent
    and paid one child-page authentication that full ranges could avoid.
    """
    if query < ranges[0][0] or query > ranges[-1][1]:
        return None, 0
    child = upper_bound_route(maxima, query, insertion=False)
    if child is None:
        return None, 0
    minimum, maximum = ranges[child]
    if query < minimum:
        return None, 1
    if query <= maximum:
        return child, 0
    raise AssertionError("upper-bound routing selected an impossible child")


def full_parent_local_valid(
    ranges: list[tuple[int, int]], parent_min: int, parent_max: int
) -> bool:
    if not ranges or ranges[0][0] != parent_min or ranges[-1][1] != parent_max:
        return False
    previous_maximum: int | None = None
    for minimum, maximum in ranges:
        if minimum <= 0 or minimum > maximum:
            return False
        if previous_maximum is not None and minimum <= previous_maximum:
            return False
        previous_maximum = maximum
    return True


def upper_parent_local_valid(maxima: list[int], parent_max: int) -> bool:
    if not maxima or maxima[-1] != parent_max:
        return False
    previous: int | None = None
    for maximum in maxima:
        if maximum <= 0:
            return False
        if previous is not None and maximum <= previous:
            return False
        previous = maximum
    return True


def full_strict_valid(
    stored_ranges: list[tuple[int, int]],
    child_headers: list[tuple[int, int]],
    parent_min: int,
    parent_max: int,
) -> bool:
    if not full_parent_local_valid(stored_ranges, parent_min, parent_max):
        return False
    if len(stored_ranges) != len(child_headers):
        return False
    return all(stored == actual for stored, actual in zip(stored_ranges, child_headers))


def upper_strict_valid(
    stored_maxima: list[int],
    child_headers: list[tuple[int, int]],
    parent_min: int,
    parent_max: int,
) -> bool:
    if not upper_parent_local_valid(stored_maxima, parent_max):
        return False
    if len(stored_maxima) != len(child_headers):
        return False

    previous_maximum: int | None = None
    for index, (stored_maximum, (minimum, maximum)) in enumerate(
        zip(stored_maxima, child_headers)
    ):
        if stored_maximum != maximum or minimum <= 0 or minimum > maximum:
            return False
        if index == 0:
            if minimum != parent_min:
                return False
        elif previous_maximum is None or minimum <= previous_maximum:
            return False
        previous_maximum = stored_maximum

    return stored_maxima[-1] == parent_max


def generate_valid_parent(rng: random.Random, child_count: int) -> list[tuple[int, int]]:
    ranges: list[tuple[int, int]] = []
    minimum = rng.randint(1, 8)
    for index in range(child_count):
        if index > 0:
            minimum = ranges[-1][1] + rng.randint(1, 7)
        maximum = minimum + rng.randint(0, 12)
        ranges.append((minimum, maximum))
    return ranges


def routing_summary() -> RoutingSummary:
    rng = random.Random(RANDOM_SEED)
    generated_parents = 0
    query_checks = 0
    in_range_lookup_checks = 0
    gap_absence_checks = 0
    outside_parent_absence_checks = 0
    lookup_result_mismatches = 0
    in_range_selected_child_mismatches = 0
    insertion_mismatches = 0
    full_range_parent_gap_shortcuts = 0
    upper_bound_extra_child_reads_for_gaps = 0
    valid_full_strict_failures = 0
    valid_upper_strict_failures = 0
    malformed_overlap_cases = 0
    full_parent_local_overlap_detections = 0
    upper_parent_local_overlap_detections = 0
    full_strict_overlap_detections = 0
    upper_strict_overlap_detections = 0

    for child_count in range(2, 33):
        for _ in range(32):
            ranges = generate_valid_parent(rng, child_count)
            maxima = [maximum for _, maximum in ranges]
            parent_min = ranges[0][0]
            parent_max = ranges[-1][1]
            generated_parents += 1

            if not full_strict_valid(ranges, ranges, parent_min, parent_max):
                valid_full_strict_failures += 1
            if not upper_strict_valid(maxima, ranges, parent_min, parent_max):
                valid_upper_strict_failures += 1

            for query in range(max(0, parent_min - 3), parent_max + 4):
                query_checks += 1
                full_child = full_range_lookup_child(ranges, query)
                upper_child, extra_reads = upper_bound_lookup_result(ranges, maxima, query)

                inside_parent = parent_min <= query <= parent_max
                inside_child = any(minimum <= query <= maximum for minimum, maximum in ranges)
                if inside_child:
                    in_range_lookup_checks += 1
                    if full_child != upper_child:
                        in_range_selected_child_mismatches += 1
                elif inside_parent:
                    gap_absence_checks += 1
                    full_range_parent_gap_shortcuts += int(full_child is None)
                    upper_bound_extra_child_reads_for_gaps += extra_reads
                else:
                    outside_parent_absence_checks += 1

                if (full_child is None) != (upper_child is None):
                    lookup_result_mismatches += 1

                if full_range_insertion_route(ranges, query) != upper_bound_route(
                    maxima, query, insertion=True
                ):
                    insertion_mismatches += 1

            # Create one authenticated-child overlap that keeps all stored
            # maxima strictly increasing. Full-range parent metadata can reject
            # the declared overlap locally; upper-bound metadata needs the
            # child's authenticated minimum during strict recursion.
            malformed = list(ranges)
            overlap_index = rng.randrange(1, len(malformed))
            previous_maximum = malformed[overlap_index - 1][1]
            _, old_maximum = malformed[overlap_index]
            malformed_minimum = max(1, previous_maximum)
            if malformed_minimum > old_maximum:
                old_maximum = malformed_minimum
            malformed[overlap_index] = (malformed_minimum, old_maximum)
            malformed_overlap_cases += 1

            if not full_parent_local_valid(malformed, parent_min, parent_max):
                full_parent_local_overlap_detections += 1
            if not upper_parent_local_valid(maxima, parent_max):
                upper_parent_local_overlap_detections += 1
            if not full_strict_valid(malformed, malformed, parent_min, parent_max):
                full_strict_overlap_detections += 1
            if not upper_strict_valid(maxima, malformed, parent_min, parent_max):
                upper_strict_overlap_detections += 1

    summary = RoutingSummary(
        generated_parents=generated_parents,
        query_checks=query_checks,
        in_range_lookup_checks=in_range_lookup_checks,
        gap_absence_checks=gap_absence_checks,
        outside_parent_absence_checks=outside_parent_absence_checks,
        lookup_result_mismatches=lookup_result_mismatches,
        in_range_selected_child_mismatches=in_range_selected_child_mismatches,
        insertion_mismatches=insertion_mismatches,
        full_range_parent_gap_shortcuts=full_range_parent_gap_shortcuts,
        upper_bound_extra_child_reads_for_gaps=upper_bound_extra_child_reads_for_gaps,
        valid_full_strict_failures=valid_full_strict_failures,
        valid_upper_strict_failures=valid_upper_strict_failures,
        malformed_overlap_cases=malformed_overlap_cases,
        full_parent_local_overlap_detections=full_parent_local_overlap_detections,
        upper_parent_local_overlap_detections=upper_parent_local_overlap_detections,
        full_strict_overlap_detections=full_strict_overlap_detections,
        upper_strict_overlap_detections=upper_strict_overlap_detections,
    )

    assert summary.lookup_result_mismatches == 0
    assert summary.in_range_selected_child_mismatches == 0
    assert summary.insertion_mismatches == 0
    assert summary.full_range_parent_gap_shortcuts == summary.gap_absence_checks
    assert summary.upper_bound_extra_child_reads_for_gaps == summary.gap_absence_checks
    assert summary.valid_full_strict_failures == 0
    assert summary.valid_upper_strict_failures == 0
    assert summary.full_parent_local_overlap_detections == summary.malformed_overlap_cases
    assert summary.upper_parent_local_overlap_detections == 0
    assert summary.full_strict_overlap_detections == summary.malformed_overlap_cases
    assert summary.upper_strict_overlap_detections == summary.malformed_overlap_cases
    return summary


def print_csv(rows: list[GeometryRow], summary: RoutingSummary) -> None:
    print("routing_metric,value")
    for key, value in asdict(summary).items():
        print(f"{key},{value}")

    print()
    print(
        "layout,id_bits,mode,objects,leaf_capacity,internal_fanout,tree_levels,"
        "level_pages,directory_bytes,four_level_capacity"
    )
    for row in rows:
        print(
            f"{row.layout},{row.id_bits},{row.mode},{row.object_count},"
            f"{row.leaf_capacity},{row.internal_fanout},{row.tree_levels},"
            f"{'/'.join(str(value) for value in row.level_page_counts)},"
            f"{row.directory_bytes},{row.four_level_capacity}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    summary = routing_summary()
    rows = geometry_rows()

    if args.json:
        print(
            json.dumps(
                {
                    "routing": asdict(summary),
                    "geometry": [asdict(row) for row in rows],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_csv(rows, summary)


if __name__ == "__main__":
    main()
