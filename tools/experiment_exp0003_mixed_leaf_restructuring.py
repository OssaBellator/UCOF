#!/usr/bin/env python3
"""Stress EXP-0003 leaf occupancy and restructuring under mixed updates.

This is deterministic-seed stochastic evidence, not an equilibrium proof and not a
full-tree simulator. It keeps UCOF's proposed insertion split exact and compares
leaf-level deletion repair policies under random-key deletion and random-gap
insertion.

The model reports immutable leaf-page emission and directional borrow behavior in
addition to occupancy. Recursive parent repair is outside this model, so measured
restructuring cost is a lower bound on full-tree copy-on-write cost.
"""

from __future__ import annotations

import argparse
import json
import math
import random
import statistics
from dataclasses import asdict, dataclass

PAGE_SIZE = 16_384
PAGE_HEADER_LEN = 80
LEAF_ENTRY_LEN = 64
CAPACITY = (PAGE_SIZE - PAGE_HEADER_LEN) // LEAF_ENTRY_LEN
INITIAL_LEAVES = 32
INITIAL_FILL = 0.70
FULL_SEEDS = (3, 17, 29, 43, 71)
QUICK_SEEDS = (3, 17, 29)
FULL_CYCLES = 800_000
FULL_BURN_IN = 200_000
QUICK_CYCLES = 60_000
QUICK_BURN_IN = 10_000


@dataclass(frozen=True)
class Policy:
    name: str
    threshold_kind: str
    borrow_strategy: str


POLICIES = (
    Policy("free-empty-reference", "free-empty", "none"),
    Policy("quarter-left-first", "quarter", "left-first"),
    Policy("half-left-first", "half", "left-first"),
    Policy("half-fullest-borrow", "half", "fullest-first"),
)


@dataclass(frozen=True)
class Trial:
    policy: str
    seed: int
    cycles: int
    burn_in_cycles: int
    repair_threshold: int
    initial_keys: int
    mean_fill: float
    mean_leaf_count: float
    split_rate: float
    left_borrow_rate: float
    right_borrow_rate: float
    merge_rate: float
    free_rate: float
    restructure_rate: float
    parent_boundary_change_rate: float
    emitted_leaf_pages_per_op: float
    emitted_leaf_bytes_per_op: float


@dataclass(frozen=True)
class Aggregate:
    policy: str
    seeds: tuple[int, ...]
    cycles_per_seed: int
    burn_in_cycles: int
    repair_threshold: int
    initial_keys: int
    mean_fill: float
    stddev_fill: float
    mean_leaf_count: float
    stddev_leaf_count: float
    split_rate: float
    left_borrow_rate: float
    right_borrow_rate: float
    borrow_rate: float
    merge_rate: float
    free_rate: float
    restructure_rate: float
    parent_boundary_change_rate: float
    emitted_leaf_pages_per_op: float
    emitted_leaf_bytes_per_op: float
    left_borrow_share: float | None


def threshold(policy: Policy) -> int:
    if policy.threshold_kind == "free-empty":
        return 1
    if policy.threshold_kind == "quarter":
        return math.ceil(CAPACITY / 4)
    if policy.threshold_kind == "half":
        return math.ceil(CAPACITY / 2)
    raise ValueError(f"unknown threshold kind: {policy.threshold_kind}")


def make_initial_leaves() -> list[int]:
    total = round(CAPACITY * INITIAL_LEAVES * INITIAL_FILL)
    base, remainder = divmod(total, INITIAL_LEAVES)
    leaves = [base + 1] * remainder + [base] * (INITIAL_LEAVES - remainder)
    assert sum(leaves) == total
    assert all(1 <= count <= CAPACITY for count in leaves)
    return leaves


def choose_leaf(
    rng: random.Random,
    leaves: list[int],
    *,
    insertion: bool,
) -> int:
    """Sample the leaf-level marginal of a uniformly random gap or key."""

    maximum = CAPACITY + 1 if insertion else CAPACITY
    while True:
        index = rng.randrange(len(leaves))
        weight = leaves[index] + 1 if insertion else leaves[index]
        if rng.random() * maximum < weight:
            return index


def choose_borrow_side(
    leaves: list[int],
    index: int,
    repair_threshold: int,
    strategy: str,
) -> str | None:
    left_ok = index > 0 and leaves[index - 1] > repair_threshold
    right_ok = index + 1 < len(leaves) and leaves[index + 1] > repair_threshold

    if strategy == "left-first":
        if left_ok:
            return "left"
        if right_ok:
            return "right"
        return None

    if strategy == "fullest-first":
        if left_ok and right_ok:
            # Deterministic: fuller lender first, left on an exact tie.
            return "left" if leaves[index - 1] >= leaves[index + 1] else "right"
        if left_ok:
            return "left"
        if right_ok:
            return "right"
        return None

    if strategy == "none":
        return None
    raise ValueError(f"unknown borrow strategy: {strategy}")


def run_trial(policy: Policy, seed: int, cycles: int, burn_in: int) -> Trial:
    if not 0 <= burn_in < cycles:
        raise ValueError("burn-in must be in [0, cycles)")

    rng = random.Random(seed)
    leaves = make_initial_leaves()
    initial_keys = sum(leaves)
    repair_threshold = threshold(policy)

    split = left_borrow = right_borrow = left_merge = right_merge = free = 0
    emitted_pages = operations = 0
    fill_sum = leaf_count_sum = 0.0
    observations = 0

    def insert_one() -> None:
        nonlocal split, emitted_pages, operations
        index = choose_leaf(rng, leaves, insertion=True)
        if leaves[index] < CAPACITY:
            leaves[index] += 1
            emitted_pages += 1
        else:
            overflow = CAPACITY + 1
            left = math.ceil(overflow / 2)
            right = math.floor(overflow / 2)
            assert left >= math.ceil(CAPACITY / 2)
            assert right >= math.ceil(CAPACITY / 2)
            leaves[index : index + 1] = [left, right]
            split += 1
            emitted_pages += 2
        operations += 1

    def delete_one() -> None:
        nonlocal left_borrow, right_borrow, left_merge, right_merge
        nonlocal free, emitted_pages, operations

        index = choose_leaf(rng, leaves, insertion=False)
        leaves[index] -= 1

        if policy.threshold_kind == "free-empty":
            if leaves[index] == 0 and len(leaves) > 1:
                leaves.pop(index)
                free += 1
            else:
                emitted_pages += 1
            operations += 1
            return

        if leaves[index] >= repair_threshold or len(leaves) == 1:
            emitted_pages += 1
            operations += 1
            return

        side = choose_borrow_side(
            leaves,
            index,
            repair_threshold,
            policy.borrow_strategy,
        )
        if side == "left":
            leaves[index - 1] -= 1
            leaves[index] += 1
            left_borrow += 1
            emitted_pages += 2
        elif side == "right":
            leaves[index + 1] -= 1
            leaves[index] += 1
            right_borrow += 1
            emitted_pages += 2
        elif index > 0:
            # Merge direction stays left-first for every threshold/borrow variant.
            leaves[index - 1] += leaves[index]
            assert leaves[index - 1] <= CAPACITY
            leaves.pop(index)
            left_merge += 1
            emitted_pages += 1
        elif index + 1 < len(leaves):
            leaves[index] += leaves[index + 1]
            assert leaves[index] <= CAPACITY
            leaves.pop(index + 1)
            right_merge += 1
            emitted_pages += 1
        else:
            emitted_pages += 1

        operations += 1

    for cycle in range(cycles):
        # One insertion and one deletion keep live cardinality fixed; randomizing
        # their order avoids privileging one transient state within each cycle.
        if rng.random() < 0.5:
            insert_one()
            delete_one()
        else:
            delete_one()
            insert_one()

        assert sum(leaves) == initial_keys
        assert all(0 < count <= CAPACITY for count in leaves)

        if cycle >= burn_in:
            fill_sum += initial_keys / (len(leaves) * CAPACITY)
            leaf_count_sum += len(leaves)
            observations += 1

    borrows = left_borrow + right_borrow
    merges = left_merge + right_merge
    restructuring = split + borrows + merges + free
    boundary_changes = split + merges + free
    pages_per_op = emitted_pages / operations

    return Trial(
        policy=policy.name,
        seed=seed,
        cycles=cycles,
        burn_in_cycles=burn_in,
        repair_threshold=repair_threshold,
        initial_keys=initial_keys,
        mean_fill=fill_sum / observations,
        mean_leaf_count=leaf_count_sum / observations,
        split_rate=split / operations,
        left_borrow_rate=left_borrow / operations,
        right_borrow_rate=right_borrow / operations,
        merge_rate=merges / operations,
        free_rate=free / operations,
        restructure_rate=restructuring / operations,
        parent_boundary_change_rate=boundary_changes / operations,
        emitted_leaf_pages_per_op=pages_per_op,
        emitted_leaf_bytes_per_op=pages_per_op * PAGE_SIZE,
    )


def aggregate(trials: list[Trial]) -> Aggregate:
    if not trials:
        raise ValueError("cannot aggregate zero trials")
    first = trials[0]
    assert all(trial.policy == first.policy for trial in trials)

    def mean(field: str) -> float:
        return statistics.fmean(getattr(trial, field) for trial in trials)

    left = mean("left_borrow_rate")
    right = mean("right_borrow_rate")
    borrow = left + right

    return Aggregate(
        policy=first.policy,
        seeds=tuple(trial.seed for trial in trials),
        cycles_per_seed=first.cycles,
        burn_in_cycles=first.burn_in_cycles,
        repair_threshold=first.repair_threshold,
        initial_keys=first.initial_keys,
        mean_fill=mean("mean_fill"),
        stddev_fill=statistics.pstdev(trial.mean_fill for trial in trials),
        mean_leaf_count=mean("mean_leaf_count"),
        stddev_leaf_count=statistics.pstdev(trial.mean_leaf_count for trial in trials),
        split_rate=mean("split_rate"),
        left_borrow_rate=left,
        right_borrow_rate=right,
        borrow_rate=borrow,
        merge_rate=mean("merge_rate"),
        free_rate=mean("free_rate"),
        restructure_rate=mean("restructure_rate"),
        parent_boundary_change_rate=mean("parent_boundary_change_rate"),
        emitted_leaf_pages_per_op=mean("emitted_leaf_pages_per_op"),
        emitted_leaf_bytes_per_op=mean("emitted_leaf_bytes_per_op"),
        left_borrow_share=(left / borrow) if borrow else None,
    )


def run_ensemble(*, quick: bool) -> tuple[list[Trial], list[Aggregate]]:
    seeds = QUICK_SEEDS if quick else FULL_SEEDS
    cycles = QUICK_CYCLES if quick else FULL_CYCLES
    burn_in = QUICK_BURN_IN if quick else FULL_BURN_IN

    all_trials: list[Trial] = []
    aggregates: list[Aggregate] = []
    for policy in POLICIES:
        trials = [run_trial(policy, seed, cycles, burn_in) for seed in seeds]
        all_trials.extend(trials)
        aggregates.append(aggregate(trials))
    return all_trials, aggregates


def self_check(aggregates: list[Aggregate], *, quick: bool) -> None:
    assert CAPACITY == 254
    by_name = {result.policy: result for result in aggregates}

    free = by_name["free-empty-reference"]
    quarter = by_name["quarter-left-first"]
    half = by_name["half-left-first"]
    fullest = by_name["half-fullest-borrow"]

    assert free.repair_threshold == 1
    assert quarter.repair_threshold == math.ceil(CAPACITY / 4)
    assert half.repair_threshold == math.ceil(CAPACITY / 2)
    assert fullest.repair_threshold == half.repair_threshold

    # The short CI ensemble is intentionally too short to claim long-horizon
    # ordering of the slowly mixing low-threshold occupancy processes. It verifies
    # structural semantics and the sibling-choice signal only.
    assert half.restructure_rate > quarter.restructure_rate
    assert half.borrow_rate > quarter.borrow_rate
    assert half.left_borrow_share is not None
    assert half.left_borrow_share > 0.6
    assert fullest.left_borrow_share is not None
    assert fullest.left_borrow_share < half.left_borrow_share

    if not quick:
        # These are evidence-run checks, not mathematical constants. Fixed seeds and
        # a long burn-in make accidental model changes visible while the document
        # continues to label the output finite-horizon Monte Carlo evidence.
        assert free.mean_fill < quarter.mean_fill < half.mean_fill
        assert fullest.restructure_rate < half.restructure_rate


def print_csv(aggregates: list[Aggregate]) -> None:
    print(
        "policy,repair_threshold,mean_fill,stddev_fill,mean_leaf_count,stddev_leaf_count,"
        "split_rate,borrow_rate,left_borrow_rate,right_borrow_rate,left_borrow_share,"
        "merge_rate,free_rate,restructure_rate,parent_boundary_change_rate,"
        "emitted_leaf_pages_per_op,emitted_leaf_bytes_per_op"
    )
    for result in aggregates:
        left_share = "" if result.left_borrow_share is None else f"{result.left_borrow_share:.9f}"
        print(
            f"{result.policy},{result.repair_threshold},{result.mean_fill:.9f},"
            f"{result.stddev_fill:.9f},{result.mean_leaf_count:.9f},"
            f"{result.stddev_leaf_count:.9f},{result.split_rate:.12g},"
            f"{result.borrow_rate:.12g},{result.left_borrow_rate:.12g},"
            f"{result.right_borrow_rate:.12g},{left_share},{result.merge_rate:.12g},"
            f"{result.free_rate:.12g},{result.restructure_rate:.12g},"
            f"{result.parent_boundary_change_rate:.12g},"
            f"{result.emitted_leaf_pages_per_op:.12g},"
            f"{result.emitted_leaf_bytes_per_op:.6f}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--quick", action="store_true", help="use the shorter deterministic CI ensemble")
    parser.add_argument("--json", action="store_true", help="emit JSON")
    args = parser.parse_args()

    trials, aggregates = run_ensemble(quick=args.quick)
    self_check(aggregates, quick=args.quick)

    if args.json:
        print(
            json.dumps(
                {
                    "configuration": {
                        "page_size": PAGE_SIZE,
                        "page_header_len": PAGE_HEADER_LEN,
                        "leaf_entry_len": LEAF_ENTRY_LEN,
                        "capacity": CAPACITY,
                        "initial_leaves": INITIAL_LEAVES,
                        "initial_fill": INITIAL_FILL,
                        "quick": args.quick,
                    },
                    "aggregates": [asdict(result) for result in aggregates],
                    "trials": [asdict(result) for result in trials],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_csv(aggregates)


if __name__ == "__main__":
    main()
