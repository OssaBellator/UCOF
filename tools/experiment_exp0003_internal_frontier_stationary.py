#!/usr/bin/env python3
"""Weight EXP-0003 internal deletion frontiers in structural event time.

Experiment 0131 enumerates the exact local internal-sibling geometry after a
non-root internal node falls from M to M-1, but deliberately gives every local
state equal weight. This experiment adds a reduced stationary workload model for
the *parent child-count process*.

One structural cycle contains exactly one child insertion (caused by a lower
split) and one child removal (caused by a lower merge), in randomized order. The
live number of child references is therefore fixed, while internal nodes split,
borrow, merge, and change occupancy under the candidate deletion policies.

Two closely related arrival kernels are run as a sensitivity check:

* child-proportional: both structural insertions and removals choose an internal
  node in proportion to its current child count;
* gap-insert: removals remain child-proportional while structural insertions use
  occupancy + 1, matching the gap weighting used by the leaf-level workload
  experiments.

The model reports frequencies per *structural child-removal event*. As a clearly
separate two-timescale estimate, it also multiplies those frequencies by the
leaf-merge rate measured by Experiment 0126 to estimate rates per ordinary
object operation. That multiplication is a closure assumption: it does not model
cross-level correlation between a leaf merge and its parent's occupancy state.

This is non-normative research evidence. It is not a proof of stationarity, a
Markov-lumpability result, a real-writer frequency measurement, or an EXP-0003
policy/epoch decision.
"""

from __future__ import annotations

import argparse
import json
import math
import random
import statistics
from collections import Counter
from dataclasses import asdict, dataclass

import experiment_exp0003_minimum_frontier_drift as leaf_drift


INTERNAL_CAPACITY = 255
INTERNAL_MINIMUM = 128
INITIAL_INTERNAL_NODES = 32
INITIAL_FILL = 0.70

QUICK_SEEDS = (3, 17, 29)
QUICK_CYCLES = 200_000
QUICK_BURN_IN = 50_000
FULL_SEEDS = (3, 17, 29, 43, 71)
FULL_CYCLES = 800_000
FULL_BURN_IN = 200_000

POLICIES = ("left-first", "fuller-sibling")
KERNELS = ("child-proportional", "gap-insert")

# Experiment 0131 exact unweighted local geometry under M=128, C=255.
UNWEIGHTED_TWO_DONOR_STATES = 16_129
UNWEIGHTED_AVOIDABLE_LEFT_CLIFF_STATES = 126
UNWEIGHTED_AVOIDABLE_CLIFF_SHARE = (
    UNWEIGHTED_AVOIDABLE_LEFT_CLIFF_STATES / UNWEIGHTED_TWO_DONOR_STATES
)


@dataclass(frozen=True)
class Trial:
    kernel: str
    policy: str
    seed: int
    cycles: int
    burn_in_cycles: int
    observed_structural_deletions: int
    underflows: int
    borrows: int
    merges: int
    splits: int
    one_donor_underflows: int
    two_donor_underflows: int
    zero_donor_underflows: int
    policy_divergence_opportunities: int
    avoidable_left_cliff_opportunities: int
    selected_donor_cliffs: int
    selected_avoidable_left_cliffs: int
    mean_minimum_internal_nodes_before_delete: float
    mean_minimum_plus_one_internal_nodes_before_delete: float
    mean_internal_node_count_before_delete: float


@dataclass(frozen=True)
class Aggregate:
    kernel: str
    policy: str
    seeds: tuple[int, ...]
    cycles_per_seed: int
    burn_in_cycles: int
    observed_structural_deletions: int
    underflows: int
    borrows: int
    merges: int
    splits: int
    one_donor_underflows: int
    two_donor_underflows: int
    zero_donor_underflows: int
    policy_divergence_opportunities: int
    avoidable_left_cliff_opportunities: int
    selected_donor_cliffs: int
    selected_avoidable_left_cliffs: int
    mean_minimum_internal_nodes_before_delete: float
    mean_minimum_plus_one_internal_nodes_before_delete: float
    mean_internal_node_count_before_delete: float
    underflow_rate_per_structural_deletion: float
    two_donor_share_of_underflows: float
    policy_divergence_share_of_two_donor: float
    avoidable_left_cliff_share_of_two_donor: float
    avoidable_cliff_enrichment_over_unweighted: float
    selected_donor_cliff_share_of_borrows: float
    leaf_merge_rate_per_object_operation: float
    estimated_internal_underflow_rate_per_object_operation: float
    estimated_avoidable_left_cliff_rate_per_object_operation: float
    estimated_selected_donor_cliff_rate_per_object_operation: float


def make_initial_nodes() -> list[int]:
    total = round(INTERNAL_CAPACITY * INITIAL_INTERNAL_NODES * INITIAL_FILL)
    base, remainder = divmod(total, INITIAL_INTERNAL_NODES)
    nodes = [base + 1] * remainder + [base] * (INITIAL_INTERNAL_NODES - remainder)
    assert sum(nodes) == total
    assert all(INTERNAL_MINIMUM <= occupancy <= INTERNAL_CAPACITY for occupancy in nodes)
    return nodes


def choose_node(
    rng: random.Random,
    nodes: list[int],
    *,
    insertion: bool,
    kernel: str,
) -> int:
    if kernel not in KERNELS:
        raise ValueError(f"unknown kernel: {kernel}")

    gap_weighted = insertion and kernel == "gap-insert"
    maximum = INTERNAL_CAPACITY + 1 if gap_weighted else INTERNAL_CAPACITY
    while True:
        index = rng.randrange(len(nodes))
        weight = nodes[index] + 1 if gap_weighted else nodes[index]
        if rng.random() * maximum < weight:
            return index


def eligible_siblings(nodes: list[int], index: int) -> tuple[int | None, int | None]:
    left = (
        nodes[index - 1]
        if index > 0 and nodes[index - 1] > INTERNAL_MINIMUM
        else None
    )
    right = (
        nodes[index + 1]
        if index + 1 < len(nodes) and nodes[index + 1] > INTERNAL_MINIMUM
        else None
    )
    return left, right


def choose_borrow_side(left: int | None, right: int | None, policy: str) -> str | None:
    if policy == "left-first":
        if left is not None:
            return "left"
        if right is not None:
            return "right"
        return None

    if policy == "fuller-sibling":
        if left is not None and right is not None:
            return "left" if left >= right else "right"
        if left is not None:
            return "left"
        if right is not None:
            return "right"
        return None

    raise ValueError(f"unknown policy: {policy}")


def run_trial(
    *,
    kernel: str,
    policy: str,
    seed: int,
    cycles: int,
    burn_in: int,
) -> Trial:
    if policy not in POLICIES:
        raise ValueError(f"unknown policy: {policy}")
    if kernel not in KERNELS:
        raise ValueError(f"unknown kernel: {kernel}")
    if not 0 <= burn_in < cycles:
        raise ValueError("burn-in must be in [0, cycles)")

    rng = random.Random(seed)
    nodes = make_initial_nodes()
    initial_node_count = len(nodes)
    total_children = sum(nodes)
    observed = Counter()
    minimum_sum = 0
    minimum_plus_one_sum = 0
    node_count_sum = 0
    observations = 0

    def insert_child(*, collect: bool) -> None:
        nonlocal nodes
        index = choose_node(rng, nodes, insertion=True, kernel=kernel)
        if nodes[index] < INTERNAL_CAPACITY:
            nodes[index] += 1
            return

        # A full internal node has C=255 children. Adding one child yields 256,
        # which lower-median balanced splitting represents as 128 + 128.
        nodes[index : index + 1] = [INTERNAL_MINIMUM, INTERNAL_MINIMUM]
        if collect:
            observed["splits"] += 1

    def remove_child(*, collect: bool) -> None:
        nonlocal nodes, minimum_sum, minimum_plus_one_sum, node_count_sum

        if collect:
            minimum_sum += sum(occupancy == INTERNAL_MINIMUM for occupancy in nodes)
            minimum_plus_one_sum += sum(
                occupancy == INTERNAL_MINIMUM + 1 for occupancy in nodes
            )
            node_count_sum += len(nodes)

        index = choose_node(rng, nodes, insertion=False, kernel=kernel)
        old_occupancy = nodes[index]
        nodes[index] -= 1
        if nodes[index] >= INTERNAL_MINIMUM:
            return

        assert old_occupancy == INTERNAL_MINIMUM
        if collect:
            observed["underflows"] += 1

        left, right = eligible_siblings(nodes, index)
        eligible = int(left is not None) + int(right is not None)
        if collect:
            observed[f"eligible_{eligible}"] += 1
            if eligible == 2:
                assert left is not None and right is not None
                if right > left:
                    observed["policy_divergence_opportunities"] += 1
                if left == INTERNAL_MINIMUM + 1 and right > left:
                    observed["avoidable_left_cliff_opportunities"] += 1

        side = choose_borrow_side(left, right, policy)
        if side is not None:
            donor = left if side == "left" else right
            other = right if side == "left" else left
            assert donor is not None and donor > INTERNAL_MINIMUM
            if collect:
                observed["borrows"] += 1
                if donor == INTERNAL_MINIMUM + 1:
                    observed["selected_donor_cliffs"] += 1
                    if other is not None and other > donor:
                        observed["selected_avoidable_left_cliffs"] += 1
            if side == "left":
                nodes[index - 1] -= 1
                nodes[index] += 1
            else:
                nodes[index + 1] -= 1
                nodes[index] += 1
            return

        if collect:
            observed["merges"] += 1
        if index > 0:
            assert nodes[index - 1] == INTERNAL_MINIMUM
            nodes[index - 1] += nodes[index]
            nodes.pop(index)
        else:
            assert nodes[index + 1] == INTERNAL_MINIMUM
            nodes[index] += nodes[index + 1]
            nodes.pop(index + 1)

    for cycle in range(cycles):
        collect = cycle >= burn_in
        if rng.random() < 0.5:
            insert_child(collect=collect)
            remove_child(collect=collect)
        else:
            remove_child(collect=collect)
            insert_child(collect=collect)

        assert sum(nodes) == total_children
        assert all(
            INTERNAL_MINIMUM <= occupancy <= INTERNAL_CAPACITY for occupancy in nodes
        )
        if collect:
            observations += 1

    # Each split adds one internal node and each merge removes one. Include the
    # unobserved burn-in delta by comparing only the final state to total child
    # conservation above; observed split/merge counts are not required to equal
    # the final node-count delta.
    assert len(nodes) >= 2
    assert initial_node_count >= 2

    return Trial(
        kernel=kernel,
        policy=policy,
        seed=seed,
        cycles=cycles,
        burn_in_cycles=burn_in,
        observed_structural_deletions=observations,
        underflows=observed["underflows"],
        borrows=observed["borrows"],
        merges=observed["merges"],
        splits=observed["splits"],
        one_donor_underflows=observed["eligible_1"],
        two_donor_underflows=observed["eligible_2"],
        zero_donor_underflows=observed["eligible_0"],
        policy_divergence_opportunities=observed["policy_divergence_opportunities"],
        avoidable_left_cliff_opportunities=observed[
            "avoidable_left_cliff_opportunities"
        ],
        selected_donor_cliffs=observed["selected_donor_cliffs"],
        selected_avoidable_left_cliffs=observed[
            "selected_avoidable_left_cliffs"
        ],
        mean_minimum_internal_nodes_before_delete=minimum_sum / observations,
        mean_minimum_plus_one_internal_nodes_before_delete=(
            minimum_plus_one_sum / observations
        ),
        mean_internal_node_count_before_delete=node_count_sum / observations,
    )


def aggregate_trials(trials: list[Trial], *, leaf_merge_rate: float) -> Aggregate:
    if not trials:
        raise ValueError("cannot aggregate zero trials")
    first = trials[0]
    assert all(trial.kernel == first.kernel for trial in trials)
    assert all(trial.policy == first.policy for trial in trials)
    assert all(trial.cycles == first.cycles for trial in trials)
    assert all(trial.burn_in_cycles == first.burn_in_cycles for trial in trials)

    def total(field: str) -> int:
        return sum(getattr(trial, field) for trial in trials)

    def mean(field: str) -> float:
        return statistics.fmean(getattr(trial, field) for trial in trials)

    structural_deletions = total("observed_structural_deletions")
    underflows = total("underflows")
    borrows = total("borrows")
    two_donor = total("two_donor_underflows")
    divergence = total("policy_divergence_opportunities")
    avoidable = total("avoidable_left_cliff_opportunities")
    selected_cliffs = total("selected_donor_cliffs")

    underflow_rate = underflows / structural_deletions
    two_donor_share = two_donor / underflows
    divergence_share = divergence / two_donor
    avoidable_share = avoidable / two_donor
    selected_cliff_share = selected_cliffs / borrows

    return Aggregate(
        kernel=first.kernel,
        policy=first.policy,
        seeds=tuple(trial.seed for trial in trials),
        cycles_per_seed=first.cycles,
        burn_in_cycles=first.burn_in_cycles,
        observed_structural_deletions=structural_deletions,
        underflows=underflows,
        borrows=borrows,
        merges=total("merges"),
        splits=total("splits"),
        one_donor_underflows=total("one_donor_underflows"),
        two_donor_underflows=two_donor,
        zero_donor_underflows=total("zero_donor_underflows"),
        policy_divergence_opportunities=divergence,
        avoidable_left_cliff_opportunities=avoidable,
        selected_donor_cliffs=selected_cliffs,
        selected_avoidable_left_cliffs=total("selected_avoidable_left_cliffs"),
        mean_minimum_internal_nodes_before_delete=mean(
            "mean_minimum_internal_nodes_before_delete"
        ),
        mean_minimum_plus_one_internal_nodes_before_delete=mean(
            "mean_minimum_plus_one_internal_nodes_before_delete"
        ),
        mean_internal_node_count_before_delete=mean(
            "mean_internal_node_count_before_delete"
        ),
        underflow_rate_per_structural_deletion=underflow_rate,
        two_donor_share_of_underflows=two_donor_share,
        policy_divergence_share_of_two_donor=divergence_share,
        avoidable_left_cliff_share_of_two_donor=avoidable_share,
        avoidable_cliff_enrichment_over_unweighted=(
            avoidable_share / UNWEIGHTED_AVOIDABLE_CLIFF_SHARE
        ),
        selected_donor_cliff_share_of_borrows=selected_cliff_share,
        leaf_merge_rate_per_object_operation=leaf_merge_rate,
        estimated_internal_underflow_rate_per_object_operation=(
            leaf_merge_rate * underflow_rate
        ),
        estimated_avoidable_left_cliff_rate_per_object_operation=(
            leaf_merge_rate * avoidable / structural_deletions
        ),
        estimated_selected_donor_cliff_rate_per_object_operation=(
            leaf_merge_rate * selected_cliffs / structural_deletions
        ),
    )


def leaf_merge_rates(*, quick: bool) -> dict[str, float]:
    _, aggregates = leaf_drift.run_ensemble(quick=quick)
    return {
        item.policy: item.merge.observed / item.observed_operations for item in aggregates
    }


def run(*, quick: bool) -> list[Aggregate]:
    if quick:
        seeds = QUICK_SEEDS
        cycles = QUICK_CYCLES
        burn_in = QUICK_BURN_IN
    else:
        seeds = FULL_SEEDS
        cycles = FULL_CYCLES
        burn_in = FULL_BURN_IN

    merge_rates = leaf_merge_rates(quick=quick)
    results = []
    for kernel in KERNELS:
        for policy in POLICIES:
            trials = [
                run_trial(
                    kernel=kernel,
                    policy=policy,
                    seed=seed,
                    cycles=cycles,
                    burn_in=burn_in,
                )
                for seed in seeds
            ]
            results.append(
                aggregate_trials(trials, leaf_merge_rate=merge_rates[policy])
            )
    return results


def self_check(results: list[Aggregate]) -> None:
    by_key = {(item.kernel, item.policy): item for item in results}
    assert set(by_key) == {
        (kernel, policy) for kernel in KERNELS for policy in POLICIES
    }

    for item in results:
        assert item.underflows == item.borrows + item.merges
        assert item.underflows == (
            item.zero_donor_underflows
            + item.one_donor_underflows
            + item.two_donor_underflows
        )
        assert item.zero_donor_underflows == item.merges
        assert item.policy_divergence_opportunities >= item.avoidable_left_cliff_opportunities
        assert item.avoidable_left_cliff_share_of_two_donor > UNWEIGHTED_AVOIDABLE_CLIFF_SHARE
        assert item.avoidable_cliff_enrichment_over_unweighted > 4.0
        assert item.mean_minimum_internal_nodes_before_delete > 0.0
        assert item.estimated_internal_underflow_rate_per_object_operation > 0.0
        assert abs(item.splits - item.merges) < 32

        if item.policy == "left-first":
            # Every locally avoidable left M+1 cliff is selected by LeftFirst.
            assert (
                item.selected_avoidable_left_cliffs
                == item.avoidable_left_cliff_opportunities
            )
        else:
            # FullerSiblingLeftTie never selects the smaller M+1 donor when a
            # strictly fuller eligible alternative exists.
            assert item.selected_avoidable_left_cliffs == 0

    for kernel in KERNELS:
        left = by_key[(kernel, "left-first")]
        fuller = by_key[(kernel, "fuller-sibling")]
        assert fuller.mean_minimum_internal_nodes_before_delete < (
            left.mean_minimum_internal_nodes_before_delete
        )
        assert fuller.underflow_rate_per_structural_deletion < (
            left.underflow_rate_per_structural_deletion
        )
        assert fuller.selected_donor_cliff_share_of_borrows < (
            left.selected_donor_cliff_share_of_borrows
        )
        assert fuller.estimated_internal_underflow_rate_per_object_operation < (
            left.estimated_internal_underflow_rate_per_object_operation
        )
        assert fuller.leaf_merge_rate_per_object_operation < (
            left.leaf_merge_rate_per_object_operation
        )


def print_csv(results: list[Aggregate]) -> None:
    print(
        "kernel,policy,seeds,structural_deletions,underflows,borrows,merges,splits,"
        "one_donor,two_donor,zero_donor,policy_divergence_opportunities,"
        "avoidable_left_cliff_opportunities,selected_donor_cliffs,"
        "selected_avoidable_left_cliffs,mean_min_internal,mean_m_plus_one_internal,"
        "mean_internal_nodes,underflow_rate_per_structural_delete,"
        "two_donor_share_underflows,policy_divergence_share_two_donor,"
        "avoidable_left_cliff_share_two_donor,unweighted_avoidable_cliff_share,"
        "avoidable_cliff_enrichment,selected_donor_cliff_share_borrows,"
        "leaf_merge_rate_per_object_op,estimated_internal_underflow_rate_per_object_op,"
        "estimated_avoidable_left_cliff_rate_per_object_op,"
        "estimated_selected_donor_cliff_rate_per_object_op"
    )
    for item in results:
        seeds = "+".join(str(seed) for seed in item.seeds)
        print(
            f"{item.kernel},{item.policy},{seeds},{item.observed_structural_deletions},"
            f"{item.underflows},{item.borrows},{item.merges},{item.splits},"
            f"{item.one_donor_underflows},{item.two_donor_underflows},"
            f"{item.zero_donor_underflows},{item.policy_divergence_opportunities},"
            f"{item.avoidable_left_cliff_opportunities},{item.selected_donor_cliffs},"
            f"{item.selected_avoidable_left_cliffs},"
            f"{item.mean_minimum_internal_nodes_before_delete:.9f},"
            f"{item.mean_minimum_plus_one_internal_nodes_before_delete:.9f},"
            f"{item.mean_internal_node_count_before_delete:.9f},"
            f"{item.underflow_rate_per_structural_deletion:.12g},"
            f"{item.two_donor_share_of_underflows:.12g},"
            f"{item.policy_divergence_share_of_two_donor:.12g},"
            f"{item.avoidable_left_cliff_share_of_two_donor:.12g},"
            f"{UNWEIGHTED_AVOIDABLE_CLIFF_SHARE:.12g},"
            f"{item.avoidable_cliff_enrichment_over_unweighted:.12g},"
            f"{item.selected_donor_cliff_share_of_borrows:.12g},"
            f"{item.leaf_merge_rate_per_object_operation:.12g},"
            f"{item.estimated_internal_underflow_rate_per_object_operation:.12g},"
            f"{item.estimated_avoidable_left_cliff_rate_per_object_operation:.12g},"
            f"{item.estimated_selected_donor_cliff_rate_per_object_operation:.12g}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--quick", action="store_true", help="use the deterministic CI ensemble"
    )
    parser.add_argument("--json", action="store_true", help="emit JSON")
    args = parser.parse_args()

    results = run(quick=args.quick)
    self_check(results)

    if args.json:
        print(
            json.dumps(
                {
                    "configuration": {
                        "internal_capacity": INTERNAL_CAPACITY,
                        "internal_minimum": INTERNAL_MINIMUM,
                        "initial_internal_nodes": INITIAL_INTERNAL_NODES,
                        "initial_fill": INITIAL_FILL,
                        "quick": args.quick,
                        "unweighted_avoidable_cliff_share": UNWEIGHTED_AVOIDABLE_CLIFF_SHARE,
                        "kernels": KERNELS,
                        "two_timescale_calibration": "Experiment 0126 leaf merge rate",
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
