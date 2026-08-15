#!/usr/bin/env python3
"""Independent fixed-state model for Experiment 0179 directory headroom.

This intentionally models the *intended* acceptance rule, including the known
stale-checkpoint blocker that is not yet promoted as accepted Rust evidence.
It does not import or parse the Rust implementation.
"""

from __future__ import annotations

import argparse


class HeadroomModelError(RuntimeError):
    pass


def validate_transient_checkpoint_headroom(
    *,
    max_entries: int,
    entries: int,
    latest_checkpoint_generation: int | None,
    durable_generation: int,
    unrecognized_entries: int = 0,
) -> None:
    """Validate bounded scan headroom for one checkpoint creation/retry slot.

    At or below the configured limit no checkpoint exception is needed. Exactly
    one entry above the limit is accepted only when there are no unrecognized
    entries and the latest authenticated checkpoint covers current durable
    authority. This prevents a stale checkpoint from lending a slot to newer
    post-checkpoint state and then allowing another checkpoint to reach max+2.
    """
    if max_entries <= 0:
        raise HeadroomModelError("invalid directory entry limit")
    if entries < 0 or unrecognized_entries < 0:
        raise HeadroomModelError("negative directory entry count")
    if unrecognized_entries > entries:
        raise HeadroomModelError("unrecognized entry count exceeds directory entries")
    if entries <= max_entries:
        return
    if entries > max_entries + 1:
        raise HeadroomModelError("directory entry limit")
    if unrecognized_entries:
        raise HeadroomModelError("unrecognized entry cannot borrow checkpoint headroom")
    if latest_checkpoint_generation is None:
        raise HeadroomModelError("transient headroom requires authenticated checkpoint")
    if latest_checkpoint_generation != durable_generation:
        raise HeadroomModelError("stale checkpoint cannot lend transient headroom")


def run_fixed_cases() -> int:
    cases = 0

    validate_transient_checkpoint_headroom(
        max_entries=2,
        entries=2,
        latest_checkpoint_generation=None,
        durable_generation=2,
    )
    cases += 1

    validate_transient_checkpoint_headroom(
        max_entries=2,
        entries=3,
        latest_checkpoint_generation=2,
        durable_generation=2,
    )
    cases += 1

    for kwargs, fragment in [
        (
            dict(
                max_entries=2,
                entries=3,
                latest_checkpoint_generation=None,
                durable_generation=2,
            ),
            "requires authenticated checkpoint",
        ),
        (
            dict(
                max_entries=2,
                entries=3,
                latest_checkpoint_generation=1,
                durable_generation=2,
            ),
            "stale checkpoint",
        ),
        (
            dict(
                max_entries=2,
                entries=3,
                latest_checkpoint_generation=2,
                durable_generation=2,
                unrecognized_entries=1,
            ),
            "unrecognized entry",
        ),
        (
            dict(
                max_entries=2,
                entries=4,
                latest_checkpoint_generation=3,
                durable_generation=3,
            ),
            "directory entry limit",
        ),
    ]:
        try:
            validate_transient_checkpoint_headroom(**kwargs)
        except HeadroomModelError as exc:
            if fragment not in str(exc):
                raise AssertionError(f"expected {fragment!r}, got {exc!r}") from exc
        else:
            raise AssertionError(f"expected HeadroomModelError containing {fragment!r}")
        cases += 1

    return cases


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.parse_args()
    cases = run_fixed_cases()
    print("restart metadata transient-headroom independent model: PASS")
    print(f"fixed_cases={cases}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
