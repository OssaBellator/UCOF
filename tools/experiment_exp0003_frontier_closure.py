#!/usr/bin/env python3
"""Compare nested state closures for EXP-0003 underflow repair outcomes."""

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

FULL_SEEDS = (3, 17, 29, 43, 71)
QUICK_SEEDS = (3, 17, 29)
FULL_CYCLES = 400_000
FULL_BURN_IN = 100_000
QUICK_CYCLES = 100_000
QUICK_BURN_IN = 20_000
SAMPLE_EVERY = 100

POLICIES = ("half-left-first", "half-fullest-borrow")


@dataclass(frozen=True)
class Trial:
    policy: str
    seed: int
    mean_fill: float
    interior_underflows: int
    global_iid_outcome_tv: float
    frontier_independent_outcome_tv: float
    frontier_error_reduction: float
    global_iid_merge_probability: float
    frontier_independent_merge_probability: float
    actual_merge_probability: float
    frontier_merge_relative_error: float
    global_left_probability: float
    frontier_left_probability: float
    actual_left_probability: float
    global_right_probability: float
    frontier_right_probability: float
    actual_right_probability: float


@dataclass(frozen=True)
class Aggregate:
    policy: str
    seeds: tuple[int, ...]
    mean_fill: float
    mean_interior_underflows: float
    global_iid_outcome_tv: float
    frontier_independent_outcome_tv: float
    frontier_error_reduction: float
    global_iid_merge_probability: float
    frontier_independent_merge_probability: float
    actual_merge_probability: float
    frontier_merge_relative_error: float
    global_left_probability: float
    frontier_left_probability: float
    actual_left_probability: float
    global_right_probability: float
    frontier_right_probability: float
    actual_right_probability: float


def make_initial_leaves() -> list[int]:
    total = round(CAPACITY * INITIAL_LEAVES * INITIAL_FILL)
    base, remainder = divmod(total, INITIAL_LEAVES)
    return [base + 1] * remainder + [base] * (INITIAL_LEAVES - remainder)


def choose_leaf(rng: random.Random, leaves: list[int], *, insertion: bool) -> int:
    maximum = CAPACITY + 1 if insertion else CAPACITY
    while True:
        index = rng.randrange(len(leaves))
        weight = leaves[index] + 1 if insertion else leaves[index]
        if rng.random() * maximum < weight:
            return index


def choose_borrow_side(leaves: list[int], index: int, policy: str) -> str | None:
    left_ok = index > 0 and leaves[index - 1] > MINIMUM
    right_ok = index + 1 < len(leaves) and leaves[index + 1] > MINIMUM

    if policy == "half-left-first":
        if left_ok:
            return "left"
        if right_ok:
            return "right"
        return None

    if policy == "half-fullest-borrow":
        if left_ok and right_ok:
            return "left" if leaves[index - 1] >= leaves[index + 1] else "right"
        if left_ok:
            return "left"
        if right_ok:
            return "right"
        return None

    raise ValueError(f"unknown policy: {policy}")


def independent_outcomes(
    left_probabilities: list[float],
    right_probabilities: list[float],
    policy: str,
) -> tuple[float, float, float]:
    left_minimum = left_probabilities[0]
    right_minimum = right_probabilities[0]
    merge = left_minimum * right_minimum

    if policy == "half-left-first":
        left = 1.0 - left_minimum
        right = left_minimum * (1.0 - right_minimum)
        return left, right, merge

    if policy == "half-fullest-borrow":
        left = 0.0
        right = 0.0
        for left_index, left_probability in enumerate(left_probabilities):
            if left_index == 0:
                continue
            left += left_probability * sum(right_probabilities[: left_index + 1])
        for right_index, right_probability in enumerate(right_probabilities):
            if right_index == 0:
                continue
            right += right_probability * sum(left_probabilities[:right_index])
        return left, right, merge

    raise ValueError(f"unknown policy: {policy}")


def total_variation(left: tuple[float, ...], right: tuple[float, ...]) -> float:
    return 0.5 * sum(abs(a - b) for a, b in zip(left, right))


def run_trial(policy: str, seed: int, cycles: int, burn_in: int) -> Trial:
    rng = random.Random(seed)
    leaves = make_initial_leaves()
    initial_keys = sum(leaves)

    global_counts = [0] * (CAPACITY - MINIMUM + 1)
    global_observations = 0
    left_frontier_counts = [0] * (CAPACITY - MINIMUM + 1)
    right_frontier_counts = [0] * (CAPACITY - MINIMUM + 1)
    interior_underflows = 0
    left_borrows = 0
    right_borrows = 0
    merges = 0
    fill_sum = 0.0
    fill_observations = 0

    def insert_one() -> None:
        index = choose_leaf(rng, leaves, insertion=True)
        if leaves[index] < CAPACITY:
            leaves[index] += 1
        else:
            leaves[index : index + 1] = [
                math.ceil((CAPACITY + 1) / 2),
                math.floor((CAPACITY + 1) / 2),
            ]

    def delete_one(*, observe: bool) -> None:
        nonlocal interior_underflows, left_borrows, right_borrows, merges

        index = choose_leaf(rng, leaves, insertion=False)
        leaves[index] -= 1
        if leaves[index] >= MINIMUM or len(leaves) == 1:
            return

        interior = 0 < index < len(leaves) - 1
        if observe and interior:
            left_frontier_counts[leaves[index - 1] - MINIMUM] += 1
            right_frontier_counts[leaves[index + 1] - MINIMUM] += 1
            interior_underflows += 1

        side = choose_borrow_side(leaves, index, policy)
        if side == "left":
            leaves[index - 1] -= 1
            leaves[index] += 1
            if observe and interior:
                left_borrows += 1
        elif side == "right":
            leaves[index + 1] -= 1
            leaves[index] += 1
            if observe and interior:
                right_borrows += 1
        elif index > 0:
            leaves[index - 1] += leaves[index]
            leaves.pop(index)
            if observe and interior:
                merges += 1
        else:
            leaves[index] += leaves[index + 1]
            leaves.pop(index + 1)

    for cycle in range(cycles):
        observe = cycle >= burn_in
        if rng.random() < 0.5:
            insert_one()
            delete_one(observe=observe)
        else:
            delete_one(observe=observe)
            insert_one()

        assert sum(leaves) == initial_keys
        assert all(MINIMUM <= count <= CAPACITY for count in leaves)

        if observe and (cycle - burn_in) % SAMPLE_EVERY == 0:
            fill_sum += initial_keys / (len(leaves) * CAPACITY)
            fill_observations += 1
            for occupancy in leaves:
                global_counts[occupancy - MINIMUM] += 1
                global_observations += 1

    global_probabilities = [count / global_observations for count in global_counts]
    left_frontier_probabilities = [
        count / interior_underflows for count in left_frontier_counts
    ]
    right_frontier_probabilities = [
        count / interior_underflows for count in right_frontier_counts
    ]

    global_prediction = independent_outcomes(
        global_probabilities,
        global_probabilities,
        policy,
    )
    frontier_prediction = independent_outcomes(
        left_frontier_probabilities,
        right_frontier_probabilities,
        policy,
    )
    actual = (
        left_borrows / interior_underflows,
        right_borrows / interior_underflows,
        merges / interior_underflows,
    )

    global_tv = total_variation(global_prediction, actual)
    frontier_tv = total_variation(frontier_prediction, actual)

    return Trial(
        policy=policy,
        seed=seed,
        mean_fill=fill_sum / fill_observations,
        interior_underflows=interior_underflows,
        global_iid_outcome_tv=global_tv,
        frontier_independent_outcome_tv=frontier_tv,
        frontier_error_reduction=1.0 - frontier_tv / global_tv,
        global_iid_merge_probability=global_prediction[2],
        frontier_independent_merge_probability=frontier_prediction[2],
        actual_merge_probability=actual[2],
        frontier_merge_relative_error=frontier_prediction[2] / actual[2] - 1.0,
        global_left_probability=global_prediction[0],
        frontier_left_probability=frontier_prediction[0],
        actual_left_probability=actual[0],
        global_right_probability=global_prediction[1],
        frontier_right_probability=frontier_prediction[1],
        actual_right_probability=actual[1],
    )


def aggregate(trials: list[Trial]) -> Aggregate:
    first = trials[0]
    assert all(trial.policy == first.policy for trial in trials)

    def mean(field: str) -> float:
        return statistics.fmean(getattr(trial, field) for trial in trials)

    return Aggregate(
        policy=first.policy,
        seeds=tuple(trial.seed for trial in trials),
        mean_fill=mean("mean_fill"),
        mean_interior_underflows=mean("interior_underflows"),
        global_iid_outcome_tv=mean("global_iid_outcome_tv"),
        frontier_independent_outcome_tv=mean("frontier_independent_outcome_tv"),
        frontier_error_reduction=mean("frontier_error_reduction"),
        global_iid_merge_probability=mean("global_iid_merge_probability"),
        frontier_independent_merge_probability=mean(
            "frontier_independent_merge_probability"
        ),
        actual_merge_probability=mean("actual_merge_probability"),
        frontier_merge_relative_error=mean("frontier_merge_relative_error"),
        global_left_probability=mean("global_left_probability"),
        frontier_left_probability=mean("frontier_left_probability"),
        actual_left_probability=mean("actual_left_probability"),
        global_right_probability=mean("global_right_probability"),
        frontier_right_probability=mean("frontier_right_probability"),
        actual_right_probability=mean("actual_right_probability"),
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
    by_policy = {result.policy: result for result in aggregates}
    left = by_policy["half-left-first"]
    fuller = by_policy["half-fullest-borrow"]

    assert left.frontier_independent_outcome_tv < left.global_iid_outcome_tv
    assert fuller.frontier_independent_outcome_tv < fuller.global_iid_outcome_tv
    assert left.frontier_independent_outcome_tv < 0.01
    assert fuller.frontier_independent_outcome_tv > left.frontier_independent_outcome_tv

    if not quick:
        assert left.frontier_error_reduction > 0.90
        assert fuller.frontier_error_reduction > 0.35
        assert abs(left.frontier_merge_relative_error) < 0.25
        assert fuller.frontier_merge_relative_error < -0.30


def print_csv(aggregates: list[Aggregate]) -> None:
    print(
        "policy,mean_fill,global_iid_outcome_tv,frontier_independent_outcome_tv,"
        "frontier_error_reduction,global_iid_merge,frontier_independent_merge,"
        "actual_merge,frontier_merge_relative_error,global_left,frontier_left,"
        "actual_left,global_right,frontier_right,actual_right"
    )
    for item in aggregates:
        print(
            f"{item.policy},{item.mean_fill:.9f},{item.global_iid_outcome_tv:.9f},"
            f"{item.frontier_independent_outcome_tv:.9f},"
            f"{item.frontier_error_reduction:.9f},"
            f"{item.global_iid_merge_probability:.9f},"
            f"{item.frontier_independent_merge_probability:.9f},"
            f"{item.actual_merge_probability:.9f},"
            f"{item.frontier_merge_relative_error:.9f},"
            f"{item.global_left_probability:.9f},{item.frontier_left_probability:.9f},"
            f"{item.actual_left_probability:.9f},{item.global_right_probability:.9f},"
            f"{item.frontier_right_probability:.9f},{item.actual_right_probability:.9f}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    trials, aggregates = run_ensemble(quick=args.quick)
    self_check(aggregates, quick=args.quick)

    if args.json:
        print(
            json.dumps(
                {
                    "configuration": {
                        "capacity": CAPACITY,
                        "minimum": MINIMUM,
                        "quick": args.quick,
                        "sample_every_cycles": SAMPLE_EVERY,
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
