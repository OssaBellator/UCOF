#!/usr/bin/env python3
"""Diagnose where an iid-neighbor occupancy model misses EXP-0003 behavior.

This is intentionally *not* the final fringe analysis. It solves a deterministic
mean-field ODE for the leaf occupancy distribution under one insertion + one
deletion cycle, then compares its predictions with Experiment 0110's long fixed-
seed Monte Carlo evidence.

The approximation assumes sibling occupancies are independent draws from the
current leaf occupancy distribution. Splits, borrows, and merges create sibling
correlations in the real tree, so the size and direction of the prediction error
is itself the result: it tells us whether a one-dimensional occupancy distribution
is rich enough for FCP-0003 policy review.
"""

from __future__ import annotations

import argparse
import json
import math
from dataclasses import asdict, dataclass

CAPACITY = 254
MINIMUM = math.ceil(CAPACITY / 2)
SPLIT_LEFT = math.ceil((CAPACITY + 1) / 2)
SPLIT_RIGHT = math.floor((CAPACITY + 1) / 2)
MERGED_OCCUPANCY = 2 * MINIMUM - 1

DT = 0.25
TOLERANCE = 1e-9
MAX_ITERATIONS = 150_000
INITIAL_OCCUPANCY = 178

EXPERIMENT_0110 = {
    "half-left-first": {
        "mean_fill": 0.62310,
        "restructure_rate_per_op": 0.0148988,
        "left_borrow_share": 0.885,
    },
    "half-fullest-borrow": {
        "mean_fill": 0.61851,
        "restructure_rate_per_op": 0.0125239,
        "left_borrow_share": 0.522,
    },
}


@dataclass(frozen=True)
class MeanFieldResult:
    policy: str
    iterations: int
    residual: float
    mean_occupancy: float
    mean_fill: float
    split_rate_per_cycle: float
    borrow_rate_per_cycle: float
    merge_rate_per_cycle: float
    restructure_rate_per_op: float
    left_borrow_rate_per_cycle: float
    right_borrow_rate_per_cycle: float
    left_borrow_share: float
    page_count_drift_per_cycle: float


@dataclass(frozen=True)
class GapResult:
    policy: str
    modeled_fill: float
    experiment_fill: float
    fill_error_percentage_points: float
    modeled_restructure_rate_per_op: float
    experiment_restructure_rate_per_op: float
    restructure_relative_error: float
    modeled_left_borrow_share: float
    experiment_left_borrow_share: float
    left_borrow_share_error_percentage_points: float


def state_index(occupancy: int) -> int:
    return occupancy - MINIMUM


def drift(probabilities: list[float], policy: str) -> tuple[list[float], dict[str, float]]:
    occupancies = range(MINIMUM, CAPACITY + 1)
    mean = sum(k * p for k, p in zip(occupancies, probabilities))
    delta = [0.0] * len(probabilities)

    insertion_denominator = mean + 1.0
    split_rate = 0.0
    for occupancy, probability in zip(occupancies, probabilities):
        event = (occupancy + 1) * probability / insertion_denominator
        index = state_index(occupancy)
        if occupancy < CAPACITY:
            delta[index] -= event
            delta[index + 1] += event
        else:
            delta[index] -= event
            delta[state_index(SPLIT_LEFT)] += event
            delta[state_index(SPLIT_RIGHT)] += event
            split_rate += event

    deletion_denominator = mean
    minimum_probability = probabilities[0]
    merge_rate = 0.0
    left_borrow_rate = 0.0
    right_borrow_rate = 0.0

    for occupancy, probability in zip(occupancies, probabilities):
        event = occupancy * probability / deletion_denominator
        index = state_index(occupancy)
        if occupancy > MINIMUM:
            delta[index] -= event
            delta[index - 1] += event
            continue

        underflow = event
        merge_conditional = minimum_probability * minimum_probability
        merge_rate += underflow * merge_conditional
        delta[state_index(MINIMUM)] -= 2.0 * underflow * merge_conditional
        delta[state_index(MERGED_OCCUPANCY)] += underflow * merge_conditional

        if policy == "half-left-first":
            for donor in range(MINIMUM + 1, CAPACITY + 1):
                donor_probability = probabilities[state_index(donor)]
                selected_mass = (1.0 + minimum_probability) * donor_probability
                delta[state_index(donor)] -= underflow * selected_mass
                delta[state_index(donor - 1)] += underflow * selected_mass

            left_borrow_rate += underflow * (1.0 - minimum_probability)
            right_borrow_rate += (
                underflow * minimum_probability * (1.0 - minimum_probability)
            )

        elif policy == "half-fullest-borrow":
            cumulative = minimum_probability
            left_conditional = 0.0
            right_conditional = 0.0
            for donor in range(MINIMUM + 1, CAPACITY + 1):
                donor_probability = probabilities[state_index(donor)]
                cumulative_before = cumulative
                cumulative += donor_probability
                selected_mass = donor_probability * (cumulative + cumulative_before)
                delta[state_index(donor)] -= underflow * selected_mass
                delta[state_index(donor - 1)] += underflow * selected_mass
                left_conditional += donor_probability * cumulative
                right_conditional += donor_probability * cumulative_before

            left_borrow_rate += underflow * left_conditional
            right_borrow_rate += underflow * right_conditional
        else:
            raise ValueError(f"unknown policy: {policy}")

    borrow_rate = left_borrow_rate + right_borrow_rate
    page_count_drift = split_rate - merge_rate
    probability_drift = [
        count_delta - probability * page_count_drift
        for count_delta, probability in zip(delta, probabilities)
    ]

    return probability_drift, {
        "mean": mean,
        "split": split_rate,
        "borrow": borrow_rate,
        "merge": merge_rate,
        "left_borrow": left_borrow_rate,
        "right_borrow": right_borrow_rate,
        "page_count_drift": page_count_drift,
    }


def solve(policy: str) -> MeanFieldResult:
    probabilities = [0.0] * (CAPACITY - MINIMUM + 1)
    probabilities[state_index(INITIAL_OCCUPANCY)] = 1.0

    metrics: dict[str, float] = {}
    residual = math.inf
    for iteration in range(MAX_ITERATIONS):
        derivative, metrics = drift(probabilities, policy)
        residual = max(abs(value) for value in derivative)
        if residual < TOLERANCE:
            break

        updated = [
            probability + DT * derivative_value
            for probability, derivative_value in zip(probabilities, derivative)
        ]
        if min(updated) < -1e-12:
            raise RuntimeError("mean-field Euler step became unstable")
        updated = [max(0.0, value) for value in updated]
        total = sum(updated)
        probabilities = [value / total for value in updated]
    else:
        raise RuntimeError("mean-field solver did not converge")

    borrow = metrics["borrow"]
    restructure_per_operation = (
        metrics["split"] + metrics["borrow"] + metrics["merge"]
    ) / 2.0

    return MeanFieldResult(
        policy=policy,
        iterations=iteration,
        residual=residual,
        mean_occupancy=metrics["mean"],
        mean_fill=metrics["mean"] / CAPACITY,
        split_rate_per_cycle=metrics["split"],
        borrow_rate_per_cycle=metrics["borrow"],
        merge_rate_per_cycle=metrics["merge"],
        restructure_rate_per_op=restructure_per_operation,
        left_borrow_rate_per_cycle=metrics["left_borrow"],
        right_borrow_rate_per_cycle=metrics["right_borrow"],
        left_borrow_share=metrics["left_borrow"] / borrow,
        page_count_drift_per_cycle=metrics["page_count_drift"],
    )


def gap(result: MeanFieldResult) -> GapResult:
    evidence = EXPERIMENT_0110[result.policy]
    return GapResult(
        policy=result.policy,
        modeled_fill=result.mean_fill,
        experiment_fill=evidence["mean_fill"],
        fill_error_percentage_points=100.0 * (result.mean_fill - evidence["mean_fill"]),
        modeled_restructure_rate_per_op=result.restructure_rate_per_op,
        experiment_restructure_rate_per_op=evidence["restructure_rate_per_op"],
        restructure_relative_error=(
            result.restructure_rate_per_op / evidence["restructure_rate_per_op"] - 1.0
        ),
        modeled_left_borrow_share=result.left_borrow_share,
        experiment_left_borrow_share=evidence["left_borrow_share"],
        left_borrow_share_error_percentage_points=100.0
        * (result.left_borrow_share - evidence["left_borrow_share"]),
    )


def self_check(results: list[MeanFieldResult], gaps: list[GapResult]) -> None:
    by_policy = {item.policy: item for item in results}
    left = by_policy["half-left-first"]
    fuller = by_policy["half-fullest-borrow"]

    assert 0.573 < left.mean_fill < 0.575
    assert 0.573 < fuller.mean_fill < 0.575
    assert 0.023 < left.restructure_rate_per_op < 0.025
    assert 0.017 < fuller.restructure_rate_per_op < 0.0185
    assert left.left_borrow_share > 0.90
    assert 0.48 < fuller.left_borrow_share < 0.54
    assert abs(left.page_count_drift_per_cycle) < 1e-7
    assert abs(fuller.page_count_drift_per_cycle) < 1e-7

    gap_by_policy = {item.policy: item for item in gaps}
    assert gap_by_policy["half-left-first"].fill_error_percentage_points < -3.0
    assert gap_by_policy["half-fullest-borrow"].fill_error_percentage_points < -3.0
    assert gap_by_policy["half-left-first"].restructure_relative_error > 0.30
    assert gap_by_policy["half-fullest-borrow"].restructure_relative_error > 0.20


def print_csv(results: list[MeanFieldResult], gaps: list[GapResult]) -> None:
    print(
        "policy,iterations,residual,mean_occupancy,mean_fill,split_per_cycle,"
        "borrow_per_cycle,merge_per_cycle,restructure_per_op,left_borrow_share,"
        "page_count_drift_per_cycle"
    )
    for item in results:
        print(
            f"{item.policy},{item.iterations},{item.residual:.12g},"
            f"{item.mean_occupancy:.9f},{item.mean_fill:.9f},"
            f"{item.split_rate_per_cycle:.12g},{item.borrow_rate_per_cycle:.12g},"
            f"{item.merge_rate_per_cycle:.12g},{item.restructure_rate_per_op:.12g},"
            f"{item.left_borrow_share:.9f},{item.page_count_drift_per_cycle:.12g}"
        )

    print("# gap_against_experiment_0110")
    print(
        "policy,modeled_fill,experiment_fill,fill_error_percentage_points,"
        "modeled_restructure_per_op,experiment_restructure_per_op,"
        "restructure_relative_error,left_borrow_share_error_percentage_points"
    )
    for item in gaps:
        print(
            f"{item.policy},{item.modeled_fill:.9f},{item.experiment_fill:.9f},"
            f"{item.fill_error_percentage_points:.6f},"
            f"{item.modeled_restructure_rate_per_op:.12g},"
            f"{item.experiment_restructure_rate_per_op:.12g},"
            f"{item.restructure_relative_error:.9f},"
            f"{item.left_borrow_share_error_percentage_points:.6f}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="emit JSON")
    args = parser.parse_args()

    results = [solve("half-left-first"), solve("half-fullest-borrow")]
    gaps = [gap(item) for item in results]
    self_check(results, gaps)

    if args.json:
        print(
            json.dumps(
                {
                    "configuration": {
                        "capacity": CAPACITY,
                        "minimum": MINIMUM,
                        "dt": DT,
                        "tolerance": TOLERANCE,
                        "initial_occupancy": INITIAL_OCCUPANCY,
                        "assumption": "iid sibling occupancies from marginal leaf distribution",
                    },
                    "mean_field": [asdict(item) for item in results],
                    "gap_against_experiment_0110": [asdict(item) for item in gaps],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_csv(results, gaps)


if __name__ == "__main__":
    main()
