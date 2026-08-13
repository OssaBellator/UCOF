#!/usr/bin/env python3
"""Measure EXP-0003 underflow/split frontier arrival hazards.

This experiment is a bridge between the finite-trace reward decomposition and a
future Markov/semi-Markov model.  It deliberately does not assume that successive
repair frontiers are iid renewals or that a small local post-repair state is
Markovian.

For the random-key/random-gap leaf model, two frontier-arrival probabilities are
available exactly from the current global occupancy state:

    P(underflow on the next deletion | state)
        = MINIMUM * count(leaves == MINIMUM) / live_keys

    P(split on the next insertion | state)
        = (CAPACITY + 1) * count(leaves == CAPACITY)
          / (live_keys + leaf_count)

Those identities separate *arrival* from *repair outcome*.  Borrow-versus-merge
still depends on sibling context, while immutable page-write reward still depends
on parent/root structure.  The policy can therefore change long-run cost by
changing how much stationary/finite-horizon mass sits at the minimum/full
occupancy frontiers even though the conditional hazard formula itself is common.

The model keeps cardinality fixed with one insertion and one deletion per cycle,
randomizes operation order, and compares half-full left-first borrowing with the
experimental fuller-sibling/left-on-tie rule.  Results are deterministic-seed
Monte Carlo evidence, not an equilibrium proof and not an epoch decision.
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
MINIMUM = math.ceil(CAPACITY / 2)
INITIAL_LEAVES = 32
INITIAL_FILL = 0.70

QUICK_SEEDS = (3, 17, 29)
QUICK_CYCLES = 200_000
QUICK_BURN_IN = 50_000
FULL_SEEDS = (3, 17, 29, 43, 71)
FULL_CYCLES = 800_000
FULL_BURN_IN = 200_000

POLICIES = ("left-first", "fuller-sibling")


@dataclass(frozen=True)
class Trial:
    policy: str
    seed: int
    cycles: int
    burn_in_cycles: int
    observed_cycles: int
    live_keys: int
    mean_leaf_count: float
    mean_minimum_leaf_count: float
    mean_full_leaf_count: float
    observed_underflows: int
    expected_underflows: float
    underflow_rate_per_operation: float
    predicted_underflow_rate_per_operation: float
    underflow_relative_residual: float
    borrows: int
    merges: int
    borrow_share_of_underflows: float
    observed_splits: int
    expected_splits: float
    split_rate_per_operation: float
    predicted_split_rate_per_operation: float
    split_relative_residual: float
    complete_underflow_intervals: int
    mean_complete_underflow_holding_operations: float
    reciprocal_underflow_rate_operations: float


@dataclass(frozen=True)
class Aggregate:
    policy: str
    seeds: tuple[int, ...]
    cycles_per_seed: int
    burn_in_cycles: int
    observed_operations: int
    live_keys: int
    mean_leaf_count: float
    mean_minimum_leaf_count: float
    mean_full_leaf_count: float
    observed_underflows: int
    expected_underflows: float
    underflow_rate_per_operation: float
    predicted_underflow_rate_per_operation: float
    underflow_relative_residual: float
    borrows: int
    merges: int
    borrow_share_of_underflows: float
    observed_splits: int
    expected_splits: float
    split_rate_per_operation: float
    predicted_split_rate_per_operation: float
    split_relative_residual: float
    complete_underflow_intervals: int
    mean_complete_underflow_holding_operations: float
    reciprocal_underflow_rate_operations: float


def make_initial_leaves() -> list[int]:
    total = round(CAPACITY * INITIAL_LEAVES * INITIAL_FILL)
    base, remainder = divmod(total, INITIAL_LEAVES)
    leaves = [base + 1] * remainder + [base] * (INITIAL_LEAVES - remainder)
    assert sum(leaves) == total
    assert all(MINIMUM <= occupancy <= CAPACITY for occupancy in leaves)
    return leaves


def choose_leaf(rng: random.Random, leaves: list[int], *, insertion: bool) -> int:
    """Sample a uniformly random live key or insertion gap without flattening."""

    maximum = CAPACITY + 1 if insertion else CAPACITY
    while True:
        index = rng.randrange(len(leaves))
        weight = leaves[index] + 1 if insertion else leaves[index]
        if rng.random() * maximum < weight:
            return index


def choose_borrow_side(leaves: list[int], index: int, policy: str) -> str | None:
    left_ok = index > 0 and leaves[index - 1] > MINIMUM
    right_ok = index + 1 < len(leaves) and leaves[index + 1] > MINIMUM

    if policy == "left-first":
        if left_ok:
            return "left"
        if right_ok:
            return "right"
        return None

    if policy == "fuller-sibling":
        if left_ok and right_ok:
            return "left" if leaves[index - 1] >= leaves[index + 1] else "right"
        if left_ok:
            return "left"
        if right_ok:
            return "right"
        return None

    raise ValueError(f"unknown policy: {policy}")


def relative_residual(observed: float, expected: float) -> float:
    if expected == 0.0:
        return 0.0 if observed == 0.0 else math.inf
    return (observed - expected) / expected


def run_trial(policy: str, seed: int, cycles: int, burn_in: int) -> Trial:
    if policy not in POLICIES:
        raise ValueError(f"unknown policy: {policy}")
    if not 0 <= burn_in < cycles:
        raise ValueError("burn-in must be in [0, cycles)")

    rng = random.Random(seed)
    leaves = make_initial_leaves()
    live_keys = sum(leaves)

    observed_underflows = borrows = merges = observed_splits = 0
    expected_underflows = expected_splits = 0.0
    minimum_leaf_count_sum = full_leaf_count_sum = leaf_count_sum = 0.0
    observed_cycles = 0

    seen_underflow = False
    operations_since_underflow = 0
    complete_underflow_holding_times: list[int] = []

    def insert_one(*, collect: bool) -> None:
        nonlocal observed_splits, expected_splits, operations_since_underflow

        if collect:
            operations_since_underflow += 1
            full_leaves = sum(occupancy == CAPACITY for occupancy in leaves)
            expected_splits += (
                (CAPACITY + 1) * full_leaves / (live_keys + len(leaves))
            )

        index = choose_leaf(rng, leaves, insertion=True)
        if leaves[index] < CAPACITY:
            leaves[index] += 1
            return

        overflow = CAPACITY + 1
        leaves[index : index + 1] = [
            math.ceil(overflow / 2),
            math.floor(overflow / 2),
        ]
        if collect:
            observed_splits += 1

    def delete_one(*, collect: bool) -> None:
        nonlocal observed_underflows, expected_underflows, borrows, merges
        nonlocal seen_underflow, operations_since_underflow

        if collect:
            operations_since_underflow += 1
            minimum_leaves = sum(occupancy == MINIMUM for occupancy in leaves)
            expected_underflows += MINIMUM * minimum_leaves / live_keys

        index = choose_leaf(rng, leaves, insertion=False)
        leaves[index] -= 1

        if leaves[index] >= MINIMUM or len(leaves) == 1:
            return

        side = choose_borrow_side(leaves, index, policy)
        if side == "left":
            leaves[index - 1] -= 1
            leaves[index] += 1
            if collect:
                borrows += 1
        elif side == "right":
            leaves[index + 1] -= 1
            leaves[index] += 1
            if collect:
                borrows += 1
        elif index > 0:
            leaves[index - 1] += leaves[index]
            assert leaves[index - 1] <= CAPACITY
            leaves.pop(index)
            if collect:
                merges += 1
        else:
            leaves[index] += leaves[index + 1]
            assert leaves[index] <= CAPACITY
            leaves.pop(index + 1)
            if collect:
                merges += 1

        if collect:
            observed_underflows += 1
            if seen_underflow:
                complete_underflow_holding_times.append(operations_since_underflow)
            seen_underflow = True
            operations_since_underflow = 0

    for cycle in range(cycles):
        collect = cycle >= burn_in
        if rng.random() < 0.5:
            insert_one(collect=collect)
            delete_one(collect=collect)
        else:
            delete_one(collect=collect)
            insert_one(collect=collect)

        assert sum(leaves) == live_keys
        assert all(MINIMUM <= occupancy <= CAPACITY for occupancy in leaves)

        if collect:
            observed_cycles += 1
            minimum_leaf_count_sum += sum(
                occupancy == MINIMUM for occupancy in leaves
            )
            full_leaf_count_sum += sum(occupancy == CAPACITY for occupancy in leaves)
            leaf_count_sum += len(leaves)

    observed_operations = 2 * observed_cycles
    underflow_rate = observed_underflows / observed_operations
    predicted_underflow_rate = expected_underflows / observed_operations
    split_rate = observed_splits / observed_operations
    predicted_split_rate = expected_splits / observed_operations

    assert observed_underflows == borrows + merges
    assert len(complete_underflow_holding_times) == max(observed_underflows - 1, 0)

    return Trial(
        policy=policy,
        seed=seed,
        cycles=cycles,
        burn_in_cycles=burn_in,
        observed_cycles=observed_cycles,
        live_keys=live_keys,
        mean_leaf_count=leaf_count_sum / observed_cycles,
        mean_minimum_leaf_count=minimum_leaf_count_sum / observed_cycles,
        mean_full_leaf_count=full_leaf_count_sum / observed_cycles,
        observed_underflows=observed_underflows,
        expected_underflows=expected_underflows,
        underflow_rate_per_operation=underflow_rate,
        predicted_underflow_rate_per_operation=predicted_underflow_rate,
        underflow_relative_residual=relative_residual(
            float(observed_underflows), expected_underflows
        ),
        borrows=borrows,
        merges=merges,
        borrow_share_of_underflows=borrows / observed_underflows,
        observed_splits=observed_splits,
        expected_splits=expected_splits,
        split_rate_per_operation=split_rate,
        predicted_split_rate_per_operation=predicted_split_rate,
        split_relative_residual=relative_residual(
            float(observed_splits), expected_splits
        ),
        complete_underflow_intervals=len(complete_underflow_holding_times),
        mean_complete_underflow_holding_operations=statistics.fmean(
            complete_underflow_holding_times
        ),
        reciprocal_underflow_rate_operations=1.0 / underflow_rate,
    )


def aggregate(trials: list[Trial]) -> Aggregate:
    if not trials:
        raise ValueError("cannot aggregate zero trials")
    first = trials[0]
    assert all(trial.policy == first.policy for trial in trials)
    assert all(trial.cycles == first.cycles for trial in trials)
    assert all(trial.burn_in_cycles == first.burn_in_cycles for trial in trials)
    assert all(trial.live_keys == first.live_keys for trial in trials)

    observed_operations = sum(2 * trial.observed_cycles for trial in trials)
    observed_underflows = sum(trial.observed_underflows for trial in trials)
    expected_underflows = sum(trial.expected_underflows for trial in trials)
    observed_splits = sum(trial.observed_splits for trial in trials)
    expected_splits = sum(trial.expected_splits for trial in trials)
    borrows = sum(trial.borrows for trial in trials)
    merges = sum(trial.merges for trial in trials)

    # Every trial has the same number of observed cycles, so arithmetic means are
    # also the exact cycle-weighted means for the occupancy statistics.
    def mean(field: str) -> float:
        return statistics.fmean(getattr(trial, field) for trial in trials)

    complete_intervals = sum(trial.complete_underflow_intervals for trial in trials)
    weighted_holding_sum = sum(
        trial.mean_complete_underflow_holding_operations
        * trial.complete_underflow_intervals
        for trial in trials
    )

    underflow_rate = observed_underflows / observed_operations
    predicted_underflow_rate = expected_underflows / observed_operations
    split_rate = observed_splits / observed_operations
    predicted_split_rate = expected_splits / observed_operations

    return Aggregate(
        policy=first.policy,
        seeds=tuple(trial.seed for trial in trials),
        cycles_per_seed=first.cycles,
        burn_in_cycles=first.burn_in_cycles,
        observed_operations=observed_operations,
        live_keys=first.live_keys,
        mean_leaf_count=mean("mean_leaf_count"),
        mean_minimum_leaf_count=mean("mean_minimum_leaf_count"),
        mean_full_leaf_count=mean("mean_full_leaf_count"),
        observed_underflows=observed_underflows,
        expected_underflows=expected_underflows,
        underflow_rate_per_operation=underflow_rate,
        predicted_underflow_rate_per_operation=predicted_underflow_rate,
        underflow_relative_residual=relative_residual(
            float(observed_underflows), expected_underflows
        ),
        borrows=borrows,
        merges=merges,
        borrow_share_of_underflows=borrows / observed_underflows,
        observed_splits=observed_splits,
        expected_splits=expected_splits,
        split_rate_per_operation=split_rate,
        predicted_split_rate_per_operation=predicted_split_rate,
        split_relative_residual=relative_residual(
            float(observed_splits), expected_splits
        ),
        complete_underflow_intervals=complete_intervals,
        mean_complete_underflow_holding_operations=(
            weighted_holding_sum / complete_intervals
        ),
        reciprocal_underflow_rate_operations=1.0 / underflow_rate,
    )


def run_ensemble(*, quick: bool) -> tuple[list[Trial], list[Aggregate]]:
    seeds = QUICK_SEEDS if quick else FULL_SEEDS
    cycles = QUICK_CYCLES if quick else FULL_CYCLES
    burn_in = QUICK_BURN_IN if quick else FULL_BURN_IN

    trials: list[Trial] = []
    aggregates: list[Aggregate] = []
    for policy in POLICIES:
        policy_trials = [run_trial(policy, seed, cycles, burn_in) for seed in seeds]
        trials.extend(policy_trials)
        aggregates.append(aggregate(policy_trials))
    return trials, aggregates


def self_check(aggregates: list[Aggregate], *, quick: bool) -> None:
    assert CAPACITY == 254
    assert MINIMUM == 127
    by_policy = {item.policy: item for item in aggregates}
    left = by_policy["left-first"]
    fuller = by_policy["fuller-sibling"]

    # Underflow is common enough that the exact conditional hazard should close
    # tightly even in the shorter deterministic CI ensemble.
    assert abs(left.underflow_relative_residual) < 0.03
    assert abs(fuller.underflow_relative_residual) < 0.03

    # Splits are much rarer, so tolerate larger finite-sample residuals while still
    # requiring the exact conditional hazard to have the right scale.
    split_tolerance = 0.20 if quick else 0.12
    assert abs(left.split_relative_residual) < split_tolerance
    assert abs(fuller.split_relative_residual) < split_tolerance

    # These are deterministic finite-horizon evidence checks, not mathematical
    # constants.  They guard the policy signal that motivates the renewal model.
    assert fuller.mean_minimum_leaf_count < left.mean_minimum_leaf_count
    assert fuller.underflow_rate_per_operation < left.underflow_rate_per_operation
    assert (
        fuller.mean_complete_underflow_holding_operations
        > left.mean_complete_underflow_holding_operations
    )
    assert fuller.mean_full_leaf_count < left.mean_full_leaf_count
    assert fuller.split_rate_per_operation < left.split_rate_per_operation


def print_csv(aggregates: list[Aggregate]) -> None:
    print(
        "policy,seeds,cycles_per_seed,burn_in_cycles,observed_operations,live_keys,"
        "mean_leaf_count,mean_minimum_leaf_count,mean_full_leaf_count,"
        "observed_underflows,expected_underflows,underflow_rate_per_operation,"
        "predicted_underflow_rate_per_operation,underflow_relative_residual,"
        "borrows,merges,borrow_share_of_underflows,observed_splits,expected_splits,"
        "split_rate_per_operation,predicted_split_rate_per_operation,"
        "split_relative_residual,complete_underflow_intervals,"
        "mean_complete_underflow_holding_operations,reciprocal_underflow_rate_operations"
    )
    for item in aggregates:
        seeds = "+".join(str(seed) for seed in item.seeds)
        print(
            f"{item.policy},{seeds},{item.cycles_per_seed},{item.burn_in_cycles},"
            f"{item.observed_operations},{item.live_keys},{item.mean_leaf_count:.9f},"
            f"{item.mean_minimum_leaf_count:.9f},{item.mean_full_leaf_count:.9f},"
            f"{item.observed_underflows},{item.expected_underflows:.9f},"
            f"{item.underflow_rate_per_operation:.12g},"
            f"{item.predicted_underflow_rate_per_operation:.12g},"
            f"{item.underflow_relative_residual:.9f},{item.borrows},{item.merges},"
            f"{item.borrow_share_of_underflows:.9f},{item.observed_splits},"
            f"{item.expected_splits:.9f},{item.split_rate_per_operation:.12g},"
            f"{item.predicted_split_rate_per_operation:.12g},"
            f"{item.split_relative_residual:.9f},{item.complete_underflow_intervals},"
            f"{item.mean_complete_underflow_holding_operations:.9f},"
            f"{item.reciprocal_underflow_rate_operations:.9f}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--quick", action="store_true", help="use the shorter deterministic CI ensemble"
    )
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
                        "minimum": MINIMUM,
                        "initial_leaves": INITIAL_LEAVES,
                        "initial_fill": INITIAL_FILL,
                        "quick": args.quick,
                    },
                    "aggregates": [asdict(item) for item in aggregates],
                    "trials": [asdict(item) for item in trials],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_csv(aggregates)


if __name__ == "__main__":
    main()
