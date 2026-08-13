#!/usr/bin/env python3
"""Quantify EXP-0003 ObjectId scope/width regimes.

The model deliberately separates three contracts that need different mathematics:

1. coordinated local allocation: uniqueness is checked/allocated inside one output;
2. independent random generation: collision probability is probabilistic;
3. independent dense local allocation followed by no-remap combination: collisions are
   deterministic whenever the allocated ranges overlap.

It also compares the 64-bit nonzero identifier cardinality with a conservative upper
bound on the number of minimum-size object records addressable by EXP-0003's u64
physical offset space.  The result is a namespace-contract diagnostic, not a width
selection by itself.
"""

from __future__ import annotations

import argparse
import json
import math
from dataclasses import asdict, dataclass

U64_MAX = (1 << 64) - 1
NONZERO_ID64 = U64_MAX
OBJECT_HEADER_VARIANTS = (48, 56, 64)
OBJECT_COUNTS = (1_000_000, 100_000_000, 1_000_000_000)
MERGE_COUNTS = (
    (1_000_000, 1_000_000),
    (100_000_000, 100_000_000),
    (1_000_000_000, 1_000_000_000),
)


@dataclass(frozen=True)
class PhysicalBound:
    object_header_bytes: int
    addressable_bytes: int
    maximum_minimum_size_records: int
    nonzero_64bit_ids: int
    fraction_of_id_space_needed: float
    id_space_to_record_bound_ratio: float


@dataclass(frozen=True)
class RandomCollision:
    bits: int
    object_count: int
    birthday_collision_probability: float


@dataclass(frozen=True)
class MergeCollision:
    bits: int
    left_objects: int
    right_objects: int
    random_cross_namespace_collision_probability: float
    dense_local_same_origin_conflicts: int


def birthday_probability(bits: int, count: int) -> float:
    """Poisson birthday approximation for uniform independent identifiers."""
    exponent = count * (count - 1) / (2 * (2**bits))
    return -math.expm1(-exponent)


def random_cross_collision_probability(bits: int, left: int, right: int) -> float:
    """Poisson approximation for at least one collision across two random sets."""
    exponent = left * right / (2**bits)
    return -math.expm1(-exponent)


def physical_bounds() -> list[PhysicalBound]:
    rows = []
    for header in OBJECT_HEADER_VARIANTS:
        maximum_records = U64_MAX // header
        rows.append(
            PhysicalBound(
                object_header_bytes=header,
                addressable_bytes=U64_MAX,
                maximum_minimum_size_records=maximum_records,
                nonzero_64bit_ids=NONZERO_ID64,
                fraction_of_id_space_needed=maximum_records / NONZERO_ID64,
                id_space_to_record_bound_ratio=NONZERO_ID64 / maximum_records,
            )
        )
    return rows


def random_rows() -> list[RandomCollision]:
    return [
        RandomCollision(
            bits=bits,
            object_count=count,
            birthday_collision_probability=birthday_probability(bits, count),
        )
        for bits in (64, 128)
        for count in OBJECT_COUNTS
    ]


def merge_rows() -> list[MergeCollision]:
    return [
        MergeCollision(
            bits=bits,
            left_objects=left,
            right_objects=right,
            random_cross_namespace_collision_probability=(
                random_cross_collision_probability(bits, left, right)
            ),
            dense_local_same_origin_conflicts=min(left, right),
        )
        for bits in (64, 128)
        for left, right in MERGE_COUNTS
    ]


def self_check(
    bounds: list[PhysicalBound],
    random: list[RandomCollision],
    merges: list[MergeCollision],
) -> None:
    # Even the smallest candidate object header consumes enough physical bytes that a
    # u64 file address space cannot contain anywhere near all nonzero 64-bit IDs as
    # distinct minimum-size object records.
    assert all(row.maximum_minimum_size_records < row.nonzero_64bit_ids for row in bounds)
    assert min(row.id_space_to_record_bound_ratio for row in bounds) >= 48.0

    random_by_key = {(row.bits, row.object_count): row for row in random}
    assert random_by_key[(64, 100_000_000)].birthday_collision_probability > 2.0e-4
    assert random_by_key[(64, 1_000_000_000)].birthday_collision_probability > 0.02
    assert random_by_key[(128, 1_000_000_000)].birthday_collision_probability < 2.0e-21

    merge_by_key = {(row.bits, row.left_objects, row.right_objects): row for row in merges}
    assert (
        merge_by_key[(64, 100_000_000, 100_000_000)]
        .random_cross_namespace_collision_probability
        > 5.0e-4
    )
    assert (
        merge_by_key[(64, 1_000_000_000, 1_000_000_000)]
        .random_cross_namespace_collision_probability
        > 0.05
    )
    assert (
        merge_by_key[(128, 1_000_000_000, 1_000_000_000)]
        .random_cross_namespace_collision_probability
        < 3.0e-21
    )

    # If two independent file-local allocators both assign dense IDs from one, a
    # no-remap combination has a deterministic overlap regardless of identifier width.
    assert all(row.dense_local_same_origin_conflicts == row.left_objects for row in merges)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    bounds = physical_bounds()
    random = random_rows()
    merges = merge_rows()
    self_check(bounds, random, merges)

    if args.json:
        print(
            json.dumps(
                {
                    "physical_bounds": [asdict(row) for row in bounds],
                    "random_collision": [asdict(row) for row in random],
                    "merge_collision": [asdict(row) for row in merges],
                },
                indent=2,
                sort_keys=True,
            )
        )
        return

    print("physical_bound,header_bytes,max_min_records,id_space_fraction,id_space_ratio")
    for row in bounds:
        print(
            f"physical_bound,{row.object_header_bytes},"
            f"{row.maximum_minimum_size_records},"
            f"{row.fraction_of_id_space_needed:.12g},"
            f"{row.id_space_to_record_bound_ratio:.6f}"
        )

    print("random_collision,bits,objects,birthday_probability")
    for row in random:
        print(
            f"random_collision,{row.bits},{row.object_count},"
            f"{row.birthday_collision_probability:.12g}"
        )

    print("merge_collision,bits,left,right,random_cross_probability,dense_conflicts")
    for row in merges:
        print(
            f"merge_collision,{row.bits},{row.left_objects},{row.right_objects},"
            f"{row.random_cross_namespace_collision_probability:.12g},"
            f"{row.dense_local_same_origin_conflicts}"
        )


if __name__ == "__main__":
    main()
