#!/usr/bin/env python3
"""Decompose EXP-0003 minimum-occupancy frontier mass into exact event flows.

Experiment 0124 established the exact deletion-underflow hazard

    lambda_underflow = 0.5 * MINIMUM / live_keys * E[n_M before deletion]

for the fixed-cardinality random-key/random-gap process.  This experiment asks why
the experimental fuller-sibling borrower leaves less mass at `occupancy == M`.

Every operation that changes the number `n_M` of minimum-occupancy leaves falls
into one of five exact classes for the current half-full leaf rule:

* insert from M:                 -1
* ordinary delete M+1 -> M:     +1
* borrow from an M+1 donor:     +1
* split C -> ceil((C+1)/2), M:  +1
* merge two minimum leaves:      -2

The classified increments must telescope exactly to

    n_M(final) - n_M(initial).

The key policy-sensitive term is `borrow from an M+1 donor`.  Borrowing repairs
the underflowed target back to M under either policy, but an M+1 donor is drained
down to M and therefore creates another immediately underflow-vulnerable leaf.
`FullerSiblingLeftTie` avoids this whenever the other eligible sibling is fuller.

This is deterministic-seed finite-horizon evidence, not a stationary proof and not
an EXP-0003 epoch or policy decision.
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
    observed_operations: int
    live_keys: int
    initial_minimum_leaf_count: int
    final_minimum_leaf_count: int
    actual_minimum_mass_delta: int
    insert_from_minimum: int
    delete_to_minimum: int
    borrow_donor_to_minimum: int
    split_creates_minimum: int
    merge_events: int
    merge_minimum_mass_removed: int
    classified_minimum_mass_delta: int
    underflow_events: int
    borrow_events: int
    borrow_donor_to_minimum_with_fuller_alternative: int
    borrow_donor_to_minimum_without_fuller_alternative: int
    donor_to_minimum_share_of_borrows: float
    mean_minimum_leaf_count_before_delete: float


@dataclass(frozen=True)
class Aggregate:
    policy: str
    seeds: tuple[int, ...]
    cycles_per_seed: int
    burn_in_cycles: int
    observed_operations: int
    live_keys: int
    summed_initial_minimum_leaf_count: int
    summed_final_minimum_leaf_count: int
    actual_minimum_mass_delta: int
    insert_from_minimum: int
    delete_to_minimum: int
    borrow_donor_to_minimum: int
    split_creates_minimum: int
    merge_events: int
    merge_minimum_mass_removed: int
    classified_minimum_mass_delta: int
    underflow_events: int
    borrow_events: int
    borrow_donor_to_minimum_with_fuller_alternative: int
    borrow_donor_to_minimum_without_fuller_alternative: int
    donor_to_minimum_share_of_borrows: float
    mean_minimum_leaf_count_before_delete: float


def make_initial_leaves() -> list[int]:
    total = round(CAPACITY * INITIAL_LEAVES * INITIAL_FILL)
    base, remainder = divmod(total, INITIAL_LEAVES)
    leaves = [base + 1] * remainder + [base] * (INITIAL_LEAVES - remainder)
    assert sum(leaves) == total
    assert all(MINIMUM <= occupancy <= CAPACITY for occupancy in leaves)
    return leaves


def choose_leaf(rng: random.Random, leaves: list[int], *, insertion: bool) -> int:
    maximum = CAPACITY + 1 if insertion else CAPACITY
    while True:
        index = rng.randrange(len(leaves))
        weight = leaves[index] + 1 if insertion else leaves[index]
        if rng.random() * maximum < weight:
            return index


def eligible_sibling_occupancies(leaves: list[int], index: int) -> tuple[int | None, int | None]:
    left = leaves[index - 1] if index > 0 and leaves[index - 1] > MINIMUM else None
    right = (
        leaves[index + 1]
        if index + 1 < len(leaves) and leaves[index + 1] > MINIMUM
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


def run_trial(policy: str, seed: int, cycles: int, burn_in: int) -> Trial:
    if policy not in POLICIES:
        raise ValueError(f"unknown policy: {policy}")
    if not 0 <= burn_in < cycles:
        raise ValueError("burn-in must be in [0, cycles)")

    rng = random.Random(seed)
    leaves = make_initial_leaves()
    live_keys = sum(leaves)

    initial_minimum_leaf_count: int | None = None
    insert_from_minimum = 0
    delete_to_minimum = 0
    borrow_donor_to_minimum = 0
    split_creates_minimum = 0
    merge_events = 0
    underflow_events = 0
    borrow_events = 0
    donor_to_min_with_fuller_alternative = 0
    donor_to_min_without_fuller_alternative = 0
    minimum_before_delete_sum = 0
    observed_deletions = 0

    def insert_one(*, collect: bool) -> None:
        nonlocal insert_from_minimum, split_creates_minimum

        before_minimum_count = sum(occupancy == MINIMUM for occupancy in leaves) if collect else 0
        index = choose_leaf(rng, leaves, insertion=True)
        old_occupancy = leaves[index]

        if old_occupancy < CAPACITY:
            leaves[index] += 1
            classified_delta = 0
            if collect and old_occupancy == MINIMUM:
                insert_from_minimum += 1
                classified_delta = -1
        else:
            overflow = CAPACITY + 1
            left = math.ceil(overflow / 2)
            right = math.floor(overflow / 2)
            assert (left, right) == (MINIMUM + 1, MINIMUM)
            leaves[index : index + 1] = [left, right]
            classified_delta = 0
            if collect:
                split_creates_minimum += 1
                classified_delta = 1

        if collect:
            after_minimum_count = sum(occupancy == MINIMUM for occupancy in leaves)
            assert after_minimum_count - before_minimum_count == classified_delta

    def delete_one(*, collect: bool) -> None:
        nonlocal delete_to_minimum, borrow_donor_to_minimum, merge_events
        nonlocal underflow_events, borrow_events
        nonlocal donor_to_min_with_fuller_alternative, donor_to_min_without_fuller_alternative
        nonlocal minimum_before_delete_sum, observed_deletions

        before_minimum_count = sum(occupancy == MINIMUM for occupancy in leaves) if collect else 0
        if collect:
            minimum_before_delete_sum += before_minimum_count
            observed_deletions += 1

        index = choose_leaf(rng, leaves, insertion=False)
        old_occupancy = leaves[index]
        leaves[index] -= 1

        if leaves[index] >= MINIMUM or len(leaves) == 1:
            classified_delta = 0
            if collect and old_occupancy == MINIMUM + 1:
                delete_to_minimum += 1
                classified_delta = 1
            if collect:
                after_minimum_count = sum(occupancy == MINIMUM for occupancy in leaves)
                assert after_minimum_count - before_minimum_count == classified_delta
            return

        assert old_occupancy == MINIMUM
        left_eligible, right_eligible = eligible_sibling_occupancies(leaves, index)
        side = choose_borrow_side(left_eligible, right_eligible, policy)

        classified_delta = 0
        if side is not None:
            chosen_occupancy = left_eligible if side == "left" else right_eligible
            other_occupancy = right_eligible if side == "left" else left_eligible
            assert chosen_occupancy is not None and chosen_occupancy > MINIMUM

            if side == "left":
                leaves[index - 1] -= 1
                leaves[index] += 1
            else:
                leaves[index + 1] -= 1
                leaves[index] += 1

            if collect:
                borrow_events += 1
                if chosen_occupancy == MINIMUM + 1:
                    borrow_donor_to_minimum += 1
                    classified_delta = 1
                    if other_occupancy is not None and other_occupancy > chosen_occupancy:
                        donor_to_min_with_fuller_alternative += 1
                    else:
                        donor_to_min_without_fuller_alternative += 1
        else:
            # All existing siblings are ineligible, hence exactly at MINIMUM.
            if index > 0:
                assert leaves[index - 1] == MINIMUM
                leaves[index - 1] += leaves[index]
                assert leaves[index - 1] <= CAPACITY
                leaves.pop(index)
            else:
                assert index + 1 < len(leaves)
                assert leaves[index + 1] == MINIMUM
                leaves[index] += leaves[index + 1]
                assert leaves[index] <= CAPACITY
                leaves.pop(index + 1)
            if collect:
                merge_events += 1
                classified_delta = -2

        if collect:
            underflow_events += 1
            after_minimum_count = sum(occupancy == MINIMUM for occupancy in leaves)
            assert after_minimum_count - before_minimum_count == classified_delta

    for cycle in range(cycles):
        collect = cycle >= burn_in
        if cycle == burn_in:
            initial_minimum_leaf_count = sum(
                occupancy == MINIMUM for occupancy in leaves
            )

        if rng.random() < 0.5:
            insert_one(collect=collect)
            delete_one(collect=collect)
        else:
            delete_one(collect=collect)
            insert_one(collect=collect)

        assert sum(leaves) == live_keys
        assert all(MINIMUM <= occupancy <= CAPACITY for occupancy in leaves)

    assert initial_minimum_leaf_count is not None
    final_minimum_leaf_count = sum(occupancy == MINIMUM for occupancy in leaves)
    actual_delta = final_minimum_leaf_count - initial_minimum_leaf_count
    merge_minimum_mass_removed = 2 * merge_events
    classified_delta = (
        delete_to_minimum
        + borrow_donor_to_minimum
        + split_creates_minimum
        - insert_from_minimum
        - merge_minimum_mass_removed
    )

    assert classified_delta == actual_delta
    assert underflow_events == borrow_events + merge_events
    assert borrow_donor_to_minimum == (
        donor_to_min_with_fuller_alternative + donor_to_min_without_fuller_alternative
    )
    if policy == "fuller-sibling":
        # The fuller-sibling policy can choose an M+1 donor only when no eligible
        # alternative is strictly fuller.
        assert donor_to_min_with_fuller_alternative == 0

    observed_operations = 2 * (cycles - burn_in)
    return Trial(
        policy=policy,
        seed=seed,
        cycles=cycles,
        burn_in_cycles=burn_in,
        observed_operations=observed_operations,
        live_keys=live_keys,
        initial_minimum_leaf_count=initial_minimum_leaf_count,
        final_minimum_leaf_count=final_minimum_leaf_count,
        actual_minimum_mass_delta=actual_delta,
        insert_from_minimum=insert_from_minimum,
        delete_to_minimum=delete_to_minimum,
        borrow_donor_to_minimum=borrow_donor_to_minimum,
        split_creates_minimum=split_creates_minimum,
        merge_events=merge_events,
        merge_minimum_mass_removed=merge_minimum_mass_removed,
        classified_minimum_mass_delta=classified_delta,
        underflow_events=underflow_events,
        borrow_events=borrow_events,
        borrow_donor_to_minimum_with_fuller_alternative=donor_to_min_with_fuller_alternative,
        borrow_donor_to_minimum_without_fuller_alternative=donor_to_min_without_fuller_alternative,
        donor_to_minimum_share_of_borrows=(
            borrow_donor_to_minimum / borrow_events if borrow_events else 0.0
        ),
        mean_minimum_leaf_count_before_delete=(
            minimum_before_delete_sum / observed_deletions
        ),
    )


def aggregate(trials: list[Trial]) -> Aggregate:
    if not trials:
        raise ValueError("cannot aggregate zero trials")
    first = trials[0]
    assert all(trial.policy == first.policy for trial in trials)
    assert all(trial.cycles == first.cycles for trial in trials)
    assert all(trial.burn_in_cycles == first.burn_in_cycles for trial in trials)
    assert all(trial.live_keys == first.live_keys for trial in trials)

    def total(field: str) -> int:
        return sum(int(getattr(trial, field)) for trial in trials)

    borrow_events = total("borrow_events")
    borrow_donor_to_minimum = total("borrow_donor_to_minimum")
    actual_delta = total("actual_minimum_mass_delta")
    classified_delta = total("classified_minimum_mass_delta")
    assert actual_delta == classified_delta

    return Aggregate(
        policy=first.policy,
        seeds=tuple(trial.seed for trial in trials),
        cycles_per_seed=first.cycles,
        burn_in_cycles=first.burn_in_cycles,
        observed_operations=sum(trial.observed_operations for trial in trials),
        live_keys=first.live_keys,
        summed_initial_minimum_leaf_count=total("initial_minimum_leaf_count"),
        summed_final_minimum_leaf_count=total("final_minimum_leaf_count"),
        actual_minimum_mass_delta=actual_delta,
        insert_from_minimum=total("insert_from_minimum"),
        delete_to_minimum=total("delete_to_minimum"),
        borrow_donor_to_minimum=borrow_donor_to_minimum,
        split_creates_minimum=total("split_creates_minimum"),
        merge_events=total("merge_events"),
        merge_minimum_mass_removed=total("merge_minimum_mass_removed"),
        classified_minimum_mass_delta=classified_delta,
        underflow_events=total("underflow_events"),
        borrow_events=borrow_events,
        borrow_donor_to_minimum_with_fuller_alternative=total(
            "borrow_donor_to_minimum_with_fuller_alternative"
        ),
        borrow_donor_to_minimum_without_fuller_alternative=total(
            "borrow_donor_to_minimum_without_fuller_alternative"
        ),
        donor_to_minimum_share_of_borrows=(
            borrow_donor_to_minimum / borrow_events if borrow_events else 0.0
        ),
        mean_minimum_leaf_count_before_delete=statistics.fmean(
            trial.mean_minimum_leaf_count_before_delete for trial in trials
        ),
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

    assert left.actual_minimum_mass_delta == left.classified_minimum_mass_delta
    assert fuller.actual_minimum_mass_delta == fuller.classified_minimum_mass_delta

    # The mechanism under test: fuller-sibling should create far fewer new minimum
    # leaves by draining barely eligible M+1 donors.
    assert fuller.borrow_donor_to_minimum < left.borrow_donor_to_minimum
    assert fuller.donor_to_minimum_share_of_borrows < left.donor_to_minimum_share_of_borrows
    assert left.borrow_donor_to_minimum_with_fuller_alternative > 0
    assert fuller.borrow_donor_to_minimum_with_fuller_alternative == 0

    # Fixed-seed guardrails are intentionally broad; they protect the qualitative
    # signal without turning Monte Carlo counts into normative constants.
    assert left.donor_to_minimum_share_of_borrows > 0.06
    assert fuller.donor_to_minimum_share_of_borrows < 0.04
    assert fuller.mean_minimum_leaf_count_before_delete < left.mean_minimum_leaf_count_before_delete

    if not quick:
        assert left.borrow_donor_to_minimum > 5_000
        assert fuller.borrow_donor_to_minimum < 3_000


def print_csv(aggregates: list[Aggregate]) -> None:
    print(
        "policy,seeds,cycles_per_seed,burn_in_cycles,observed_operations,live_keys,"
        "summed_initial_minimum_leaf_count,summed_final_minimum_leaf_count,"
        "actual_minimum_mass_delta,insert_from_minimum,delete_to_minimum,"
        "borrow_donor_to_minimum,split_creates_minimum,merge_events,"
        "merge_minimum_mass_removed,classified_minimum_mass_delta,underflow_events,"
        "borrow_events,borrow_donor_to_minimum_with_fuller_alternative,"
        "borrow_donor_to_minimum_without_fuller_alternative,"
        "donor_to_minimum_share_of_borrows,mean_minimum_leaf_count_before_delete"
    )
    for item in aggregates:
        seeds = "+".join(str(seed) for seed in item.seeds)
        print(
            f"{item.policy},{seeds},{item.cycles_per_seed},{item.burn_in_cycles},"
            f"{item.observed_operations},{item.live_keys},"
            f"{item.summed_initial_minimum_leaf_count},"
            f"{item.summed_final_minimum_leaf_count},{item.actual_minimum_mass_delta},"
            f"{item.insert_from_minimum},{item.delete_to_minimum},"
            f"{item.borrow_donor_to_minimum},{item.split_creates_minimum},"
            f"{item.merge_events},{item.merge_minimum_mass_removed},"
            f"{item.classified_minimum_mass_delta},{item.underflow_events},"
            f"{item.borrow_events},"
            f"{item.borrow_donor_to_minimum_with_fuller_alternative},"
            f"{item.borrow_donor_to_minimum_without_fuller_alternative},"
            f"{item.donor_to_minimum_share_of_borrows:.9f},"
            f"{item.mean_minimum_leaf_count_before_delete:.9f}"
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
