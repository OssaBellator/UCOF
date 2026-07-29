#!/usr/bin/env python3
"""Compare exact-end and bounded-backward footer discovery costs."""

from __future__ import annotations

from dataclasses import dataclass

FOOTER_MAGIC = b"UCFTR001"
FOOTER_LEN = 80
BACKWARD_BOUND = 65_536


@dataclass(frozen=True)
class Case:
    name: str
    data: bytes


def exact_end_candidates(data: bytes) -> list[int]:
    offset = len(data) - FOOTER_LEN
    if offset >= 0 and data[offset : offset + len(FOOTER_MAGIC)] == FOOTER_MAGIC:
        return [offset]
    return []


def bounded_backward_candidates(data: bytes) -> list[int]:
    start = max(0, len(data) - BACKWARD_BOUND - FOOTER_LEN)
    stop = max(0, len(data) - FOOTER_LEN + 1)
    return [
        offset
        for offset in range(start, stop)
        if data[offset : offset + len(FOOTER_MAGIC)] == FOOTER_MAGIC
    ]


def cases() -> list[Case]:
    valid_footer = FOOTER_MAGIC + bytes(FOOTER_LEN - len(FOOTER_MAGIC))
    base = b"A" * 100 + valid_footer
    dense_tail = FOOTER_MAGIC * (BACKWARD_BOUND // len(FOOTER_MAGIC))
    return [
        Case("exact", base),
        Case("trailing-16", base + b"X" * 16),
        Case("trailing-64KiB", base + b"X" * BACKWARD_BOUND),
        Case("dense-fake-magics", base + dense_tail),
    ]


def main() -> None:
    print(
        "| Case | File bytes | Exact candidates | Backward candidates | "
        "Exact bytes examined | Backward bytes examined |"
    )
    print("|---|---:|---:|---:|---:|---:|")
    observed: dict[str, tuple[int, int]] = {}
    for case in cases():
        exact = exact_end_candidates(case.data)
        backward = bounded_backward_candidates(case.data)
        exact_examined = min(len(case.data), FOOTER_LEN)
        backward_examined = min(len(case.data), BACKWARD_BOUND + FOOTER_LEN)
        observed[case.name] = (len(exact), len(backward))
        print(
            f"| {case.name} | {len(case.data):,} | {len(exact):,} | "
            f"{len(backward):,} | {exact_examined:,} | {backward_examined:,} |"
        )

    assert observed == {
        "exact": (1, 1),
        "trailing-16": (0, 1),
        "trailing-64KiB": (0, 1),
        "dense-fake-magics": (1, 8_184),
    }


if __name__ == "__main__":
    main()
