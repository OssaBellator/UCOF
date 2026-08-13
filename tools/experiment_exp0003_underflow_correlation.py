#!/usr/bin/env python3
"""Measure the local occupancy correlation that defeats the EXP-0003 iid model."""

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
BAND_NAMES = (
    "M",
    "M+1",
    "M+2..M+7",
    "M+8..M+31",
    "M+32..M+63",
    "M+64..C-1",
    "C",
)


@dataclass(frozen=True)
class Trial:
    policy: str
    seed: int
    cycles: int
    burn_in_cycles: int
    mean_fill: float
    global_minimum_fraction: float
    adjacent_band_tv_from_iid: float
    adjacent_band_mutual_information_bits: float
    underflow_band_tv_from_global_iid: float
    underflow_neighbor_mutual_information_bits: float
    underflow_left_neighbor_tv_from_global: float
    underflow_right_neighbor_tv_from_global: float
    interior_underflow_events: int
    boundary_underflow_events: int
    predicted_left_borrow_share_per_underflow: float
    predicted_right_borrow_share_per_underflow: float
    predicted_merge_share_per_underflow: float
    actual_left_borrow_share_per_underflow: float
    actual_right_borrow_share_per_underflow: float
    actual_merge_share_per_underflow: float
    outcome_tv_from_iid: float
    merge_multiplier_over_iid: float


@dataclass(frozen=True)
class Aggregate:
    policy: str
    seeds: tuple[int, ...]
    cycles_per_seed: int
    burn_in_cycles: int
    mean_fill: float
    global_minimum_fraction: float
    adjacent_band_tv_from_iid: float
    adjacent_band_mutual_information_bits: float
    underflow_band_tv_from_global_iid: float
    underflow_neighbor_mutual_information_bits: float
    underflow_left_neighbor_tv_from_global: float
    underflow_right_neighbor_tv_from_global: float
    mean_interior_underflow_events: float
    mean_boundary_underflow_events: float
    predicted_left_borrow_share_per_underflow: float
    predicted_right_borrow_share_per_underflow: float
    predicted_merge_share_per_underflow: float
    actual_left_borrow_share_per_underflow: float
    actual_right_borrow_share_per_underflow: float
    actual_merge_share_per_underflow: float
    outcome_tv_from_iid: float
    merge_multiplier_over_iid: float


def make_initial_leaves() -> list[int]:
    total = round(CAPACITY * INITIAL_LEAVES * INITIAL_FILL)
    base, remainder = divmod(total, INITIAL_LEAVES)
    return [base + 1] * remainder + [base] * (INITIAL_LEAVES - remainder)


def choose_leaf(
    rng: random.Random,
    leaves: list[int],
    *,
    insertion: bool,
) -> int:
    maximum = CAPACITY + 1 if insertion else CAPACITY
    while True:
        index = rng.randrange(len(leaves))
        weight = leaves[index] + 1 if insertion else leaves[index]
        if rng.random() * maximum < weight:
            return index


def occupancy_band(occupancy: int) -> int:
    if occupancy == MINIMUM:
        return 0
    if occupancy == MINIMUM + 1:
        return 1
    if occupancy <= MINIMUM + 7:
        return 2
    if occupancy <= MINIMUM + 31:
        return 3
    if occupancy <= MINIMUM + 63:
        return 4
    if occupancy < CAPACITY:
        return 5
    return 6


def total_variation(left: list[float], right: list[float]) -> float:
    assert len(left) == len(right)
    return 0.5 * sum(abs(a - b) for a, b in zip(left, right))


def joint_total_variation(
    joint_counts: list[list[int]],
    total: int,
    left_reference: list[float],
    right_reference: list[float],
) -> float:
    if total == 0:
        return 0.0
    difference = 0.0
    for left_index, row in enumerate(joint_counts):
        for right_index, count in enumerate(row):
            observed = count / total
            expected = left_reference[left_index] * right_reference[right_index]
            difference += abs(observed - expected)
    return 0.5 * difference


def joint_mutual_information_bits(
    joint_counts: list[list[int]],
    total: int,
) -> float:
    if total == 0:
        return 0.0
    left = [sum(row) / total for row in joint_counts]
    right = [
        sum(joint_counts[left_index][right_index] for left_index in range(len(joint_counts)))
        / total
        for right_index in range(len(joint_counts[0]))
    ]
    information = 0.0
    for left_index, row in enumerate(joint_counts):
        for right_index, count in enumerate(row):
            if count == 0:
                continue
            observed = count / total
            independent = left[left_index] * right[right_index]
            information += observed * math.log2(observed / independent)
    return information


def iid_outcome_prediction(
    occupancy_probabilities: list[float],
    policy: str,
) -> tuple[float, float, float]:
    minimum_probability = occupancy_probabilities[0]
    merge = minimum_probability * minimum_probability

    if policy == "half-left-first":
        left = 1.0 - minimum_probability
        right = minimum_probability * (1.0 - minimum_probability)
        return left, right, merge

    if policy == "half-fullest-borrow":
        cumulative = minimum_probability
        left = 0.0
        right = 0.0
        for donor_probability in occupancy_probabilities[1:]:
            cumulative_before = cumulative
            cumulative += donor_probability
            left += donor_probability * cumulative
            right += donor_probability * cumulative_before
        return left, right, merge

    raise ValueError(f"unknown policy: {policy}")


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


def run_trial(policy: str, seed: int, cycles: int, burn_in: int) -> Trial:
    rng = random.Random(seed)
    leaves = make_initial_leaves()
    initial_keys = sum(leaves)

    occupancy_counts = [0] * (CAPACITY - MINIMUM + 1)
    occupancy_observations = 0
    fill_sum = 0.0
    fill_observations = 0

    adjacent_joint = [[0] * len(BAND_NAMES) for _ in BAND_NAMES]
    adjacent_observations = 0
    underflow_joint = [[0] * len(BAND_NAMES) for _ in BAND_NAMES]
    interior_underflows = 0
    boundary_underflows = 0
    left_borrows = 0
    right_borrows = 0
    merges = 0

    def insert_one() -> None:
        index = choose_leaf(rng, leaves, insertion=True)
        if leaves[index] < CAPACITY:
            leaves[index] += 1
        else:
            overflow = CAPACITY + 1
            left = math.ceil(overflow / 2)
            right = math.floor(overflow / 2)
            leaves[index : index + 1] = [left, right]

    def delete_one(*, observe: bool) -> None:
        nonlocal interior_underflows, boundary_underflows
        nonlocal left_borrows, right_borrows, merges

        index = choose_leaf(rng, leaves, insertion=False)
        leaves[index] -= 1
        if leaves[index] >= MINIMUM or len(leaves) == 1:
            return

        interior = 0 < index < len(leaves) - 1
        if observe:
            if interior:
                underflow_joint[occupancy_band(leaves[index - 1])][
                    occupancy_band(leaves[index + 1])
                ] += 1
                interior_underflows += 1
            else:
                boundary_underflows += 1

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
            for count in leaves:
                occupancy_counts[count - MINIMUM] += 1
                occupancy_observations += 1
            for left_count, right_count in zip(leaves, leaves[1:]):
                adjacent_joint[occupancy_band(left_count)][occupancy_band(right_count)] += 1
                adjacent_observations += 1

    occupancy_probabilities = [count / occupancy_observations for count in occupancy_counts]
    global_band_probabilities = [0.0] * len(BAND_NAMES)
    for offset, probability in enumerate(occupancy_probabilities):
        global_band_probabilities[occupancy_band(MINIMUM + offset)] += probability

    adjacent_left = [sum(row) / adjacent_observations for row in adjacent_joint]
    adjacent_right = [
        sum(adjacent_joint[left_index][right_index] for left_index in range(len(BAND_NAMES)))
        / adjacent_observations
        for right_index in range(len(BAND_NAMES))
    ]

    underflow_left = [sum(row) / interior_underflows for row in underflow_joint]
    underflow_right = [
        sum(underflow_joint[left_index][right_index] for left_index in range(len(BAND_NAMES)))
        / interior_underflows
        for right_index in range(len(BAND_NAMES))
    ]

    predicted_left, predicted_right, predicted_merge = iid_outcome_prediction(
        occupancy_probabilities,
        policy,
    )
    actual_left = left_borrows / interior_underflows
    actual_right = right_borrows / interior_underflows
    actual_merge = merges / interior_underflows

    predicted_outcomes = [predicted_left, predicted_right, predicted_merge]
    actual_outcomes = [actual_left, actual_right, actual_merge]

    return Trial(
        policy=policy,
        seed=seed,
        cycles=cycles,
        burn_in_cycles=burn_in,
        mean_fill=fill_sum / fill_observations,
        global_minimum_fraction=occupancy_probabilities[0],
        adjacent_band_tv_from_iid=joint_total_variation(
            adjacent_joint,
            adjacent_observations,
            adjacent_left,
            adjacent_right,
        ),
        adjacent_band_mutual_information_bits=joint_mutual_information_bits(
            adjacent_joint,
            adjacent_observations,
        ),
        underflow_band_tv_from_global_iid=joint_total_variation(
            underflow_joint,
            interior_underflows,
            global_band_probabilities,
            global_band_probabilities,
        ),
        underflow_neighbor_mutual_information_bits=joint_mutual_information_bits(
            underflow_joint,
            interior_underflows,
        ),
        underflow_left_neighbor_tv_from_global=total_variation(
            underflow_left,
            global_band_probabilities,
        ),
        underflow_right_neighbor_tv_from_global=total_variation(
            underflow_right,
            global_band_probabilities,
        ),
        interior_underflow_events=interior_underflows,
        boundary_underflow_events=boundary_underflows,
        predicted_left_borrow_share_per_underflow=predicted_left,
        predicted_right_borrow_share_per_underflow=predicted_right,
        predicted_merge_share_per_underflow=predicted_merge,
        actual_left_borrow_share_per_underflow=actual_left,
        actual_right_borrow_share_per_underflow=actual_right,
        actual_merge_share_per_underflow=actual_merge,
        outcome_tv_from_iid=total_variation(predicted_outcomes, actual_outcomes),
        merge_multiplier_over_iid=actual_merge / predicted_merge,
    )


def aggregate(trials: list[Trial]) -> Aggregate:
    first = trials[0]
    assert all(trial.policy == first.policy for trial in trials)

    def mean(field: str) -> float:
        return statistics.fmean(getattr(trial, field) for trial in trials)

    return Aggregate(
        policy=first.policy,
        seeds=tuple(trial.seed for trial in trials),
        cycles_per_seed=first.cycles,
        burn_in_cycles=first.burn_in_cycles,
        mean_fill=mean("mean_fill"),
        global_minimum_fraction=mean("global_minimum_fraction"),
        adjacent_band_tv_from_iid=mean("adjacent_band_tv_from_iid"),
        adjacent_band_mutual_information_bits=mean("adjacent_band_mutual_information_bits"),
        underflow_band_tv_from_global_iid=mean("underflow_band_tv_from_global_iid"),
        underflow_neighbor_mutual_information_bits=mean(
            "underflow_neighbor_mutual_information_bits"
        ),
        underflow_left_neighbor_tv_from_global=mean(
            "underflow_left_neighbor_tv_from_global"
        ),
        underflow_right_neighbor_tv_from_global=mean(
            "underflow_right_neighbor_tv_from_global"
        ),
        mean_interior_underflow_events=mean("interior_underflow_events"),
        mean_boundary_underflow_events=mean("boundary_underflow_events"),
        predicted_left_borrow_share_per_underflow=mean(
            "predicted_left_borrow_share_per_underflow"
        ),
        predicted_right_borrow_share_per_underflow=mean(
            "predicted_right_borrow_share_per_underflow"
        ),
        predicted_merge_share_per_underflow=mean(
            "predicted_merge_share_per_underflow"
        ),
        actual_left_borrow_share_per_underflow=mean(
            "actual_left_borrow_share_per_underflow"
        ),
        actual_right_borrow_share_per_underflow=mean(
            "actual_right_borrow_share_per_underflow"
        ),
        actual_merge_share_per_underflow=mean("actual_merge_share_per_underflow"),
        outcome_tv_from_iid=mean("outcome_tv_from_iid"),
        merge_multiplier_over_iid=mean("merge_multiplier_over_iid"),
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
    by_policy = {result.policy: result for result in aggregates}
    left = by_policy["half-left-first"]
    fuller = by_policy["half-fullest-borrow"]

    # Ordinary adjacency is only modestly non-iid, but conditioning on an
    # underflowing target moves the neighbor-pair distribution much farther from
    # the global iid product. This is the state reduction Experiment 0116 tests.
    assert left.adjacent_band_tv_from_iid < 0.15
    assert fuller.adjacent_band_tv_from_iid < 0.15
    assert left.underflow_band_tv_from_global_iid > 0.12
    assert fuller.underflow_band_tv_from_global_iid > 0.12
    assert left.underflow_band_tv_from_global_iid > left.adjacent_band_tv_from_iid
    assert fuller.underflow_band_tv_from_global_iid > fuller.adjacent_band_tv_from_iid

    # iid materially underpredicts merges near the repair frontier.
    assert left.merge_multiplier_over_iid > 2.0
    assert fuller.merge_multiplier_over_iid > 2.0

    # Sibling choice remains visible after conditioning on the same frontier state.
    assert left.actual_left_borrow_share_per_underflow > 0.75
    assert fuller.actual_left_borrow_share_per_underflow < 0.65

    if not quick:
        assert left.underflow_band_tv_from_global_iid > 0.18
        assert fuller.underflow_band_tv_from_global_iid > 0.15


def print_csv(aggregates: list[Aggregate]) -> None:
    print(
        "policy,mean_fill,global_minimum_fraction,adjacent_band_tv_from_iid,"
        "adjacent_band_mutual_information_bits,underflow_band_tv_from_global_iid,"
        "underflow_neighbor_mutual_information_bits,"
        "underflow_left_neighbor_tv_from_global,underflow_right_neighbor_tv_from_global,"
        "predicted_left,actual_left,predicted_right,actual_right,predicted_merge,"
        "actual_merge,outcome_tv_from_iid,merge_multiplier_over_iid"
    )
    for result in aggregates:
        print(
            f"{result.policy},{result.mean_fill:.9f},{result.global_minimum_fraction:.9f},"
            f"{result.adjacent_band_tv_from_iid:.9f},"
            f"{result.adjacent_band_mutual_information_bits:.9f},"
            f"{result.underflow_band_tv_from_global_iid:.9f},"
            f"{result.underflow_neighbor_mutual_information_bits:.9f},"
            f"{result.underflow_left_neighbor_tv_from_global:.9f},"
            f"{result.underflow_right_neighbor_tv_from_global:.9f},"
            f"{result.predicted_left_borrow_share_per_underflow:.9f},"
            f"{result.actual_left_borrow_share_per_underflow:.9f},"
            f"{result.predicted_right_borrow_share_per_underflow:.9f},"
            f"{result.actual_right_borrow_share_per_underflow:.9f},"
            f"{result.predicted_merge_share_per_underflow:.9f},"
            f"{result.actual_merge_share_per_underflow:.9f},"
            f"{result.outcome_tv_from_iid:.9f},"
            f"{result.merge_multiplier_over_iid:.9f}"
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
                        "band_names": BAND_NAMES,
                        "sample_every_cycles": SAMPLE_EVERY,
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
