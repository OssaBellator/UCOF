#!/usr/bin/env python3
"""Stress EXP-0003 leaf occupancy and restructuring under mixed updates.

This is a deterministic-seed stochastic model, not an equilibrium proof and not a
full-tree simulator. It keeps UCOF's proposed insertion split exact and compares
leaf-level deletion repair policies under random-key deletion and random-gap
insertion.

The model deliberately reports immutable leaf-page emission and directional
borrow behavior in addition to occupancy. Recursive parent repair is outside this
model, so restructuring costs are lower bounds on full-tree copy-on-write cost.
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
    Policy(
        name="free-empty-reference",
        threshold_kind="free-empty",
        borrow_strategy="none",
    ),
    Policy(
        name="quarter-left-first",
        threshold_kind="quarter",
        borrow_strategy="left-first",
    ),
    Policy(
        name="half-left-first",
        threshold_kind="half",
        borrow_strategy="left-first",
    ),
    Policy(
        name="half-fullest-borrow",
        threshold_kind="half",
        borrow_strategy="fullest-first",
    ),
)


@dataclass(frozen=True)
class TrialResult:
    policy: str
    seed: int
    cycles: int
    burn_in_cycles: int
    capacity: int
    repair_threshold: int
    initial_keys: int
    mean_fill: float
    mean_leaf_count: float
    split_rate_per_op: float
    left_borrow_rate_per_op: float
    right_borrow_rate_per_op: float
    borrow_rate_per_op: float
    left_merge_rate_per_op: float
    right_merge_rate_per_op: float
    merge_rate_per_op: float
    free_rate_per_op: float
    restructuring_rate_per_op: float
    parent_boundary_change_rate_per_op: float
    emitted_leaf_pages_per_op: float
    emitted_leaf_bytes_per_op: float
    left_share_of_borrows: float | None


@dataclass(frozen=True)
class AggregateResult:
    policy: str
    seeds: list[int]
    cycles_per_seed: int
    burn_in_cycles: int
    capacity: int
    repair_threshold: int
    initial_keys: int
    mean_fill: float
    stddev_fill: float
    mean_leaf_count: float
    stddev_leaf_count: float
    split_rate_per_op: float
    borrow_rate_per_op: float
    merge_rate_per_op: float
    free_rate_per_op: float
    restructuring_rate_per_op: float
    parent_boundary_change_rate_per_op: float
    emitted_leaf_pages_per_op: float
    emitted_leaf_bytes_per_op: float
    left_borrow_rate_per_op: float
    right_borrow_rate_per_op: float
    left_share_of_borrows: float | None


def repair_threshold(policy: Policy, capacity: int) -> int:
    if policy.threshold_kind == "free-empty":
        return 1
    if policy.threshold_kind == "quarter":
        return math.ceil(capacity / 4)
    if policy.threshold_kind == "half":
        return math.ceil(capacity / 2)
    raise ValueError(f"unknown threshold kind: {policy.threshold_kind}")


def initial_counts(capacity: int, leaf_count: int, fill: float) -> list[int]:
    if leaf_count <= 0:
        raise ValueError("leaf_count must be positive")
    if not 0.0 < fill <= 1.0:
        raise ValueError("fill must be in (0, 1]")

    total = round(capacity * leaf_count * fill)
    base, remainder = divmod(total, leaf_count)
    counts = [base + 1] * remainder + [base] * (leaf_count - remainder)
    assert sum(counts) == total
    assert all(1 <= count <= capacity for count in counts)
    return counts


def choose_weighted_leaf(
    rng: random.Random,
    leaves: list[int],
    *,
    insertion: bool,
    capacity: int,
) -> int:
    """Choose a leaf by random-gap or random-key weight using rejection sampling.

    For insertion, a leaf with n keys has n+1 insertion gaps. For deletion, a
    leaf with n keys has n deletable keys. Sampling proportional to those weights
    gives the leaf-level marginal of uniformly random gaps/keys without storing a
    Fenwick tree for this small stress model.
    """

    maximum = capacity + 1 if insertion else capacity
    while True:
        index = rng.randrange(len(leaves))
        weight = leaves[index] + 1 if insertion else leaves[index]
        if rng.random() * maximum < weight:
            return index


def choose_borrow_side(
    leaves: list[int],
    index: int,
    threshold: int,
    strategy: str,
) -> str | None:
    left_ok = index > 0 and leaves[index - 1] > threshold
    right_ok = index + 1 < len(leaves) and leaves[index + 1] > threshold

    if strategy == "left-first":
        if left_ok:
            return "left"
        if right_ok:
            return "right"
        return None

    if strategy == "fullest-first":
        if left_ok and right_ok:
            # Deterministic: use the fuller lender, with left as the exact tie-break.
            return "left" if leaves[index - 1] >= leaves[index + 1] else "right"
        if left_ok:
            return "left"
        if right_ok:
            return "right"
        return None

    if strategy == "none":
        return None
    raise ValueError(f"unknown borrow strategy: {strategy}")


def run_trial(
    policy: Policy,
    seed: int,
    cycles: int,
    burn_in_cycles: int,
    *,
    capacity: int = CAPACITY,
    leaf_count: int = INITIAL_LEAVES,
    initial_fill: float = INITIAL_FILL,
) -> TrialResult:
    if burn_in_cycles >= cycles:
        raise ValueError("burn-in must be smaller than total cycles")

    rng = random.Random(seed)
    leaves = initial_counts(capacity, leaf_count, initial_fill)
    initial_keys = sum(leaves)
    threshold = repair_threshold(policy, capacity)

    counters = {
        "split": 0,
        "left_borrow": 0,
        "right_borrow": 0,
        "left_merge": 0,
        "right_merge": 0,
        "free": 0,
        "emitted_leaf_pages": 0,
        "operations": 0,
    }

    fill_sum = 0.0
    leaf_count_sum = 0.0
    observations = 0

    def insert_one() -> None:
        index = choose_weighted_leaf(
            rng,
            leaves,
            insertion=True,
            capacity=capacity,
        )
        if leaves[index] < capacity:
            leaves[index] += 1
            counters["emitted_leaf_pages"] += 1
        else:
            overflow = capacity + 1
            left = math.ceil(overflow / 2)
            right = math.floor(overflow / 2)
            assert left >= math.ceil(capacity / 2)
            assert right >= math.ceil(capacity / 2)
            leaves[index : index + 1] = [left, right]
            counters["split"] += 1
            counters["emitted_leaf_pages"] += 2
        counters["operations"] += 1

    def delete_one() -> None:
        index = choose_weighted_leaf(
            rng,
            leaves,
            insertion=False,
            capacity=capacity,
        )
        leaves[index] -= 1

        if policy.threshold_kind == "free-empty":
            if leaves[index] == 0 and len(leaves) > 1:
                # Reference policy: remove an empty leaf instead of borrowing or
                # merging at a positive occupancy threshold.
                leaves.pop(index)
                counters["free"] += 1
            else:
                counters["emitted_leaf_pages"] += 1
            counters["operations"] += 1
            return

        if leaves[index] >= threshold or len(leaves) == 1:
            counters["emitted_leaf_pages"] += 1
            counters["operations"] += 1
            return

        side = choose_borrow_side(
            leaves,
            index,
            threshold,
            policy.borrow_strategy,
        )
        if side == "left":
            leaves[index - 1] -= 1
            leaves[index] += 1
            counters["left_borrow"] += 1
            counters["emitted_leaf_pages"] += 2
        elif side == "right":
            leaves[index + 1] -= 1
            leaves[index] += 1
            counters["right_borrow"] += 1
            counters["emitted_leaf_pages"] += 2
        elif index > 0:
            # Keep UCOF's current deterministic merge direction for every policy
            # here. The fullest-borrow alternative isolates only the lender choice.
            leaves[index - 1] += leaves[index]
            assert leaves[index - 1] <= capacity
            leaves.pop(index)
            counters["left_merge"] += 1
            counters["emitted_leaf_pages"] += 1
        elif index + 1 < len(leaves):
            leaves[index] += leaves[index + 1]
            assert leaves[index] <= capacity
            leaves.pop(index + 1)
            counters["right_merge"] += 1
            counters["emitted_leaf_pages"] += 1
        else:
            counters["emitted_leaf_pages"] += 1

        counters["operations"] += 1

    for cycle in range(cycles):
        # Each cycle has one insertion and one deletion, with randomized order.
        # Sampling after the pair keeps the live key count exactly constant.
        if rng.random() < 0.5:
            insert_one()
            delete_one()
        else:
            delete_one()
            insert_one()

        assert sum(leaves) == initial_keys
        assert all(0 < count <= capacity for count in leaves)

        if cycle >= burn_in_cycles:
            fill_sum += initial_keys / (len(leaves) * capacity)
            leaf_count_sum += len(leaves)
            observations += 1

    operations = counters["operations"]
    borrows = counters["left_borrow"] + counters["right_borrow"]
    merges = counters["left_merge"] + counters["right_merge"]
    restructuring = counters["split"] + borrows + merges + counters["free"]
    parent_boundary_changes = counters["split"] + merges + counters["free"]
    left_share = counters["left_borrow"] / borrows if borrows else None
    emitted_pages = counters["emitted_leaf_pages"] / operations

    return TrialResult(
        policy=policy.name,
        seed=seed,
        cycles=cycles,
        burn_in_cycles=burn_in_cycles,
        capacity=capacity,
        repair_threshold=threshold,
        initial_keys=initial_keys,
        mean_fill=fill_sum / observations,
        mean_leaf_count=leaf_count_sum / observations,
        split_rate_per_op=counters["split"] / operations,
        left_borrow_rate_per_op=counters["left_borrow"] / operations,
        right_borrow_rate_per_op=counters["right_borrow"] / operations,
        borrow_rate_per_op=borrows / operations,
        left_merge_rate_per_op=counters["left_merge"] / operations,
        right_merge_rate_per_op=counters["right_merge"] / operations,
        merge_rate_per_op=merges / operations,
        free_rate_per_op=counters["free"] / operations,
        restructuring_rate_per_op=restructuring / operations,
        parent_boundary_change_rate_per_op=parent_boundary_changes / operations,
        emitted_leaf_pages_per_op=emitted_pages,
        emitted_leaf_bytes_per_op=emitted_pages * PAGE_SIZE,
        left_share_of_borrows=left_share,
    )


def aggregate(trials: list[TrialResult]) -> AggregateResult:
    if not trials:
        raise ValueError("cannot aggregate zero trials")
    first = trials[0]
    if any(trial.policy != first.policy for trial in trials):
        raise ValueError("all trials must share a policy")

    def mean(field: str) -> float:
        return statistics.fmean(getattr(trial, field) for trial in trials)

    left = mean("left_borrow_rate_per_op")
    right = mean("right_borrow_rate_per_op")
    borrow = left + right

    return AggregateResult(
        policy=first.policy,
        seeds=[trial.seed for trial in trials],
        cycles_per_seed=first.cycles,
        burn_in_cycles=first.burn_in_cycles,
        capacity=first.capacity,
        repair_threshold=first.repair_threshold,
        initial_keys=first.initial_keys,
        mean_fill=mean("mean_fill"),
        stddev_fill=statistics.pstdev(trial.mean_fill for trial in trials),
        mean_leaf_count=mean("mean_leaf_count"),
        stddev_leaf_count=statistics.pstdev(trial.mean_leaf_count for trial in trials),
        split_rate_per_op=mean("split_rate_per_op"),
        borrow_rate_per_op=mean("borrow_rate_per_op"),
        merge_rate_per_op=mean("merge_rate_per_op"),
        free_rate_per_op=mean("free_rate_per_op"),
        restructuring_rate_per_op=mean("restructuring_rate_per_op"),
        parent_boundary_change_rate_per_op=mean("parent_boundary_change_rate_per_op"),
        emitted_leaf_pages_per_op=mean("emitted_leaf_pages_per_op"),
        emitted_leaf_bytes_per_op=mean("emitted_leaf_bytes_per_op"),
        left_borrow_rate_per_op=left,
        right_borrow_rate_per_op=right,
        left_share_of_borrows=(left / borrow) if borrow else None,
    )


def run_ensemble(*, quick: bool) -> tuple[list[TrialResult], list[AggregateResult]]:
    seeds = QUICK_SEEDS if quick else FULL_SEEDS
    cycles = QUICK_CYCLES if quick else FULL_CYCLES
    burn_in = QUICK_BURN_IN if quick else FULL_BURN_IN

    trials: list[TrialResult] = []
    aggregates: list[AggregateResult] = []
    for policy in POLICIES:
        policy_trials = [
            run_trial(policy, seed, cycles, burn_in)
            for seed in seeds
        ]
        trials.extend(policy_trials)
        aggregates.append(aggregate(policy_trials))
    return trials, aggregates


def self_check(aggregates: list[AggregateResult]) -> None:
    assert CAPACITY == 254
    by_name = {result.policy: result for result in aggregates}

    free = by_name["free-empty-reference"]
    quarter = by_name["quarter-left-first"]
    half = by_name["half-left-first"]
    fullest = by_name["half-fullest-borrow"]

    # These are intentionally broad deterministic-seed checks. They protect the
    # qualitative model from accidental semantic changes without pretending the
    # Monte Carlo estimates are mathematical constants.
    assert free.mean_fill < quarter.mean_fill < half.mean_fill
    assert half.restructuring_rate_per_op > quarter.restructuring_rate_per_op
    assert half.borrow_rate_per_op > quarter.borrow_rate_per_op
    assert half.left_share_of_borrows is not None
    assert half.left_share_of_borrows > 0.6

    # Changing only lender selection should preserve the half-full floor while
    # reducing the directional bias in this workload envelope.
    assert fullest.repair_threshold == half.repair_threshold
    assert fullest.left_share_of_borrows is not None
    assert fullest.left_share_of_borrows < half.left_share_of_borrows


def print_csv(aggregates: list[AggregateResult]) -> None:
    print(
        "policy,repair_threshold,mean_fill,stddev_fill,mean_leaf_count,stddev_leaf_count,"
        "split_rate,borrow_rate,left_borrow_rate,right_borrow_rate,left_borrow_share,"
        "merge_rate,free_rate,restructuring_rate,parent_boundary_change_rate,"
        "emitted_leaf_pages_per_op,emitted_leaf_bytes_per_op"
    )
    for result in aggregates:
        left_share = "" if result.left_share_of_borrows is None else f"{result.left_share_of_borrows:.9f}"
        print(
            f"{result.policy},{result.repair_threshold},{result.mean_fill:.9f},"
            f"{result.stddev_fill:.9f},{result.mean_leaf_count:.9f},"
            f"{result.stddev_leaf_count:.9f},{result.split_rate_per_op:.12g},"
            f"{result.borrow_rate_per_op:.12g},{result.left_borrow_rate_per_op:.12g},"
            f"{result.right_borrow_rate_per_op:.12g},{left_share},"
            f"{result.merge_rate_per_op:.12g},{result.free_rate_per_op:.12g},"
            f"{result.restructuring_rate_per_op:.12g},"
            f"{result.parent_boundary_change_rate_per_op:.12g},"
            f"{result.emitted_leaf_pages_per_op:.12g},"
            f"{result.emitted_leaf_bytes_per_op:.6f}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--quick",
        action="store_true",
        help="use the shorter deterministic CI ensemble",
    )
    parser.add_argument("--json", action="store_true", help="emit JSON")
    args = parser.parse_args()

    trials, aggregates = run_ensemble(quick=args.quick)
    self_check(aggregates)

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
