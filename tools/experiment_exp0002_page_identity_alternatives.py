#!/usr/bin/env python3
"""Compare page identity semantics after Candidate 1 reuse failure."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from math import ceil

PAGE_BYTES = 16 * 1024
PAGE_HEADER_BYTES = 64
LEAF_ENTRY_BYTES = 88
INTERNAL_ENTRY_BYTES = 64
OBJECTS = 100_000_000
UPDATE_COUNTS = (1, 100, 10_000, 1_000_000)
DOMAIN = b"UCOF-EXP-0002-PAGE-IDENTITY-EXPERIMENT\x00"


@dataclass(frozen=True)
class TreeShape:
    level_counts: tuple[int, ...]  # leaves first, root last

    @property
    def depth(self) -> int:
        return len(self.level_counts)

    @property
    def total_pages(self) -> int:
        return sum(self.level_counts)

    @property
    def full_rebuild_bytes(self) -> int:
        return self.total_pages * PAGE_BYTES


@dataclass(frozen=True)
class IdentityAlternative:
    name: str
    page_field: str
    validation_rule: str
    exact_historical_reuse: bool
    identical_content_dedup: bool
    binds_page_age: bool
    prevents_whole_file_rollback: bool


def shape(objects: int) -> TreeShape:
    leaf_capacity = (PAGE_BYTES - PAGE_HEADER_BYTES) // LEAF_ENTRY_BYTES
    internal_fanout = (PAGE_BYTES - PAGE_HEADER_BYTES) // INTERNAL_ENTRY_BYTES
    levels = [max(1, ceil(objects / leaf_capacity))]
    while levels[-1] > 1:
        levels.append(ceil(levels[-1] / internal_fanout))
    return TreeShape(tuple(levels))


def selected_leaf_indices(leaf_pages: int, changed_objects: int) -> set[int]:
    changed_leaves = min(leaf_pages, changed_objects)
    if changed_leaves == leaf_pages:
        return set(range(leaf_pages))
    # Deterministically spread updates across the tree to avoid understating
    # ancestor fan-out while remaining reproducible.
    return {
        min(leaf_pages - 1, (index * leaf_pages) // changed_leaves)
        for index in range(changed_leaves)
    }


def batched_path_pages(tree: TreeShape, changed_objects: int) -> int:
    internal_fanout = (PAGE_BYTES - PAGE_HEADER_BYTES) // INTERNAL_ENTRY_BYTES
    current = selected_leaf_indices(tree.level_counts[0], changed_objects)
    total = len(current)
    for level_count in tree.level_counts[1:]:
        current = {min(level_count - 1, index // internal_fanout) for index in current}
        total += len(current)
    return total


def page_digest(payload: bytes, sequence_field: int | None) -> bytes:
    field = b"" if sequence_field is None else sequence_field.to_bytes(8, "little")
    return hashlib.sha256(DOMAIN + field + payload).digest()


def semantic_checks() -> None:
    payload = b"canonical unchanged page contents"
    old_sequence = 7
    active_sequence = 8

    candidate1_old = page_digest(payload, old_sequence)
    candidate1_new = page_digest(payload, active_sequence)
    assert candidate1_old != candidate1_new
    assert old_sequence != active_sequence  # Candidate 1 equality rejects reuse.

    birth_old = page_digest(payload, old_sequence)
    assert old_sequence <= active_sequence  # A birth rule permits reuse.
    assert birth_old == page_digest(payload, old_sequence)
    assert birth_old != page_digest(payload, active_sequence)

    immutable_old = page_digest(payload, None)
    immutable_later = page_digest(payload, None)
    assert immutable_old == immutable_later

    mutated = page_digest(payload + b"!", None)
    assert mutated != immutable_old


def main() -> None:
    tree = shape(OBJECTS)
    alternatives = (
        IdentityAlternative(
            "active snapshot sequence",
            "u64 active_sequence",
            "page.sequence == snapshot.sequence",
            False,
            False,
            True,
            False,
        ),
        IdentityAlternative(
            "page birth sequence",
            "u64 birth_sequence",
            "page.birth_sequence <= snapshot.sequence",
            True,
            False,
            True,
            False,
        ),
        IdentityAlternative(
            "immutable content identity",
            "reserved zero / no sequence",
            "page digest and authenticated parent/root membership",
            True,
            True,
            False,
            False,
        ),
    )

    semantic_checks()

    assert tree.level_counts == (540_541, 2_120, 9, 1)
    assert tree.total_pages == 542_671
    assert tree.depth == 4

    print("| Alternative | Page field | Validation rule | Exact reuse | Content dedup | Binds age | Rollback protection |")
    print("|---|---|---|---|---|---|---|")
    for alternative in alternatives:
        print(
            f"| {alternative.name} | {alternative.page_field} | "
            f"{alternative.validation_rule} | "
            f"{'yes' if alternative.exact_historical_reuse else 'no'} | "
            f"{'yes' if alternative.identical_content_dedup else 'no'} | "
            f"{'yes' if alternative.binds_page_age else 'no'} | "
            f"{'yes' if alternative.prevents_whole_file_rollback else 'no'} |"
        )

    print()
    print("| Changed objects | Candidate 1 pages | Batched reusable pages | Candidate 1 bytes | Reusable bytes | Amplification |")
    print("|---:|---:|---:|---:|---:|---:|")
    for changed_objects in UPDATE_COUNTS:
        reusable_pages = batched_path_pages(tree, changed_objects)
        reusable_bytes = reusable_pages * PAGE_BYTES
        amplification = tree.full_rebuild_bytes / reusable_bytes
        print(
            f"| {changed_objects:,} | {tree.total_pages:,} | {reusable_pages:,} | "
            f"{tree.full_rebuild_bytes:,} | {reusable_bytes:,} | {amplification:.2f}x |"
        )

    single_update_pages = batched_path_pages(tree, 1)
    assert single_update_pages == tree.depth
    assert tree.full_rebuild_bytes == 8_891_121_664
    assert tree.full_rebuild_bytes // (single_update_pages * PAGE_BYTES) == 135_668

    all_update_pages = batched_path_pages(tree, OBJECTS)
    assert all_update_pages == tree.total_pages

    print()
    print(f"tree_level_counts={tree.level_counts}")
    print(f"single_update_reusable_pages={single_update_pages}")
    print("finding=active snapshot sequence binding is incompatible with exact page reuse")
    print("finding=page birth sequence permits reuse but prevents identical-content dedup across births")
    print("finding=immutable content identity permits reuse and dedup; snapshot/root authentication supplies membership")
    print("finding=no page identity alternative provides external whole-file freshness")


if __name__ == "__main__":
    main()
