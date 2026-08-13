#!/usr/bin/env python3
"""Enumerate exact one-step reward/successor partitions for EXP-0003 underflow repair.

This experiment is deliberately deterministic.  It enumerates every legal pair of
interior sibling occupancies when the target has just underflowed from M to M-1,
then asks how many exact microstates can be grouped without changing:

* the repair action;
* leaf pages emitted;
* leaf-count delta; or
* the post-repair occupancy bands.

The resulting partition is a candidate state reduction for a later Markov reward
model.  It is NOT a proof of multi-step Markov lumpability: two microstates with
the same one-step signature can still have different future transition laws.
"""

from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from dataclasses import asdict, dataclass

PAGE_SIZE = 16_384
PAGE_HEADER_LEN = 80
LEAF_ENTRY_LEN = 64
CAPACITY = (PAGE_SIZE - PAGE_HEADER_LEN) // LEAF_ENTRY_LEN
MINIMUM = math.ceil(CAPACITY / 2)
UNDERFLOW = MINIMUM - 1
MICROSTATES = (CAPACITY - MINIMUM + 1) ** 2

POLICIES = ("left-first", "fuller-sibling")

# Widths cover exact legal occupancies M..=C.  The 7-band scheme matches
# Experiment 0116; the surrounding schemes expose the fidelity/state-count tradeoff.
SCHEMES: tuple[tuple[str, tuple[int, ...]], ...] = (
    ("eligibility-only", (1, 127)),
    ("3-band", (1, 31, 96)),
    ("5-band", (1, 1, 14, 48, 64)),
    ("7-band", (1, 1, 6, 24, 32, 63, 1)),
    ("9-band", (1, 1, 2, 4, 8, 16, 32, 63, 1)),
)


@dataclass(frozen=True)
class Repair:
    action: str
    pages_emitted: int
    leaf_count_delta: int
    post_left: int
    post_target: int | None
    post_right: int


@dataclass(frozen=True)
class Result:
    scheme: str
    policy: str
    bands: int
    microstates: int
    immediate_reward_classes: int
    input_band_cells: int
    input_cells_needing_refinement: int
    max_signatures_per_input_cell: int
    input_refined_state_count: int
    unique_reward_successor_signatures: int
    signature_compression_ratio: float


def band_lookup(widths: tuple[int, ...]) -> dict[int, int]:
    assert sum(widths) == CAPACITY - MINIMUM + 1
    lookup: dict[int, int] = {}
    occupancy = MINIMUM
    for band, width in enumerate(widths):
        for _ in range(width):
            lookup[occupancy] = band
            occupancy += 1
    assert occupancy == CAPACITY + 1
    return lookup


def repair(left: int, right: int, policy: str) -> Repair:
    assert MINIMUM <= left <= CAPACITY
    assert MINIMUM <= right <= CAPACITY

    left_ok = left > MINIMUM
    right_ok = right > MINIMUM

    if policy == "left-first":
        if left_ok:
            action = "borrow-left"
        elif right_ok:
            action = "borrow-right"
        else:
            action = "merge-left"
    elif policy == "fuller-sibling":
        if left_ok and right_ok:
            action = "borrow-left" if left >= right else "borrow-right"
        elif left_ok:
            action = "borrow-left"
        elif right_ok:
            action = "borrow-right"
        else:
            action = "merge-left"
    else:
        raise ValueError(f"unknown policy: {policy}")

    if action == "borrow-left":
        return Repair(
            action=action,
            pages_emitted=2,
            leaf_count_delta=0,
            post_left=left - 1,
            post_target=MINIMUM,
            post_right=right,
        )
    if action == "borrow-right":
        return Repair(
            action=action,
            pages_emitted=2,
            leaf_count_delta=0,
            post_left=left,
            post_target=MINIMUM,
            post_right=right - 1,
        )

    # For an interior half-full underflow, merge is reachable only when neither
    # sibling can lend, hence left=right=M.  The underflow target has M-1 entries.
    assert left == MINIMUM and right == MINIMUM
    return Repair(
        action=action,
        pages_emitted=1,
        leaf_count_delta=-1,
        post_left=left + UNDERFLOW,
        post_target=None,
        post_right=right,
    )


def reward_signature(item: Repair) -> tuple[str, int, int]:
    return item.action, item.pages_emitted, item.leaf_count_delta


def successor_signature(
    item: Repair,
    lookup: dict[int, int],
) -> tuple[str, int, int, int, int | None, int]:
    return (
        item.action,
        item.pages_emitted,
        item.leaf_count_delta,
        lookup[item.post_left],
        None if item.post_target is None else lookup[item.post_target],
        lookup[item.post_right],
    )


def evaluate(scheme: str, widths: tuple[int, ...], policy: str) -> Result:
    lookup = band_lookup(widths)
    signatures: set[tuple[str, int, int, int, int | None, int]] = set()
    reward_classes: set[tuple[str, int, int]] = set()
    per_input_cell: dict[
        tuple[int, int], set[tuple[str, int, int, int, int | None, int]]
    ] = defaultdict(set)

    for left in range(MINIMUM, CAPACITY + 1):
        for right in range(MINIMUM, CAPACITY + 1):
            item = repair(left, right, policy)
            signature = successor_signature(item, lookup)
            signatures.add(signature)
            reward_classes.add(reward_signature(item))
            per_input_cell[(lookup[left], lookup[right])].add(signature)

    input_cells = len(widths) ** 2
    assert len(per_input_cell) == input_cells
    cell_signature_counts = [len(values) for values in per_input_cell.values()]

    return Result(
        scheme=scheme,
        policy=policy,
        bands=len(widths),
        microstates=MICROSTATES,
        immediate_reward_classes=len(reward_classes),
        input_band_cells=input_cells,
        input_cells_needing_refinement=sum(count > 1 for count in cell_signature_counts),
        max_signatures_per_input_cell=max(cell_signature_counts),
        input_refined_state_count=sum(cell_signature_counts),
        unique_reward_successor_signatures=len(signatures),
        signature_compression_ratio=MICROSTATES / len(signatures),
    )


def run() -> list[Result]:
    return [
        evaluate(scheme, widths, policy)
        for scheme, widths in SCHEMES
        for policy in POLICIES
    ]


def self_check(results: list[Result]) -> None:
    assert CAPACITY == 254
    assert MINIMUM == 127
    assert UNDERFLOW == 126
    assert MICROSTATES == 16_384
    assert all(item.immediate_reward_classes == 3 for item in results)

    keyed = {(item.scheme, item.policy): item for item in results}
    left7 = keyed[("7-band", "left-first")]
    fuller7 = keyed[("7-band", "fuller-sibling")]

    assert left7.input_band_cells == 49
    assert fuller7.input_band_cells == 49
    assert left7.input_cells_needing_refinement == 32
    assert fuller7.input_cells_needing_refinement == 32
    assert left7.max_signatures_per_input_cell == 2
    assert fuller7.max_signatures_per_input_cell == 3
    assert left7.input_refined_state_count == 81
    assert fuller7.input_refined_state_count == 85
    assert left7.unique_reward_successor_signatures == 49
    assert fuller7.unique_reward_successor_signatures == 49

    # Finer successor bands should not reduce the number of exact signatures.
    for policy in POLICIES:
        counts = [keyed[(scheme, policy)].unique_reward_successor_signatures for scheme, _ in SCHEMES]
        assert counts == sorted(counts)


def print_csv(results: list[Result]) -> None:
    print(
        "scheme,policy,bands,microstates,immediate_reward_classes,input_band_cells,"
        "input_cells_needing_refinement,max_signatures_per_input_cell,"
        "input_refined_state_count,unique_reward_successor_signatures,"
        "signature_compression_ratio"
    )
    for item in results:
        print(
            f"{item.scheme},{item.policy},{item.bands},{item.microstates},"
            f"{item.immediate_reward_classes},{item.input_band_cells},"
            f"{item.input_cells_needing_refinement},{item.max_signatures_per_input_cell},"
            f"{item.input_refined_state_count},{item.unique_reward_successor_signatures},"
            f"{item.signature_compression_ratio:.6f}"
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
                        "underflow": UNDERFLOW,
                        "microstates": MICROSTATES,
                        "schemes": {name: widths for name, widths in SCHEMES},
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
