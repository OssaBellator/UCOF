#!/usr/bin/env python3
"""Validate an exact one-step drift kernel for EXP-0003 minimum-frontier mass.

Experiments 0124 and 0125 established:

* underflow arrival is controlled by minimum-leaf mass `n_M`;
* every realized change in `n_M` belongs to five exact event-flow classes;
* fuller-sibling strongly suppresses the borrow-from-M+1 donor cliff.

This experiment moves from realized flow accounting to conditional expectation.
For a fixed occupancy state, the random-key/random-gap process gives exact hazards
for each of the five `n_M` flow events.  Those hazards require only six summary
statistics:

    n_M, n_(M+1), n_C, leaf_count,
    minimum targets whose selected donor is M+1,
    minimum targets with no eligible donor (merge targets).

The full ordered occupancy vector is still needed to *update* those statistics and
therefore this is not a Markov-lumpability claim.  It is a one-step sufficient
statistic for the expected reward/drift `E[Delta n_M | state]`.

The experiment compares accumulated exact conditional expectations with realized
Monte Carlo event counts under left-first and fuller-sibling borrowing.  Rare-event
residuals are treated as finite-sample noise, not fitted model parameters.
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
FLOW_NAMES = (
    "insert_from_minimum",
    "split_creates_minimum",
    "delete_to_minimum",
    "borrow_donor_to_minimum",
    "merge",
)


@dataclass(frozen=True)
class FlowClosure:
    observed: int
    expected: float
    relative_residual: float


@dataclass(frozen=True)
class Trial:
    policy: str
    seed: int
    cycles: int
    burn_in_cycles: int
    observed_operations: int
    live_keys: int
    insert_from_minimum: FlowClosure
    split_creates_minimum: FlowClosure
    delete_to_minimum: FlowClosure
    borrow_donor_to_minimum: FlowClosure
    merge: FlowClosure
    mean_minimum_before_insert: float
    mean_full_before_insert: float
    mean_leaf_count_before_insert: float
    mean_minimum_before_delete: float
    mean_minimum_plus_one_before_delete: float
    mean_cliff_targets_before_delete: float
    mean_merge_targets_before_delete: float
    mean_neutral_borrow_targets_before_delete: float


@dataclass(frozen=True)
class Aggregate:
    policy: str
    seeds: tuple[int, ...]
    cycles_per_seed: int
    burn_in_cycles: int
    observed_operations: int
    live_keys: int
    insert_from_minimum: FlowClosure
    split_creates_minimum: FlowClosure
    delete_to_minimum: FlowClosure
    borrow_donor_to_minimum: FlowClosure
    merge: FlowClosure
    mean_minimum_before_insert: float
    mean_full_before_insert: float
    mean_leaf_count_before_insert: float
    mean_minimum_before_delete: float
    mean_minimum_plus_one_before_delete: float
    mean_cliff_targets_before_delete: float
    mean_merge_targets_before_delete: float
    mean_neutral_borrow_targets_before_delete: float
    expected_insert_drift: float
    observed_insert_drift: int
    expected_delete_drift: float
    observed_delete_drift: int


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


def eligible_siblings(leaves: list[int], index: int) -> tuple[int | None, int | None]:
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


def classify_minimum_targets(leaves: list[int], policy: str) -> tuple[int, int, int]:
    """Return (M+1 donor cliff, merge, neutral-borrow) minimum target counts."""

    cliff = merge = neutral = 0
    minimum_count = 0
    for index, occupancy in enumerate(leaves):
        if occupancy != MINIMUM:
            continue
        minimum_count += 1
        left, right = eligible_siblings(leaves, index)
        side = choose_borrow_side(left, right, policy)
        if side is None:
            merge += 1
            continue
        donor = left if side == "left" else right
        assert donor is not None and donor > MINIMUM
        if donor == MINIMUM + 1:
            cliff += 1
        else:
            neutral += 1

    assert cliff + merge + neutral == minimum_count
    return cliff, merge, neutral


def relative_residual(observed: int, expected: float) -> float:
    if expected == 0.0:
        return 0.0 if observed == 0 else math.inf
    return (observed - expected) / expected


def closure(observed: int, expected: float) -> FlowClosure:
    return FlowClosure(
        observed=observed,
        expected=expected,
        relative_residual=relative_residual(observed, expected),
    )


def run_trial(policy: str, seed: int, cycles: int, burn_in: int) -> Trial:
    if policy not in POLICIES:
        raise ValueError(f"unknown policy: {policy}")
    if not 0 <= burn_in < cycles:
        raise ValueError("burn-in must be in [0, cycles)")

    rng = random.Random(seed)
    leaves = make_initial_leaves()
    live_keys = sum(leaves)

    observed = {name: 0 for name in FLOW_NAMES}
    expected = {name: 0.0 for name in FLOW_NAMES}

    minimum_before_insert_sum = 0
    full_before_insert_sum = 0
    leaf_count_before_insert_sum = 0
    minimum_before_delete_sum = 0
    minimum_plus_one_before_delete_sum = 0
    cliff_targets_before_delete_sum = 0
    merge_targets_before_delete_sum = 0
    neutral_targets_before_delete_sum = 0
    observations = 0

    def insert_one(*, collect: bool) -> None:
        nonlocal leaves

        if collect:
            n_minimum = sum(occupancy == MINIMUM for occupancy in leaves)
            n_full = sum(occupancy == CAPACITY for occupancy in leaves)
            leaf_count = len(leaves)
            minimum_before_insert_sum_local = n_minimum
            full_before_insert_sum_local = n_full
            leaf_count_before_insert_sum_local = leaf_count

            expected["insert_from_minimum"] += (
                (MINIMUM + 1) * n_minimum / (live_keys + leaf_count)
            )
            expected["split_creates_minimum"] += (
                (CAPACITY + 1) * n_full / (live_keys + leaf_count)
            )
        else:
            minimum_before_insert_sum_local = 0
            full_before_insert_sum_local = 0
            leaf_count_before_insert_sum_local = 0

        # Python requires writes to enclosing scalar accumulators to be declared;
        # keep those writes after target selection semantics have been recorded.
        if collect:
            nonlocal minimum_before_insert_sum, full_before_insert_sum
            nonlocal leaf_count_before_insert_sum
            minimum_before_insert_sum += minimum_before_insert_sum_local
            full_before_insert_sum += full_before_insert_sum_local
            leaf_count_before_insert_sum += leaf_count_before_insert_sum_local

        index = choose_leaf(rng, leaves, insertion=True)
        old_occupancy = leaves[index]
        if old_occupancy < CAPACITY:
            leaves[index] += 1
            if collect and old_occupancy == MINIMUM:
                observed["insert_from_minimum"] += 1
        else:
            leaves[index : index + 1] = [MINIMUM + 1, MINIMUM]
            if collect:
                observed["split_creates_minimum"] += 1

    def delete_one(*, collect: bool) -> None:
        nonlocal leaves
        nonlocal minimum_before_delete_sum, minimum_plus_one_before_delete_sum
        nonlocal cliff_targets_before_delete_sum, merge_targets_before_delete_sum
        nonlocal neutral_targets_before_delete_sum

        if collect:
            n_minimum = sum(occupancy == MINIMUM for occupancy in leaves)
            n_minimum_plus_one = sum(
                occupancy == MINIMUM + 1 for occupancy in leaves
            )
            cliff_targets, merge_targets, neutral_targets = classify_minimum_targets(
                leaves, policy
            )
            minimum_before_delete_sum += n_minimum
            minimum_plus_one_before_delete_sum += n_minimum_plus_one
            cliff_targets_before_delete_sum += cliff_targets
            merge_targets_before_delete_sum += merge_targets
            neutral_targets_before_delete_sum += neutral_targets

            expected["delete_to_minimum"] += (
                (MINIMUM + 1) * n_minimum_plus_one / live_keys
            )
            expected["borrow_donor_to_minimum"] += (
                MINIMUM * cliff_targets / live_keys
            )
            expected["merge"] += MINIMUM * merge_targets / live_keys

        index = choose_leaf(rng, leaves, insertion=False)
        old_occupancy = leaves[index]
        leaves[index] -= 1

        if leaves[index] >= MINIMUM:
            if collect and old_occupancy == MINIMUM + 1:
                observed["delete_to_minimum"] += 1
            return

        assert old_occupancy == MINIMUM
        left, right = eligible_siblings(leaves, index)
        side = choose_borrow_side(left, right, policy)
        if side is not None:
            donor = left if side == "left" else right
            assert donor is not None
            if side == "left":
                leaves[index - 1] -= 1
                leaves[index] += 1
            else:
                leaves[index + 1] -= 1
                leaves[index] += 1
            if collect and donor == MINIMUM + 1:
                observed["borrow_donor_to_minimum"] += 1
            return

        if index > 0:
            assert leaves[index - 1] == MINIMUM
            leaves[index - 1] += leaves[index]
            leaves.pop(index)
        else:
            assert leaves[index + 1] == MINIMUM
            leaves[index] += leaves[index + 1]
            leaves.pop(index + 1)
        if collect:
            observed["merge"] += 1

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
            observations += 1

    return Trial(
        policy=policy,
        seed=seed,
        cycles=cycles,
        burn_in_cycles=burn_in,
        observed_operations=2 * observations,
        live_keys=live_keys,
        insert_from_minimum=closure(
            observed["insert_from_minimum"], expected["insert_from_minimum"]
        ),
        split_creates_minimum=closure(
            observed["split_creates_minimum"], expected["split_creates_minimum"]
        ),
        delete_to_minimum=closure(
            observed["delete_to_minimum"], expected["delete_to_minimum"]
        ),
        borrow_donor_to_minimum=closure(
            observed["borrow_donor_to_minimum"],
            expected["borrow_donor_to_minimum"],
        ),
        merge=closure(observed["merge"], expected["merge"]),
        mean_minimum_before_insert=minimum_before_insert_sum / observations,
        mean_full_before_insert=full_before_insert_sum / observations,
        mean_leaf_count_before_insert=leaf_count_before_insert_sum / observations,
        mean_minimum_before_delete=minimum_before_delete_sum / observations,
        mean_minimum_plus_one_before_delete=(
            minimum_plus_one_before_delete_sum / observations
        ),
        mean_cliff_targets_before_delete=cliff_targets_before_delete_sum / observations,
        mean_merge_targets_before_delete=merge_targets_before_delete_sum / observations,
        mean_neutral_borrow_targets_before_delete=(
            neutral_targets_before_delete_sum / observations
        ),
    )


def combine_closure(trials: list[Trial], field: str) -> FlowClosure:
    observed = sum(getattr(trial, field).observed for trial in trials)
    expected = sum(getattr(trial, field).expected for trial in trials)
    return closure(observed, expected)


def aggregate(trials: list[Trial]) -> Aggregate:
    if not trials:
        raise ValueError("cannot aggregate zero trials")
    first = trials[0]
    assert all(trial.policy == first.policy for trial in trials)
    assert all(trial.cycles == first.cycles for trial in trials)
    assert all(trial.burn_in_cycles == first.burn_in_cycles for trial in trials)
    assert all(trial.live_keys == first.live_keys for trial in trials)

    insert_from_minimum = combine_closure(trials, "insert_from_minimum")
    split_creates_minimum = combine_closure(trials, "split_creates_minimum")
    delete_to_minimum = combine_closure(trials, "delete_to_minimum")
    borrow_donor_to_minimum = combine_closure(trials, "borrow_donor_to_minimum")
    merge = combine_closure(trials, "merge")

    def mean(field: str) -> float:
        return statistics.fmean(getattr(trial, field) for trial in trials)

    expected_insert_drift = (
        -insert_from_minimum.expected + split_creates_minimum.expected
    )
    observed_insert_drift = (
        -insert_from_minimum.observed + split_creates_minimum.observed
    )
    expected_delete_drift = (
        delete_to_minimum.expected
        + borrow_donor_to_minimum.expected
        - 2.0 * merge.expected
    )
    observed_delete_drift = (
        delete_to_minimum.observed
        + borrow_donor_to_minimum.observed
        - 2 * merge.observed
    )

    return Aggregate(
        policy=first.policy,
        seeds=tuple(trial.seed for trial in trials),
        cycles_per_seed=first.cycles,
        burn_in_cycles=first.burn_in_cycles,
        observed_operations=sum(trial.observed_operations for trial in trials),
        live_keys=first.live_keys,
        insert_from_minimum=insert_from_minimum,
        split_creates_minimum=split_creates_minimum,
        delete_to_minimum=delete_to_minimum,
        borrow_donor_to_minimum=borrow_donor_to_minimum,
        merge=merge,
        mean_minimum_before_insert=mean("mean_minimum_before_insert"),
        mean_full_before_insert=mean("mean_full_before_insert"),
        mean_leaf_count_before_insert=mean("mean_leaf_count_before_insert"),
        mean_minimum_before_delete=mean("mean_minimum_before_delete"),
        mean_minimum_plus_one_before_delete=mean(
            "mean_minimum_plus_one_before_delete"
        ),
        mean_cliff_targets_before_delete=mean("mean_cliff_targets_before_delete"),
        mean_merge_targets_before_delete=mean("mean_merge_targets_before_delete"),
        mean_neutral_borrow_targets_before_delete=mean(
            "mean_neutral_borrow_targets_before_delete"
        ),
        expected_insert_drift=expected_insert_drift,
        observed_insert_drift=observed_insert_drift,
        expected_delete_drift=expected_delete_drift,
        observed_delete_drift=observed_delete_drift,
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

    # High-frequency flow hazards should close tightly in the deterministic quick
    # ensemble.  Split/cliff/merge events are much rarer and get a wider envelope.
    for item in aggregates:
        assert abs(item.insert_from_minimum.relative_residual) < 0.03
        assert abs(item.delete_to_minimum.relative_residual) < 0.03
        assert abs(item.split_creates_minimum.relative_residual) < 0.15
        assert abs(item.borrow_donor_to_minimum.relative_residual) < 0.15
        assert abs(item.merge.relative_residual) < 0.15

        # The target classes partition all minimum leaves before deletion.
        assert math.isclose(
            item.mean_minimum_before_delete,
            item.mean_cliff_targets_before_delete
            + item.mean_merge_targets_before_delete
            + item.mean_neutral_borrow_targets_before_delete,
            rel_tol=1e-12,
            abs_tol=1e-12,
        )

    # Policy signal inherited from 0124/0125 but expressed as kernel-state mass.
    assert fuller.mean_minimum_before_delete < left.mean_minimum_before_delete
    assert fuller.mean_cliff_targets_before_delete < left.mean_cliff_targets_before_delete
    assert fuller.borrow_donor_to_minimum.observed < left.borrow_donor_to_minimum.observed

    if not quick:
        assert left.borrow_donor_to_minimum.observed > 5_000
        assert fuller.borrow_donor_to_minimum.observed < 3_000


def print_csv(aggregates: list[Aggregate]) -> None:
    print(
        "policy,seeds,observed_operations,live_keys,flow,observed,expected,"
        "relative_residual"
    )
    for item in aggregates:
        seeds = "+".join(str(seed) for seed in item.seeds)
        for field in FLOW_NAMES:
            value = getattr(item, field)
            print(
                f"{item.policy},{seeds},{item.observed_operations},{item.live_keys},"
                f"{field},{value.observed},{value.expected:.9f},"
                f"{value.relative_residual:.9f}"
            )

        print(
            f"# {item.policy}: mean_min_before_insert={item.mean_minimum_before_insert:.9f},"
            f"mean_full_before_insert={item.mean_full_before_insert:.9f},"
            f"mean_leaf_count_before_insert={item.mean_leaf_count_before_insert:.9f},"
            f"mean_min_before_delete={item.mean_minimum_before_delete:.9f},"
            f"mean_min_plus_one_before_delete={item.mean_minimum_plus_one_before_delete:.9f},"
            f"mean_cliff_targets={item.mean_cliff_targets_before_delete:.9f},"
            f"mean_merge_targets={item.mean_merge_targets_before_delete:.9f},"
            f"mean_neutral_borrow_targets={item.mean_neutral_borrow_targets_before_delete:.9f},"
            f"expected_insert_drift={item.expected_insert_drift:.9f},"
            f"observed_insert_drift={item.observed_insert_drift},"
            f"expected_delete_drift={item.expected_delete_drift:.9f},"
            f"observed_delete_drift={item.observed_delete_drift}"
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
