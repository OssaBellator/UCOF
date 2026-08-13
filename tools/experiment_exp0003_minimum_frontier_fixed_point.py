#!/usr/bin/env python3
"""Test a phase-blind stationary closure for EXP-0003 minimum-frontier mass.

Experiment 0126 gives an exact one-step expected drift for `n_M` from six compact
statistics.  This experiment asks whether setting that drift to zero and replacing
state-dependent ratios by ratios of finite-horizon means can predict minimum-leaf
mass without feeding the observed `n_M` back into the fixed point.

The closure uses five measured inputs from Experiment 0126:

    mean n_(M+1), mean n_C, mean leaf count,
    mean donor-cliff target count, mean merge-target count.

It solves for `n_M*` from zero expected cycle drift.  Because the workload
randomizes insert/delete order, the actual process has two nearby operation phases:
minimum mass before insertion and minimum mass before deletion.  A single
phase-blind fixed point is therefore compared with both phases and with their
midpoint.

This is an approximate stationary closure diagnostic, not a proof of stationarity,
not a Markov-lumpability claim, and not an EXP-0003 policy/epoch decision.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass

import experiment_exp0003_minimum_frontier_drift as drift


@dataclass(frozen=True)
class FixedPointResult:
    policy: str
    seeds: tuple[int, ...]
    observed_operations: int
    live_keys: int
    predicted_minimum_mass: float
    observed_pre_insert_minimum_mass: float
    observed_pre_delete_minimum_mass: float
    observed_phase_midpoint_minimum_mass: float
    pre_insert_relative_error: float
    pre_delete_relative_error: float
    midpoint_relative_error: float
    phase_gap: float
    predicted_underflow_rate_per_operation: float
    observed_state_hazard_rate_per_operation: float
    underflow_hazard_relative_error: float
    mean_minimum_plus_one_before_delete: float
    mean_full_before_insert: float
    mean_leaf_count_before_insert: float
    mean_cliff_targets_before_delete: float
    mean_merge_targets_before_delete: float


def relative_error(predicted: float, observed: float) -> float:
    if observed == 0.0:
        return 0.0 if predicted == 0.0 else float("inf")
    return (predicted - observed) / observed


def solve_fixed_point(item: drift.Aggregate) -> FixedPointResult:
    """Solve zero mean drift without using observed n_M as a predictor input."""

    k = float(item.live_keys)
    m = float(drift.MINIMUM)
    c = float(drift.CAPACITY)
    leaf_count = item.mean_leaf_count_before_insert

    # Approximate E[n_C/(K+L)] by E[n_C]/(K+E[L]).  The other deletion
    # denominators are exactly constant because live cardinality K is fixed.
    split_source = (c + 1.0) * item.mean_full_before_insert / (k + leaf_count)
    delete_to_min_source = (
        (m + 1.0) * item.mean_minimum_plus_one_before_delete / k
    )
    cliff_source = m * item.mean_cliff_targets_before_delete / k
    merge_sink = 2.0 * m * item.mean_merge_targets_before_delete / k

    # Zero phase-blind expected drift:
    #
    #   -(M+1)n_M*/(K+E[L])
    #   + split_source + delete_to_min_source + cliff_source - merge_sink = 0.
    other_flow = split_source + delete_to_min_source + cliff_source - merge_sink
    predicted = other_flow * (k + leaf_count) / (m + 1.0)

    observed_insert = item.mean_minimum_before_insert
    observed_delete = item.mean_minimum_before_delete
    midpoint = 0.5 * (observed_insert + observed_delete)
    phase_gap = observed_insert - observed_delete

    # Experiment 0124's underflow arrival law, now driven by the fixed-point n_M*
    # rather than the observed pre-delete n_M.
    predicted_underflow = 0.5 * m * predicted / k
    state_hazard_underflow = 0.5 * m * observed_delete / k

    return FixedPointResult(
        policy=item.policy,
        seeds=item.seeds,
        observed_operations=item.observed_operations,
        live_keys=item.live_keys,
        predicted_minimum_mass=predicted,
        observed_pre_insert_minimum_mass=observed_insert,
        observed_pre_delete_minimum_mass=observed_delete,
        observed_phase_midpoint_minimum_mass=midpoint,
        pre_insert_relative_error=relative_error(predicted, observed_insert),
        pre_delete_relative_error=relative_error(predicted, observed_delete),
        midpoint_relative_error=relative_error(predicted, midpoint),
        phase_gap=phase_gap,
        predicted_underflow_rate_per_operation=predicted_underflow,
        observed_state_hazard_rate_per_operation=state_hazard_underflow,
        underflow_hazard_relative_error=relative_error(
            predicted_underflow, state_hazard_underflow
        ),
        mean_minimum_plus_one_before_delete=item.mean_minimum_plus_one_before_delete,
        mean_full_before_insert=item.mean_full_before_insert,
        mean_leaf_count_before_insert=leaf_count,
        mean_cliff_targets_before_delete=item.mean_cliff_targets_before_delete,
        mean_merge_targets_before_delete=item.mean_merge_targets_before_delete,
    )


def run(*, quick: bool) -> list[FixedPointResult]:
    _, aggregates = drift.run_ensemble(quick=quick)
    return [solve_fixed_point(item) for item in aggregates]


def self_check(results: list[FixedPointResult], *, quick: bool) -> None:
    by_policy = {item.policy: item for item in results}
    left = by_policy["left-first"]
    fuller = by_policy["fuller-sibling"]

    # The phase-blind closure should land much closer to the midpoint than to
    # either phase boundary.  The quick deterministic ensemble already supports
    # a tight midpoint closure; full mode uses the same envelope rather than
    # turning a Monte Carlo number into a normative constant.
    for item in results:
        assert abs(item.midpoint_relative_error) < 0.01
        assert abs(item.pre_insert_relative_error) < 0.03
        assert abs(item.pre_delete_relative_error) < 0.03
        assert abs(item.underflow_hazard_relative_error) < 0.03
        assert item.observed_pre_insert_minimum_mass > item.observed_pre_delete_minimum_mass
        assert (
            item.observed_pre_delete_minimum_mass
            < item.predicted_minimum_mass
            < item.observed_pre_insert_minimum_mass
        )

    # The reduced fixed point must preserve the policy ordering found by 0124–0126.
    assert fuller.predicted_minimum_mass < left.predicted_minimum_mass
    assert (
        fuller.predicted_underflow_rate_per_operation
        < left.predicted_underflow_rate_per_operation
    )

    # The policy gap should remain large compared with the sub-percent midpoint
    # closure error, so the approximation does not manufacture the ordering.
    policy_gap = left.predicted_minimum_mass - fuller.predicted_minimum_mass
    assert policy_gap > 0.20

    if not quick:
        assert abs(left.midpoint_relative_error) < 0.03
        assert abs(fuller.midpoint_relative_error) < 0.03


def print_csv(results: list[FixedPointResult]) -> None:
    print(
        "policy,seeds,observed_operations,live_keys,predicted_minimum_mass,"
        "observed_pre_insert_minimum_mass,observed_pre_delete_minimum_mass,"
        "observed_phase_midpoint_minimum_mass,pre_insert_relative_error,"
        "pre_delete_relative_error,midpoint_relative_error,phase_gap,"
        "predicted_underflow_rate_per_operation,"
        "observed_state_hazard_rate_per_operation,underflow_hazard_relative_error,"
        "mean_minimum_plus_one_before_delete,mean_full_before_insert,"
        "mean_leaf_count_before_insert,mean_cliff_targets_before_delete,"
        "mean_merge_targets_before_delete"
    )
    for item in results:
        seeds = "+".join(str(seed) for seed in item.seeds)
        print(
            f"{item.policy},{seeds},{item.observed_operations},{item.live_keys},"
            f"{item.predicted_minimum_mass:.9f},"
            f"{item.observed_pre_insert_minimum_mass:.9f},"
            f"{item.observed_pre_delete_minimum_mass:.9f},"
            f"{item.observed_phase_midpoint_minimum_mass:.9f},"
            f"{item.pre_insert_relative_error:.9f},"
            f"{item.pre_delete_relative_error:.9f},"
            f"{item.midpoint_relative_error:.9f},{item.phase_gap:.9f},"
            f"{item.predicted_underflow_rate_per_operation:.12g},"
            f"{item.observed_state_hazard_rate_per_operation:.12g},"
            f"{item.underflow_hazard_relative_error:.9f},"
            f"{item.mean_minimum_plus_one_before_delete:.9f},"
            f"{item.mean_full_before_insert:.9f},"
            f"{item.mean_leaf_count_before_insert:.9f},"
            f"{item.mean_cliff_targets_before_delete:.9f},"
            f"{item.mean_merge_targets_before_delete:.9f}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--quick", action="store_true", help="use the deterministic CI ensemble"
    )
    parser.add_argument("--json", action="store_true", help="emit JSON")
    args = parser.parse_args()

    results = run(quick=args.quick)
    self_check(results, quick=args.quick)

    if args.json:
        print(
            json.dumps(
                {
                    "configuration": {
                        "capacity": drift.CAPACITY,
                        "minimum": drift.MINIMUM,
                        "quick": args.quick,
                        "closure": "phase-blind zero-drift mean-field",
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
