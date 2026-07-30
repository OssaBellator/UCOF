#!/usr/bin/env python3
"""Measure complete-snapshot checkpoint cadence for Candidate 1."""

from __future__ import annotations

from dataclasses import dataclass
from math import ceil

PAGE_BYTES = 16 * 1024
PAGE_HEADER_BYTES = 64
LEAF_ENTRY_BYTES = 88
INTERNAL_ENTRY_BYTES = 64
LEAF_CAPACITY = (PAGE_BYTES - PAGE_HEADER_BYTES) // LEAF_ENTRY_BYTES
INTERNAL_FANOUT = (PAGE_BYTES - PAGE_HEADER_BYTES) // INTERNAL_ENTRY_BYTES
SNAPSHOT_BYTES_ONE_ROOT = 160 + 8
FOOTER_BYTES = 160
TOTAL_OBJECTS = 1_000_000
PAYLOAD_BYTES_PER_OBJECT = 4 * 1024
CADENCES = (100, 1_000, 10_000, 100_000)


@dataclass(frozen=True)
class DirectoryShape:
    pages: int
    depth: int

    @property
    def bytes(self) -> int:
        return self.pages * PAGE_BYTES


def directory_shape(objects: int) -> DirectoryShape:
    leaves = max(1, ceil(objects / LEAF_CAPACITY))
    pages = leaves
    depth = 1
    level = leaves
    while level > 1:
        level = ceil(level / INTERNAL_FANOUT)
        pages += level
        depth += 1
    return DirectoryShape(pages, depth)


def checkpoint_counts(cadence: int) -> list[int]:
    return [
        min(index * cadence, TOTAL_OBJECTS)
        for index in range(1, ceil(TOTAL_OBJECTS / cadence) + 1)
    ]


def full_rebuild_bytes(cadence: int) -> int:
    per_checkpoint_fixed = SNAPSHOT_BYTES_ONE_ROOT + FOOTER_BYTES
    return sum(
        directory_shape(objects).bytes + per_checkpoint_fixed
        for objects in checkpoint_counts(cadence)
    )


def conservative_path_copy_bytes(cadence: int) -> int:
    """Upper model: each inserted object copies one path; splits are excluded."""

    fixed = SNAPSHOT_BYTES_ONE_ROOT + FOOTER_BYTES
    total = 0
    previous = 0
    for objects in checkpoint_counts(cadence):
        inserted = objects - previous
        total += inserted * directory_shape(objects).depth * PAGE_BYTES + fixed
        previous = objects
    return total


def human_bytes(value: int) -> str:
    units = ("B", "KiB", "MiB", "GiB", "TiB")
    current = float(value)
    for unit in units:
        if current < 1024 or unit == units[-1]:
            return f"{current:.2f} {unit}"
        current /= 1024
    raise AssertionError("unreachable")


def main() -> None:
    print(
        "| Objects/checkpoint | Checkpoints | Max unpublished payload | "
        "Cumulative full-rebuild metadata | Conservative path-copy metadata |"
    )
    print("|---:|---:|---:|---:|---:|")
    rows = []
    for cadence in CADENCES:
        checkpoints = len(checkpoint_counts(cadence))
        lost_work = cadence * PAYLOAD_BYTES_PER_OBJECT
        rebuilt = full_rebuild_bytes(cadence)
        copied = conservative_path_copy_bytes(cadence)
        rows.append((cadence, checkpoints, lost_work, rebuilt, copied))
        print(
            f"| {cadence:,} | {checkpoints:,} | {human_bytes(lost_work)} | "
            f"{human_bytes(rebuilt)} | {human_bytes(copied)} |"
        )

    assert LEAF_CAPACITY == 185
    assert INTERNAL_FANOUT == 255
    assert directory_shape(1_000_000) == DirectoryShape(5_429, 3)
    assert rows[0][1] == 10_000
    assert rows[-1][1] == 10
    assert rows[0][2] == 400 * 1024
    assert rows[-1][2] == 400_000 * 1024

    # Frequent checkpoints make repeated full rebuilds dominant. Once checkpoints
    # become sparse, naive per-object path copying becomes more expensive because
    # it preserves every intermediate path rather than sharing work within a batch.
    assert rows[0][3] > rows[0][4]
    for _, _, _, rebuilt, copied in rows[1:]:
        assert rebuilt < copied

    assert rows[0][3] > rows[1][3] > rows[2][3] > rows[3][3]
    assert rows[0][4] <= rows[1][4] <= rows[2][4] <= rows[3][4]


if __name__ == "__main__":
    main()
