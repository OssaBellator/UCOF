#!/usr/bin/env python3
"""Measure one-more-local-operation residuals for EXP-0003 frontier aggregates.

Experiment 0118 proved that 16,384 exact interior-underflow sibling pairs can be
compressed strongly when only the immediate repair reward and successor occupancy
bands are observed.  That is not enough to claim Markov lumpability.

This experiment starts from every exact underflow pair, performs the immediate
repair, and constructs an exact probability distribution for *one additional local
operation* inside the repaired window.  It then measures how different the rows
of microstates assigned to the same proposed macrostate remain.

The local kernel is deliberately bounded: insertion/deletion is selected with
probability 1/2, then a local leaf is selected proportional to its insertion gaps
or keys.  An underflow at a window boundary is classified as an escape because an
external sibling would be required to resolve it without making a hidden context
assumption.  Therefore this is a local aggregation diagnostic, not a complete tree
Markov model.
"""

from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from dataclasses import asdict, dataclass
from typing import Hashable

PAGE_SIZE = 16_384
PAGE_HEADER_LEN = 80
LEAF_ENTRY_LEN = 64
CAPACITY = (PAGE_SIZE - PAGE_HEADER_LEN) // LEAF_ENTRY_LEN
MINIMUM = math.ceil(CAPACITY / 2)
UNDERFLOW = MINIMUM - 1
MICROSTATES = (CAPACITY - MINIMUM + 1) ** 2

POLICIES = ("left-first", "fuller-sibling")
PARTITIONS = ("signature-only", "edge-aware", "edge-plus-comparison")

# The seven occupancy bands from Experiments 0116 and 0118.
BAND_WIDTHS = (1, 1, 6, 24, 32, 63, 1)


@dataclass(frozen=True)
class ImmediateRepair:
    action: str
    pages_emitted: int
    leaf_count_delta: int
    leaves: tuple[int, ...]


@dataclass(frozen=True)
class Result:
    policy: str
    partition: str
    macrostates: int
    microstates: int
    maximum_microstates_per_macrostate: int
    maximum_row_tv_from_macro_average: float
    microstate_weighted_mean_row_tv: float
    compression_ratio: float


def band_lookup() -> dict[int, int]:
    assert sum(BAND_WIDTHS) == CAPACITY - MINIMUM + 1
    lookup: dict[int, int] = {}
    occupancy = MINIMUM
    for band, width in enumerate(BAND_WIDTHS):
        for _ in range(width):
            lookup[occupancy] = band
            occupancy += 1
    assert occupancy == CAPACITY + 1
    return lookup


BAND = band_lookup()


def choose_action(left: int, right: int, policy: str) -> str:
    left_ok = left > MINIMUM
    right_ok = right > MINIMUM

    if policy == "left-first":
        if left_ok:
            return "borrow-left"
        if right_ok:
            return "borrow-right"
        return "merge-left"

    if policy == "fuller-sibling":
        if left_ok and right_ok:
            return "borrow-left" if left >= right else "borrow-right"
        if left_ok:
            return "borrow-left"
        if right_ok:
            return "borrow-right"
        return "merge-left"

    raise ValueError(f"unknown policy: {policy}")


def immediate_repair(left: int, right: int, policy: str) -> ImmediateRepair:
    action = choose_action(left, right, policy)

    if action == "borrow-left":
        return ImmediateRepair(
            action=action,
            pages_emitted=2,
            leaf_count_delta=0,
            leaves=(left - 1, MINIMUM, right),
        )
    if action == "borrow-right":
        return ImmediateRepair(
            action=action,
            pages_emitted=2,
            leaf_count_delta=0,
            leaves=(left, MINIMUM, right - 1),
        )

    assert left == MINIMUM and right == MINIMUM
    return ImmediateRepair(
        action=action,
        pages_emitted=1,
        leaf_count_delta=-1,
        leaves=(left + UNDERFLOW, right),
    )


def occupancy_bands(leaves: tuple[int, ...] | list[int]) -> tuple[int, ...]:
    return tuple(BAND[occupancy] for occupancy in leaves)


def edge_class(occupancy: int) -> tuple[int, bool, bool]:
    band = BAND[occupancy]
    decrement_crosses = occupancy > MINIMUM and BAND[occupancy - 1] != band
    increment_crosses = occupancy < CAPACITY and BAND[occupancy + 1] != band
    return band, decrement_crosses, increment_crosses


def outer_comparison(leaves: tuple[int, ...]) -> str:
    if len(leaves) != 3:
        return "not-applicable"
    if leaves[0] < leaves[2]:
        return "left-less"
    if leaves[0] > leaves[2]:
        return "left-greater"
    return "equal"


def macrostate(left: int, right: int, policy: str, partition: str) -> Hashable:
    repaired = immediate_repair(left, right, policy)
    base: tuple[Hashable, ...] = (
        repaired.action,
        repaired.pages_emitted,
        repaired.leaf_count_delta,
        occupancy_bands(repaired.leaves),
    )

    if partition == "signature-only":
        return base

    edge_state = tuple(edge_class(occupancy) for occupancy in repaired.leaves)
    if partition == "edge-aware":
        return base + (edge_state,)

    if partition == "edge-plus-comparison":
        return base + (edge_state, outer_comparison(repaired.leaves))

    raise ValueError(f"unknown partition: {partition}")


def repair_middle_underflow(leaves: list[int], policy: str) -> tuple[str, int, int]:
    assert len(leaves) == 3
    assert leaves[1] == UNDERFLOW

    action = choose_action(leaves[0], leaves[2], policy)
    if action == "borrow-left":
        leaves[0] -= 1
        leaves[1] += 1
        return action, 2, 0
    if action == "borrow-right":
        leaves[2] -= 1
        leaves[1] += 1
        return action, 2, 0

    leaves[0] += leaves[1]
    leaves.pop(1)
    return action, 1, -1


def add_probability(
    output: dict[Hashable, float],
    key: Hashable,
    probability: float,
) -> None:
    output[key] = output.get(key, 0.0) + probability


def local_next_operation_distribution(
    left: int,
    right: int,
    policy: str,
) -> dict[Hashable, float]:
    repaired = immediate_repair(left, right, policy)
    leaves = list(repaired.leaves)
    output: dict[Hashable, float] = {}

    insertion_denominator = sum(occupancy + 1 for occupancy in leaves)
    for index, occupancy in enumerate(leaves):
        probability = 0.5 * (occupancy + 1) / insertion_denominator
        successor = leaves.copy()
        if occupancy < CAPACITY:
            successor[index] += 1
            signature: Hashable = (
                "insert",
                1,
                0,
                occupancy_bands(successor),
            )
        else:
            successor[index : index + 1] = [
                math.ceil((CAPACITY + 1) / 2),
                math.floor((CAPACITY + 1) / 2),
            ]
            signature = (
                "insert-split",
                2,
                1,
                occupancy_bands(successor),
            )
        add_probability(output, signature, probability)

    deletion_denominator = sum(leaves)
    for index, occupancy in enumerate(leaves):
        probability = 0.5 * occupancy / deletion_denominator
        successor = leaves.copy()

        if occupancy > MINIMUM:
            successor[index] -= 1
            signature = (
                "delete",
                1,
                0,
                occupancy_bands(successor),
            )
            add_probability(output, signature, probability)
            continue

        # A boundary underflow requires an external sibling.  Record that context
        # escape rather than inventing a repair rule for a truncated local window.
        if index == 0:
            signature = (
                "delete-boundary-underflow-left",
                occupancy_bands(successor),
            )
            add_probability(output, signature, probability)
            continue
        if index == len(successor) - 1:
            signature = (
                "delete-boundary-underflow-right",
                occupancy_bands(successor),
            )
            add_probability(output, signature, probability)
            continue

        successor[index] -= 1
        action, pages_emitted, leaf_count_delta = repair_middle_underflow(
            successor,
            policy,
        )
        signature = (
            f"delete-{action}",
            pages_emitted,
            leaf_count_delta,
            occupancy_bands(successor),
        )
        add_probability(output, signature, probability)

    assert abs(sum(output.values()) - 1.0) < 1e-12
    return output


def total_variation(left: dict[Hashable, float], right: dict[Hashable, float]) -> float:
    keys = set(left) | set(right)
    return 0.5 * sum(abs(left.get(key, 0.0) - right.get(key, 0.0)) for key in keys)


def evaluate(policy: str, partition: str) -> Result:
    grouped: dict[Hashable, list[tuple[int, int]]] = defaultdict(list)
    for left in range(MINIMUM, CAPACITY + 1):
        for right in range(MINIMUM, CAPACITY + 1):
            grouped[macrostate(left, right, policy, partition)].append((left, right))

    maximum_residual = 0.0
    residual_sum = 0.0
    maximum_group = 0

    for microstates in grouped.values():
        maximum_group = max(maximum_group, len(microstates))
        rows = [
            local_next_operation_distribution(left, right, policy)
            for left, right in microstates
        ]

        average: dict[Hashable, float] = defaultdict(float)
        weight = 1.0 / len(rows)
        for row in rows:
            for key, probability in row.items():
                average[key] += weight * probability

        for row in rows:
            residual = total_variation(row, average)
            maximum_residual = max(maximum_residual, residual)
            residual_sum += residual

    return Result(
        policy=policy,
        partition=partition,
        macrostates=len(grouped),
        microstates=MICROSTATES,
        maximum_microstates_per_macrostate=maximum_group,
        maximum_row_tv_from_macro_average=maximum_residual,
        microstate_weighted_mean_row_tv=residual_sum / MICROSTATES,
        compression_ratio=MICROSTATES / len(grouped),
    )


def run() -> list[Result]:
    return [
        evaluate(policy, partition)
        for policy in POLICIES
        for partition in PARTITIONS
    ]


def self_check(results: list[Result]) -> None:
    assert CAPACITY == 254
    assert MINIMUM == 127
    assert MICROSTATES == 16_384

    keyed = {(item.policy, item.partition): item for item in results}

    left_signature = keyed[("left-first", "signature-only")]
    fuller_signature = keyed[("fuller-sibling", "signature-only")]
    left_edge = keyed[("left-first", "edge-aware")]
    fuller_edge = keyed[("fuller-sibling", "edge-aware")]
    left_compare = keyed[("left-first", "edge-plus-comparison")]
    fuller_compare = keyed[("fuller-sibling", "edge-plus-comparison")]

    assert left_signature.macrostates == 49
    assert fuller_signature.macrostates == 49
    assert 0.49 < left_signature.maximum_row_tv_from_macro_average < 0.52
    assert 0.49 < fuller_signature.maximum_row_tv_from_macro_average < 0.52

    assert left_edge.macrostates == 225
    assert fuller_edge.macrostates == 225
    assert left_edge.maximum_row_tv_from_macro_average < 0.04
    assert fuller_edge.maximum_row_tv_from_macro_average > 0.10

    assert left_compare.macrostates == 233
    assert fuller_compare.macrostates == 237
    assert left_compare.maximum_row_tv_from_macro_average < 0.04
    assert fuller_compare.maximum_row_tv_from_macro_average < 0.04

    # The comparison bit is policy-relevant: it should collapse the fuller-sibling
    # worst-case residual substantially while barely changing left-first.
    assert fuller_compare.maximum_row_tv_from_macro_average < (
        fuller_edge.maximum_row_tv_from_macro_average / 3.0
    )
    assert abs(
        left_compare.maximum_row_tv_from_macro_average
        - left_edge.maximum_row_tv_from_macro_average
    ) < 0.001


def print_csv(results: list[Result]) -> None:
    print(
        "policy,partition,macrostates,microstates,maximum_microstates_per_macrostate,"
        "maximum_row_tv_from_macro_average,microstate_weighted_mean_row_tv,"
        "compression_ratio"
    )
    for item in results:
        print(
            f"{item.policy},{item.partition},{item.macrostates},{item.microstates},"
            f"{item.maximum_microstates_per_macrostate},"
            f"{item.maximum_row_tv_from_macro_average:.12f},"
            f"{item.microstate_weighted_mean_row_tv:.12f},"
            f"{item.compression_ratio:.6f}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    results = run()
    self_check(results)

    if args.json:
        print(
            json.dumps(
                {
                    "configuration": {
                        "capacity": CAPACITY,
                        "minimum": MINIMUM,
                        "microstates": MICROSTATES,
                        "band_widths": BAND_WIDTHS,
                        "kernel": "one local operation after immediate underflow repair",
                        "boundary_underflow": "escape; external sibling context required",
                        "macro_average_weighting": "uniform over exact microstates",
                    },
                    "results": [asdict(item) for item in results],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_csv(results)


if __name__ == "__main__":
    main()
