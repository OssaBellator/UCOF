#!/usr/bin/env python3
"""Derive read/update page-size phase regions for EXP-0003.

This model extends Experiment 0109. It treats each page-size candidate as an
'affine cost line' in the latency-bandwidth product Q for a fixed effective update
pressure U:

    cost = path_bytes + path_reads * Q + U * expected_cow_bytes

where Q = read_latency * read_bandwidth, measured in bytes, and U is a
dimensionless workload/cost weight. U deliberately absorbs update frequency and
the relative cost of writing one byte versus reading one byte. It is not a storage
provider benchmark.

For fixed U, the minimum-cost page size is the lower envelope of three lines.
Pairwise boundaries are linear in U, producing a two-dimensional phase diagram.
"""

from __future__ import annotations

import argparse
import json
import math
from dataclasses import asdict, dataclass

from experiment_exp0003_cost_envelope import PAGE_SIZES, all_rows

UPDATE_PRESSURES = (0.0, 0.01, 0.1, 1.0, 10.0)


@dataclass(frozen=True)
class Boundary:
    geometry: str
    occupancy: str
    object_count: int
    page_size_a: int
    page_size_b: int
    q_intercept_bytes: float | None
    q_slope_bytes_per_update_pressure: float | None


@dataclass(frozen=True)
class PhaseRegion:
    geometry: str
    occupancy: str
    object_count: int
    update_pressure: float
    page_size: int
    q_min_bytes: float
    q_max_bytes: float | None


def normalized_cost(row: object, q_bytes: float, update_pressure: float) -> float:
    return (
        row.authenticated_path_bytes
        + row.authenticated_path_page_reads * q_bytes
        + update_pressure * row.expected_cow_page_bytes_per_insert
    )


def pair_boundary(a: object, b: object) -> Boundary:
    if (a.geometry, a.occupancy, a.object_count) != (
        b.geometry,
        b.occupancy,
        b.object_count,
    ):
        raise ValueError("rows must share geometry, occupancy, and object count")

    request_difference = a.authenticated_path_page_reads - b.authenticated_path_page_reads
    if request_difference == 0:
        intercept = None
        slope = None
    else:
        intercept = (
            b.authenticated_path_bytes - a.authenticated_path_bytes
        ) / request_difference
        slope = (
            b.expected_cow_page_bytes_per_insert
            - a.expected_cow_page_bytes_per_insert
        ) / request_difference

    return Boundary(
        geometry=a.geometry,
        occupancy=a.occupancy,
        object_count=a.object_count,
        page_size_a=a.page_size,
        page_size_b=b.page_size,
        q_intercept_bytes=intercept,
        q_slope_bytes_per_update_pressure=slope,
    )


def all_boundaries(rows: list[object]) -> list[Boundary]:
    grouped: dict[tuple[str, str, int], list[object]] = {}
    for row in rows:
        grouped.setdefault((row.geometry, row.occupancy, row.object_count), []).append(row)

    boundaries: list[Boundary] = []
    for group in grouped.values():
        ordered = sorted(group, key=lambda row: row.page_size)
        for left_index in range(len(ordered)):
            for right_index in range(left_index + 1, len(ordered)):
                boundaries.append(pair_boundary(ordered[left_index], ordered[right_index]))
    return boundaries


def phase_regions(group: list[object], update_pressure: float) -> list[PhaseRegion]:
    if not group:
        return []
    key = (group[0].geometry, group[0].occupancy, group[0].object_count)
    if any((row.geometry, row.occupancy, row.object_count) != key for row in group):
        raise ValueError("phase group must share geometry, occupancy, and object count")

    # Every candidate is affine in Q. The lower envelope can only change at a
    # non-negative pairwise intersection, so evaluate one point in every interval.
    intersections = {0.0}
    for left_index in range(len(group)):
        for right_index in range(left_index + 1, len(group)):
            a = group[left_index]
            b = group[right_index]
            denominator = a.authenticated_path_page_reads - b.authenticated_path_page_reads
            if denominator == 0:
                continue
            numerator = (
                b.authenticated_path_bytes
                - a.authenticated_path_bytes
                + update_pressure
                * (
                    b.expected_cow_page_bytes_per_insert
                    - a.expected_cow_page_bytes_per_insert
                )
            )
            q = numerator / denominator
            if q > 0 and math.isfinite(q):
                intersections.add(q)

    points = sorted(intersections)
    intervals: list[tuple[float, float | None]] = []
    for index, start in enumerate(points):
        end = points[index + 1] if index + 1 < len(points) else None
        intervals.append((start, end))

    raw: list[tuple[int, float, float | None]] = []
    for start, end in intervals:
        probe = start + 1.0 if end is None else (start + end) / 2.0
        winner = min(
            group,
            key=lambda row: (
                normalized_cost(row, probe, update_pressure),
                row.page_size,
            ),
        )
        raw.append((winner.page_size, start, end))

    # Coalesce adjacent intervals won by the same page size. Some pairwise
    # intersections belong to lines that never reach the lower envelope.
    coalesced: list[tuple[int, float, float | None]] = []
    for page_size, start, end in raw:
        if coalesced and coalesced[-1][0] == page_size:
            old_page, old_start, _ = coalesced[-1]
            coalesced[-1] = (old_page, old_start, end)
        else:
            coalesced.append((page_size, start, end))

    return [
        PhaseRegion(
            geometry=key[0],
            occupancy=key[1],
            object_count=key[2],
            update_pressure=update_pressure,
            page_size=page_size,
            q_min_bytes=start,
            q_max_bytes=end,
        )
        for page_size, start, end in coalesced
    ]


def all_phase_regions(rows: list[object]) -> list[PhaseRegion]:
    grouped: dict[tuple[str, str, int], list[object]] = {}
    for row in rows:
        grouped.setdefault((row.geometry, row.occupancy, row.object_count), []).append(row)

    regions: list[PhaseRegion] = []
    for group in grouped.values():
        for update_pressure in UPDATE_PRESSURES:
            regions.extend(phase_regions(group, update_pressure))
    return regions


def self_check(boundaries: list[Boundary], regions: list[PhaseRegion]) -> None:
    boundary_lookup = {
        (
            item.geometry,
            item.occupancy,
            item.object_count,
            item.page_size_a,
            item.page_size_b,
        ): item
        for item in boundaries
    }
    b_4_16 = boundary_lookup[
        ("compact-128", "yao-random-insert", 100_000_000, 4096, 16384)
    ]
    b_16_64 = boundary_lookup[
        ("compact-128", "yao-random-insert", 100_000_000, 16384, 65536)
    ]

    assert math.isclose(b_4_16.q_intercept_bytes or -1, 20_480.0, abs_tol=1e-9)
    assert math.isclose(b_16_64.q_intercept_bytes or -1, 131_072.0, abs_tol=1e-9)
    assert 20_000 < (b_4_16.q_slope_bytes_per_update_pressure or 0) < 21_000
    assert 131_000 < (b_16_64.q_slope_bytes_per_update_pressure or 0) < 132_000

    selected = [
        item
        for item in regions
        if (
            item.geometry,
            item.occupancy,
            item.object_count,
            item.update_pressure,
        )
        == ("compact-128", "yao-random-insert", 100_000_000, 1.0)
    ]
    assert [item.page_size for item in selected] == list(PAGE_SIZES)
    assert math.isclose(selected[0].q_max_bytes or -1, 40_958.50581622361, rel_tol=1e-12)
    assert math.isclose(selected[1].q_max_bytes or -1, 262_143.27935169635, rel_tol=1e-12)
    assert selected[2].q_max_bytes is None


def print_csv(boundaries: list[Boundary], regions: list[PhaseRegion]) -> None:
    print("# pairwise_phase_boundaries: Q(U) = intercept + slope * U")
    print(
        "geometry,occupancy,objects,page_a,page_b,q_intercept_bytes,"
        "q_slope_bytes_per_update_pressure"
    )
    for item in boundaries:
        intercept = "" if item.q_intercept_bytes is None else f"{item.q_intercept_bytes:.9f}"
        slope = (
            ""
            if item.q_slope_bytes_per_update_pressure is None
            else f"{item.q_slope_bytes_per_update_pressure:.9f}"
        )
        print(
            f"{item.geometry},{item.occupancy},{item.object_count},{item.page_size_a},"
            f"{item.page_size_b},{intercept},{slope}"
        )

    print("# lower-envelope phase regions at representative update pressures")
    print("geometry,occupancy,objects,update_pressure,page_size,q_min_bytes,q_max_bytes")
    for item in regions:
        q_max = "" if item.q_max_bytes is None else f"{item.q_max_bytes:.9f}"
        print(
            f"{item.geometry},{item.occupancy},{item.object_count},{item.update_pressure:g},"
            f"{item.page_size},{item.q_min_bytes:.9f},{q_max}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="emit JSON")
    args = parser.parse_args()

    rows = all_rows()
    boundaries = all_boundaries(rows)
    regions = all_phase_regions(rows)
    self_check(boundaries, regions)

    if args.json:
        print(
            json.dumps(
                {
                    "update_pressures": list(UPDATE_PRESSURES),
                    "boundaries": [asdict(item) for item in boundaries],
                    "regions": [asdict(item) for item in regions],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_csv(boundaries, regions)


if __name__ == "__main__":
    main()
