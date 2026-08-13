#!/usr/bin/env python3
"""Measure cross-level correlation at EXP-0003 recursive deletion frontiers.

Experiment 0132 estimates recursive internal repair frequency by multiplying a
leaf-merge rate by an independently modeled internal child-removal process.  The
remaining uncertainty is whether a *real leaf merge* reaches parent occupancies
with the same distribution as a child-proportional random internal removal.

This diagnostic couples the two levels directly.  Each parent owns an ordered
leaf-occupancy vector.  Object insertion/deletion selects leaves by gap/key
weight; a leaf split inserts one child into its actual parent; a leaf merge
removes one child from that same parent; and resulting parent overflow/underflow
is repaired with the same candidate borrower policy.

At every realized leaf merge, the experiment compares:

    actual_recursive = 1(parent_occupancy_before_child_removal == internal_M)

with the child-proportional independence baseline at the same event time:

    p_independent = internal_M * count(parents == internal_M) / total_leaf_count

The ratio of aggregate actual recursive frequency to aggregate p_independent is
reported as a cross-level selection factor.  A value different from one means a
leaf merge is not selecting parent state like a uniformly random child removal.

The starting state intentionally over-represents the internal minimum frontier
so CI observes enough recursive events in a bounded run.  This is therefore a
frontier-stress diagnostic, *not* a stationary workload-frequency estimate.  It
is run for both the current Rust research geometry and the first EXP-0003 Draft
geometry to expose geometry sensitivity rather than silently mixing them.

Non-normative research only.  No policy, epoch, vector, or wire-format decision
is made by this experiment.
"""

from __future__ import annotations

import argparse
import json
import random
import statistics
from collections import Counter
from dataclasses import asdict, dataclass


GEOMETRIES = {
    "rust-research": {
        "leaf_capacity": 185,
        "leaf_minimum": 93,
        "internal_capacity": 255,
        "internal_minimum": 128,
    },
    "exp0003-draft": {
        "leaf_capacity": 254,
        "leaf_minimum": 127,
        "internal_capacity": 226,
        "internal_minimum": 113,
    },
}

POLICIES = ("left-first", "fuller-sibling")
QUICK_SEEDS = (3, 17, 29)
QUICK_CYCLES = 80_000
QUICK_BURN_IN = 15_000
FULL_SEEDS = (3, 17, 29, 43, 71)
FULL_CYCLES = 200_000
FULL_BURN_IN = 40_000
INITIAL_PARENTS = 8
INITIAL_LEAF_FILL = 0.62


@dataclass(frozen=True)
class Trial:
    geometry: str
    policy: str
    seed: int
    cycles: int
    burn_in_cycles: int
    observed_object_operations: int
    leaf_underflows: int
    leaf_borrows: int
    leaf_merges: int
    leaf_splits: int
    leaf_merge_edge_targets: int
    recursive_internal_underflows: int
    internal_borrows: int
    internal_merges: int
    internal_splits: int
    independent_recursive_probability_sum: float
    final_parent_count: int
    final_leaf_count: int


@dataclass(frozen=True)
class Aggregate:
    geometry: str
    policy: str
    seeds: tuple[int, ...]
    cycles_per_seed: int
    burn_in_cycles: int
    observed_object_operations: int
    leaf_underflows: int
    leaf_borrows: int
    leaf_merges: int
    leaf_splits: int
    leaf_merge_edge_targets: int
    recursive_internal_underflows: int
    internal_borrows: int
    internal_merges: int
    internal_splits: int
    actual_recursive_share_of_leaf_merges: float
    independent_recursive_share_at_merge_times: float
    cross_level_selection_factor: float | None
    leaf_merge_rate_per_object_operation: float
    recursive_internal_underflow_rate_per_object_operation: float
    mean_final_parent_count: float
    mean_final_leaf_count: float


def split_pair(capacity: int) -> tuple[int, int]:
    total = capacity + 1
    left = (total + 1) // 2
    right = total - left
    return left, right


def choose_borrow_side(
    left: int | None,
    right: int | None,
    minimum: int,
    policy: str,
) -> str | None:
    left_can_lend = left is not None and left > minimum
    right_can_lend = right is not None and right > minimum

    if policy == "left-first":
        if left_can_lend:
            return "left"
        if right_can_lend:
            return "right"
        return None

    if policy == "fuller-sibling":
        if left_can_lend and right_can_lend:
            assert left is not None and right is not None
            return "left" if left >= right else "right"
        if left_can_lend:
            return "left"
        if right_can_lend:
            return "right"
        return None

    raise ValueError(f"unknown policy: {policy}")


def initial_state(geometry: dict[str, int], seed: int) -> tuple[list[list[int]], list[int]]:
    """Create a deterministic minimum-frontier-heavy valid two-level state."""

    rng = random.Random(seed ^ 0xA5A5A5)
    leaf_capacity = geometry["leaf_capacity"]
    leaf_minimum = geometry["leaf_minimum"]
    internal_capacity = geometry["internal_capacity"]
    internal_minimum = geometry["internal_minimum"]

    parent_occupancies = [
        internal_minimum,
        internal_minimum,
        internal_minimum,
        internal_minimum,
        internal_minimum + 1,
        internal_minimum + 1,
        internal_minimum + 2,
        internal_minimum + 4,
    ]
    assert len(parent_occupancies) == INITIAL_PARENTS
    assert all(
        internal_minimum <= occupancy <= internal_capacity
        for occupancy in parent_occupancies
    )

    target_leaf_occupancy = leaf_capacity * INITIAL_LEAF_FILL
    mean_increment = max(1.0, target_leaf_occupancy - leaf_minimum)

    parents: list[list[int]] = []
    object_counts: list[int] = []
    for parent_occupancy in parent_occupancies:
        leaves: list[int] = []
        for _ in range(parent_occupancy):
            increment = min(
                leaf_capacity - leaf_minimum,
                int(rng.expovariate(1.0 / mean_increment)),
            )
            leaves.append(leaf_minimum + increment)
        parents.append(leaves)
        object_counts.append(sum(leaves))

    return parents, object_counts


def choose_parent_leaf(
    rng: random.Random,
    parents: list[list[int]],
    object_counts: list[int],
    geometry: dict[str, int],
    *,
    insertion: bool,
) -> tuple[int, int]:
    """Select a random gap/key by bounded rejection without flattening the tree."""

    leaf_capacity = geometry["leaf_capacity"]
    internal_capacity = geometry["internal_capacity"]
    max_parent_weight = internal_capacity * (
        leaf_capacity + 1 if insertion else leaf_capacity
    )
    max_leaf_weight = leaf_capacity + 1 if insertion else leaf_capacity

    while True:
        parent_index = rng.randrange(len(parents))
        parent_weight = object_counts[parent_index]
        if insertion:
            parent_weight += len(parents[parent_index])
        if rng.random() * max_parent_weight >= parent_weight:
            continue

        leaves = parents[parent_index]
        while True:
            leaf_index = rng.randrange(len(leaves))
            leaf_weight = leaves[leaf_index] + (1 if insertion else 0)
            if rng.random() * max_leaf_weight < leaf_weight:
                return parent_index, leaf_index


def validate_state(
    parents: list[list[int]],
    object_counts: list[int],
    geometry: dict[str, int],
    expected_objects: int,
) -> None:
    leaf_capacity = geometry["leaf_capacity"]
    leaf_minimum = geometry["leaf_minimum"]
    internal_capacity = geometry["internal_capacity"]
    internal_minimum = geometry["internal_minimum"]

    assert len(parents) == len(object_counts)
    assert len(parents) >= 2
    assert sum(object_counts) == expected_objects
    for leaves, object_count in zip(parents, object_counts):
        assert internal_minimum <= len(leaves) <= internal_capacity
        assert sum(leaves) == object_count
        assert all(leaf_minimum <= occupancy <= leaf_capacity for occupancy in leaves)


def run_trial(
    *,
    geometry_name: str,
    policy: str,
    seed: int,
    cycles: int,
    burn_in_cycles: int,
) -> Trial:
    if geometry_name not in GEOMETRIES:
        raise ValueError(f"unknown geometry: {geometry_name}")
    if policy not in POLICIES:
        raise ValueError(f"unknown policy: {policy}")
    if not 0 <= burn_in_cycles < cycles:
        raise ValueError("burn-in must be in [0, cycles)")

    geometry = GEOMETRIES[geometry_name]
    leaf_capacity = geometry["leaf_capacity"]
    leaf_minimum = geometry["leaf_minimum"]
    internal_capacity = geometry["internal_capacity"]
    internal_minimum = geometry["internal_minimum"]

    rng = random.Random(seed)
    parents, object_counts = initial_state(geometry, seed)
    expected_objects = sum(object_counts)
    observed = Counter()
    independent_recursive_probability_sum = 0.0

    def repair_internal(parent_index: int, *, collect: bool) -> None:
        left = len(parents[parent_index - 1]) if parent_index > 0 else None
        right = (
            len(parents[parent_index + 1])
            if parent_index + 1 < len(parents)
            else None
        )
        side = choose_borrow_side(left, right, internal_minimum, policy)
        if side is not None:
            if collect:
                observed["internal_borrows"] += 1
            if side == "left":
                moved = parents[parent_index - 1].pop()
                object_counts[parent_index - 1] -= moved
                parents[parent_index].insert(0, moved)
                object_counts[parent_index] += moved
            else:
                moved = parents[parent_index + 1].pop(0)
                object_counts[parent_index + 1] -= moved
                parents[parent_index].append(moved)
                object_counts[parent_index] += moved
            return

        if collect:
            observed["internal_merges"] += 1
        if parent_index > 0:
            parents[parent_index - 1].extend(parents[parent_index])
            object_counts[parent_index - 1] += object_counts[parent_index]
            parents.pop(parent_index)
            object_counts.pop(parent_index)
        else:
            parents[parent_index].extend(parents[parent_index + 1])
            object_counts[parent_index] += object_counts[parent_index + 1]
            parents.pop(parent_index + 1)
            object_counts.pop(parent_index + 1)

    def split_parent_if_needed(parent_index: int, *, collect: bool) -> None:
        if len(parents[parent_index]) <= internal_capacity:
            return
        assert len(parents[parent_index]) == internal_capacity + 1
        left_count, right_count = split_pair(internal_capacity)
        old = parents[parent_index]
        left = old[:left_count]
        right = old[left_count:]
        assert len(right) == right_count
        parents[parent_index : parent_index + 1] = [left, right]
        object_counts[parent_index : parent_index + 1] = [sum(left), sum(right)]
        if collect:
            observed["internal_splits"] += 1

    def insert_one(*, collect: bool) -> None:
        parent_index, leaf_index = choose_parent_leaf(
            rng,
            parents,
            object_counts,
            geometry,
            insertion=True,
        )
        if parents[parent_index][leaf_index] < leaf_capacity:
            parents[parent_index][leaf_index] += 1
            object_counts[parent_index] += 1
            return

        left_count, right_count = split_pair(leaf_capacity)
        parents[parent_index][leaf_index : leaf_index + 1] = [left_count, right_count]
        object_counts[parent_index] += 1
        if collect:
            observed["leaf_splits"] += 1
        split_parent_if_needed(parent_index, collect=collect)

    def delete_one(*, collect: bool) -> None:
        nonlocal independent_recursive_probability_sum

        parent_index, leaf_index = choose_parent_leaf(
            rng,
            parents,
            object_counts,
            geometry,
            insertion=False,
        )
        old_occupancy = parents[parent_index][leaf_index]
        parents[parent_index][leaf_index] -= 1
        object_counts[parent_index] -= 1
        if parents[parent_index][leaf_index] >= leaf_minimum:
            return

        assert old_occupancy == leaf_minimum
        if collect:
            observed["leaf_underflows"] += 1

        left = (
            parents[parent_index][leaf_index - 1]
            if leaf_index > 0
            else None
        )
        right = (
            parents[parent_index][leaf_index + 1]
            if leaf_index + 1 < len(parents[parent_index])
            else None
        )
        side = choose_borrow_side(left, right, leaf_minimum, policy)
        if side is not None:
            if collect:
                observed["leaf_borrows"] += 1
            if side == "left":
                parents[parent_index][leaf_index - 1] -= 1
                parents[parent_index][leaf_index] += 1
            else:
                parents[parent_index][leaf_index + 1] -= 1
                parents[parent_index][leaf_index] += 1
            return

        parent_occupancy_before_child_removal = len(parents[parent_index])
        if collect:
            observed["leaf_merges"] += 1
            if leaf_index == 0 or leaf_index + 1 == parent_occupancy_before_child_removal:
                observed["leaf_merge_edge_targets"] += 1
            if parent_occupancy_before_child_removal == internal_minimum:
                observed["recursive_internal_underflows"] += 1

            minimum_parents = sum(
                len(parent) == internal_minimum for parent in parents
            )
            total_leaf_count = sum(len(parent) for parent in parents)
            independent_recursive_probability_sum += (
                internal_minimum * minimum_parents / total_leaf_count
            )

        if leaf_index > 0:
            parents[parent_index][leaf_index - 1] += parents[parent_index][leaf_index]
            parents[parent_index].pop(leaf_index)
        else:
            parents[parent_index][leaf_index] += parents[parent_index][leaf_index + 1]
            parents[parent_index].pop(leaf_index + 1)

        if len(parents[parent_index]) < internal_minimum:
            repair_internal(parent_index, collect=collect)

    for cycle in range(cycles):
        collect = cycle >= burn_in_cycles
        if rng.random() < 0.5:
            insert_one(collect=collect)
            delete_one(collect=collect)
        else:
            delete_one(collect=collect)
            insert_one(collect=collect)

        if cycle % 1024 == 0 or cycle + 1 == cycles:
            validate_state(parents, object_counts, geometry, expected_objects)

    observed_cycles = cycles - burn_in_cycles
    leaf_merges = observed["leaf_merges"]
    assert observed["leaf_underflows"] == observed["leaf_borrows"] + leaf_merges
    assert observed["recursive_internal_underflows"] == (
        observed["internal_borrows"] + observed["internal_merges"]
    )

    return Trial(
        geometry=geometry_name,
        policy=policy,
        seed=seed,
        cycles=cycles,
        burn_in_cycles=burn_in_cycles,
        observed_object_operations=2 * observed_cycles,
        leaf_underflows=observed["leaf_underflows"],
        leaf_borrows=observed["leaf_borrows"],
        leaf_merges=leaf_merges,
        leaf_splits=observed["leaf_splits"],
        leaf_merge_edge_targets=observed["leaf_merge_edge_targets"],
        recursive_internal_underflows=observed["recursive_internal_underflows"],
        internal_borrows=observed["internal_borrows"],
        internal_merges=observed["internal_merges"],
        internal_splits=observed["internal_splits"],
        independent_recursive_probability_sum=independent_recursive_probability_sum,
        final_parent_count=len(parents),
        final_leaf_count=sum(len(parent) for parent in parents),
    )


def aggregate(trials: list[Trial]) -> Aggregate:
    if not trials:
        raise ValueError("cannot aggregate zero trials")
    first = trials[0]
    assert all(trial.geometry == first.geometry for trial in trials)
    assert all(trial.policy == first.policy for trial in trials)
    assert all(trial.cycles == first.cycles for trial in trials)
    assert all(trial.burn_in_cycles == first.burn_in_cycles for trial in trials)

    def total(field: str) -> int:
        return sum(getattr(trial, field) for trial in trials)

    merges = total("leaf_merges")
    recursive = total("recursive_internal_underflows")
    operations = total("observed_object_operations")
    independent_sum = sum(
        trial.independent_recursive_probability_sum for trial in trials
    )
    actual_share = recursive / merges if merges else 0.0
    independent_share = independent_sum / merges if merges else 0.0
    factor = actual_share / independent_share if independent_share > 0.0 else None

    return Aggregate(
        geometry=first.geometry,
        policy=first.policy,
        seeds=tuple(trial.seed for trial in trials),
        cycles_per_seed=first.cycles,
        burn_in_cycles=first.burn_in_cycles,
        observed_object_operations=operations,
        leaf_underflows=total("leaf_underflows"),
        leaf_borrows=total("leaf_borrows"),
        leaf_merges=merges,
        leaf_splits=total("leaf_splits"),
        leaf_merge_edge_targets=total("leaf_merge_edge_targets"),
        recursive_internal_underflows=recursive,
        internal_borrows=total("internal_borrows"),
        internal_merges=total("internal_merges"),
        internal_splits=total("internal_splits"),
        actual_recursive_share_of_leaf_merges=actual_share,
        independent_recursive_share_at_merge_times=independent_share,
        cross_level_selection_factor=factor,
        leaf_merge_rate_per_object_operation=merges / operations,
        recursive_internal_underflow_rate_per_object_operation=(
            recursive / operations
        ),
        mean_final_parent_count=statistics.fmean(
            trial.final_parent_count for trial in trials
        ),
        mean_final_leaf_count=statistics.fmean(
            trial.final_leaf_count for trial in trials
        ),
    )


def run(*, quick: bool) -> tuple[list[Trial], list[Aggregate]]:
    seeds = QUICK_SEEDS if quick else FULL_SEEDS
    cycles = QUICK_CYCLES if quick else FULL_CYCLES
    burn_in = QUICK_BURN_IN if quick else FULL_BURN_IN

    trials: list[Trial] = []
    aggregates: list[Aggregate] = []
    for geometry_name in GEOMETRIES:
        for policy in POLICIES:
            group = [
                run_trial(
                    geometry_name=geometry_name,
                    policy=policy,
                    seed=seed,
                    cycles=cycles,
                    burn_in_cycles=burn_in,
                )
                for seed in seeds
            ]
            trials.extend(group)
            aggregates.append(aggregate(group))

    # The stress ensemble must actually exercise the intended recursive boundary.
    assert all(item.leaf_merges > 0 for item in aggregates)
    assert all(item.recursive_internal_underflows > 0 for item in aggregates)
    # At least one geometry/policy must falsify a near-exact child-proportional
    # closure materially; otherwise this diagnostic has lost its discriminating
    # stress property.
    finite_factors = [
        item.cross_level_selection_factor
        for item in aggregates
        if item.cross_level_selection_factor is not None
    ]
    assert finite_factors
    assert any(abs(factor - 1.0) >= 0.15 for factor in finite_factors)

    return trials, aggregates


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    trials, aggregates = run(quick=args.quick)
    if args.json:
        print(
            json.dumps(
                {
                    "trials": [asdict(item) for item in trials],
                    "aggregates": [asdict(item) for item in aggregates],
                },
                indent=2,
                sort_keys=True,
            )
        )
        return

    print(
        "geometry,policy,leaf_merges,recursive_internal_underflows,"
        "actual_recursive_share,independent_share,cross_level_factor,"
        "leaf_merge_rate_per_op,recursive_rate_per_op"
    )
    for item in aggregates:
        factor = (
            "none"
            if item.cross_level_selection_factor is None
            else f"{item.cross_level_selection_factor:.6f}"
        )
        print(
            f"{item.geometry},{item.policy},{item.leaf_merges},"
            f"{item.recursive_internal_underflows},"
            f"{item.actual_recursive_share_of_leaf_merges:.6f},"
            f"{item.independent_recursive_share_at_merge_times:.6f},"
            f"{factor},{item.leaf_merge_rate_per_object_operation:.9f},"
            f"{item.recursive_internal_underflow_rate_per_object_operation:.9f}"
        )


if __name__ == "__main__":
    main()
