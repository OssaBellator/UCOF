#!/usr/bin/env python3
"""Exact local-state census for the EXP-0003 internal deletion frontier.

This is a non-normative research experiment. It enumerates every ordered pair
of immediate sibling occupancies admitted by the current half-full internal
occupancy geometry when a non-root internal node has just fallen from M to
M-1 after a child merge.

The census is deliberately unweighted. It describes local state-space geometry,
not the stationary frequency of those states under any workload.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum


INTERNAL_MIN_OCCUPANCY = 128
INTERNAL_FANOUT = 255


class Policy(Enum):
    LEFT_FIRST = "left-first"
    FULLER_SIBLING_LEFT_TIE = "fuller-sibling-left-tie"


class Side(Enum):
    LEFT = "left"
    RIGHT = "right"


@dataclass(frozen=True)
class Decision:
    side: Side | None
    donor_occupancy: int | None
    donor_cliff: bool
    local_minimum_count_after_repair: int | None


def choose_side(policy: Policy, left: int, right: int, minimum: int) -> Side | None:
    left_can_lend = left > minimum
    right_can_lend = right > minimum

    if policy is Policy.LEFT_FIRST:
        if left_can_lend:
            return Side.LEFT
        if right_can_lend:
            return Side.RIGHT
        return None

    if left_can_lend and right_can_lend:
        return Side.LEFT if left >= right else Side.RIGHT
    if left_can_lend:
        return Side.LEFT
    if right_can_lend:
        return Side.RIGHT
    return None


def decision(policy: Policy, left: int, right: int) -> Decision:
    minimum = INTERNAL_MIN_OCCUPANCY
    side = choose_side(policy, left, right, minimum)
    donor = left if side is Side.LEFT else right if side is Side.RIGHT else None
    donor_cliff = donor == minimum + 1

    # The underflowed target is repaired to exactly M when borrowing succeeds.
    # In the two-donor domain neither unselected sibling starts at M. Therefore
    # the local post-repair minimum count is one for the repaired target plus
    # one iff the selected donor was M+1 and is drained to M.
    local_minimum_count_after_repair = None
    if donor is not None:
        local_minimum_count_after_repair = 1 + int(donor_cliff)

    return Decision(side, donor, donor_cliff, local_minimum_count_after_repair)


def main() -> None:
    minimum = INTERNAL_MIN_OCCUPANCY
    capacity = INTERNAL_FANOUT
    occupancies = range(minimum, capacity + 1)

    local_pairs = 0
    merge_states = 0
    single_donor_states = 0
    two_donor_states = 0
    policy_divergence_states = 0
    left_cliff_two_donor_states = 0
    fuller_cliff_two_donor_states = 0
    avoidable_left_cliff_states = 0
    minimum_mass_divergence_states = 0

    for left in occupancies:
        for right in occupancies:
            local_pairs += 1
            left_can_lend = left > minimum
            right_can_lend = right > minimum
            eligible = int(left_can_lend) + int(right_can_lend)
            if eligible == 0:
                merge_states += 1
            elif eligible == 1:
                single_donor_states += 1
            else:
                two_donor_states += 1

            left_decision = decision(Policy.LEFT_FIRST, left, right)
            fuller_decision = decision(Policy.FULLER_SIBLING_LEFT_TIE, left, right)

            # Borrower selection cannot change whether a donor exists; it can
            # only change which eligible donor is used. Thus fixed-frontier
            # borrow-vs-merge reward class is policy-neutral.
            assert (left_decision.side is None) == (fuller_decision.side is None)

            if eligible == 2:
                if left_decision.side != fuller_decision.side:
                    policy_divergence_states += 1
                if left_decision.donor_cliff:
                    left_cliff_two_donor_states += 1
                if fuller_decision.donor_cliff:
                    fuller_cliff_two_donor_states += 1
                if (
                    left_decision.donor_cliff
                    and right > left
                    and fuller_decision.side is Side.RIGHT
                ):
                    avoidable_left_cliff_states += 1
                if (
                    left_decision.local_minimum_count_after_repair
                    != fuller_decision.local_minimum_count_after_repair
                ):
                    minimum_mass_divergence_states += 1

    width = capacity - minimum
    assert local_pairs == (width + 1) ** 2
    assert merge_states == 1
    assert single_donor_states == 2 * width
    assert two_donor_states == width**2
    assert policy_divergence_states == width * (width - 1) // 2
    assert left_cliff_two_donor_states == width
    assert fuller_cliff_two_donor_states == 1
    assert avoidable_left_cliff_states == width - 1
    assert minimum_mass_divergence_states == avoidable_left_cliff_states

    divergence_share = policy_divergence_states / two_donor_states
    cliff_share = avoidable_left_cliff_states / two_donor_states
    cliff_given_divergence = avoidable_left_cliff_states / policy_divergence_states

    print("metric,value")
    print(f"internal_minimum,{minimum}")
    print(f"internal_capacity,{capacity}")
    print(f"local_sibling_pairs,{local_pairs}")
    print(f"merge_states,{merge_states}")
    print(f"single_donor_states,{single_donor_states}")
    print(f"two_donor_states,{two_donor_states}")
    print(f"policy_divergence_states,{policy_divergence_states}")
    print(f"policy_divergence_share_two_donor,{divergence_share:.12f}")
    print(f"left_donor_cliff_two_donor_states,{left_cliff_two_donor_states}")
    print(f"fuller_donor_cliff_two_donor_states,{fuller_cliff_two_donor_states}")
    print(f"avoidable_left_donor_cliff_states,{avoidable_left_cliff_states}")
    print(f"avoidable_cliff_share_two_donor,{cliff_share:.12f}")
    print(f"avoidable_cliff_share_policy_divergence,{cliff_given_divergence:.12f}")
    print(f"minimum_mass_divergence_states,{minimum_mass_divergence_states}")
    print("borrow_merge_class_policy_neutral,1")


if __name__ == "__main__":
    main()
