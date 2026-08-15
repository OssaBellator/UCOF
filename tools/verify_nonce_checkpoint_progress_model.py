#!/usr/bin/env python3
"""Independent checkpoint generation/counter progress model for Experiment 0179.

This does not parse or call Rust. It captures the minimum progress implied by
non-empty nonce leases: every committed generation consumes at least one nonce,
and no generation can be committed after nonce authority is exhausted.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional


class ProgressError(RuntimeError):
    pass


@dataclass(frozen=True)
class Checkpoint:
    generation: int
    next_unreserved: Optional[int]


def validate_checkpoint(checkpoint: Checkpoint) -> None:
    if checkpoint.generation <= 0:
        raise ProgressError("generation")
    if checkpoint.next_unreserved is not None:
        if checkpoint.next_unreserved < 0:
            raise ProgressError("counter")
        if checkpoint.next_unreserved < checkpoint.generation:
            raise ProgressError("absolute generation/counter progress")


def validate_transition(previous: Checkpoint, current: Checkpoint) -> None:
    validate_checkpoint(previous)
    validate_checkpoint(current)
    if current.generation <= previous.generation:
        raise ProgressError("generation rollback")

    generation_delta = current.generation - previous.generation
    if previous.next_unreserved is None:
        raise ProgressError("generation after exhaustion")
    if current.next_unreserved is None:
        # Reaching exhaustion is forward nonce progress. Detailed feasibility
        # also depends on remaining counter space / configured lease bounds;
        # this model only captures the minimum non-empty-lease invariant.
        return

    if current.next_unreserved < previous.next_unreserved:
        raise ProgressError("counter rollback")
    counter_delta = current.next_unreserved - previous.next_unreserved
    if counter_delta < generation_delta:
        raise ProgressError("insufficient counter progress")


def expect_error(fragment: str, fn) -> None:
    try:
        fn()
    except ProgressError as exc:
        if fragment not in str(exc):
            raise AssertionError(f"expected {fragment!r}, got {exc!r}") from exc
    else:
        raise AssertionError(f"expected ProgressError containing {fragment!r}")


def run_all() -> dict[str, int]:
    validate_checkpoint(Checkpoint(1, 1))
    validate_checkpoint(Checkpoint(2, None))
    validate_transition(Checkpoint(1, 5), Checkpoint(2, 12))
    validate_transition(Checkpoint(2, 12), Checkpoint(5, 15))
    validate_transition(Checkpoint(2, 12), Checkpoint(3, None))

    expect_error(
        "absolute generation/counter progress",
        lambda: validate_checkpoint(Checkpoint(5, 4)),
    )
    expect_error(
        "insufficient counter progress",
        lambda: validate_transition(Checkpoint(2, 100), Checkpoint(5, 102)),
    )
    expect_error(
        "generation after exhaustion",
        lambda: validate_transition(Checkpoint(2, None), Checkpoint(3, None)),
    )
    expect_error(
        "generation after exhaustion",
        lambda: validate_transition(Checkpoint(2, None), Checkpoint(3, 200)),
    )
    expect_error(
        "counter rollback",
        lambda: validate_transition(Checkpoint(2, 100), Checkpoint(3, 99)),
    )
    return {"valid_cases": 5, "rejected_cases": 5}


def main() -> int:
    summary = run_all()
    print("nonce checkpoint progress independent model: PASS")
    for key, value in summary.items():
        print(f"{key}={value}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
