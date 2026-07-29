#!/usr/bin/env python3
"""Closed-form comparison of initial Phase 3 directory candidates."""

from __future__ import annotations

from dataclasses import dataclass
from math import ceil, log2

PAGE_BYTES = 16 * 1024
LEAF_CAPACITY = 240
INTERNAL_FANOUT = 256
SORTED_ENTRY_BYTES = 64
HASH_EFFECTIVE_BUCKET_CAPACITY = 180
HASH_LOCATORS_PER_PAGE = 1024


@dataclass(frozen=True)
class ModelResult:
    model: str
    entries: int
    directory_bytes: int
    lookup_page_reads: int
    single_update_rewrite_bytes: int
    notes: str


def btree_shape(entries: int) -> tuple[int, int]:
    level_pages = max(1, ceil(entries / LEAF_CAPACITY))
    pages = level_pages
    depth = 1
    while level_pages > 1:
        level_pages = ceil(level_pages / INTERNAL_FANOUT)
        pages += level_pages
        depth += 1
    return pages, depth


def btree(entries: int) -> ModelResult:
    pages, depth = btree_shape(entries)
    return ModelResult(
        model="copy-on-write B+ tree",
        entries=entries,
        directory_bytes=pages * PAGE_BYTES,
        lookup_page_reads=depth,
        single_update_rewrite_bytes=depth * PAGE_BYTES,
        notes="ordered; bounded path; page-local validation",
    )


def sorted_array(entries: int) -> ModelResult:
    probes = 1 if entries <= 1 else ceil(log2(entries))
    total = entries * SORTED_ENTRY_BYTES
    return ModelResult(
        model="monolithic sorted array",
        entries=entries,
        directory_bytes=total,
        lookup_page_reads=probes,
        single_update_rewrite_bytes=total,
        notes="compact; poor append rewrite amplification",
    )


def hash_pages(entries: int) -> ModelResult:
    bucket_pages = max(1, ceil(entries / HASH_EFFECTIVE_BUCKET_CAPACITY))
    locator_pages = max(1, ceil(bucket_pages / HASH_LOCATORS_PER_PAGE))
    total_pages = 1 + locator_pages + bucket_pages
    return ModelResult(
        model="deterministic hash pages",
        entries=entries,
        directory_bytes=total_pages * PAGE_BYTES,
        lookup_page_reads=3,
        single_update_rewrite_bytes=3 * PAGE_BYTES,
        notes="short expected path; collision and ordered-iteration complexity",
    )


def mib(value: int) -> str:
    return f"{value / (1024 * 1024):.2f}"


def main() -> None:
    counts = (1_000, 1_000_000, 100_000_000)
    results = [model(count) for count in counts for model in (btree, sorted_array, hash_pages)]

    print("| Entries | Model | Directory MiB | Lookup page reads | One-update rewrite MiB |")
    print("|---:|---|---:|---:|---:|")
    for result in results:
        print(
            f"| {result.entries:,} | {result.model} | {mib(result.directory_bytes)} | "
            f"{result.lookup_page_reads} | {mib(result.single_update_rewrite_bytes)} |"
        )

    expected = {
        1_000: (6, 2),
        1_000_000: (4_185, 3),
        100_000_000: (418_303, 4),
    }
    for entries, shape in expected.items():
        assert btree_shape(entries) == shape

    assert sorted_array(100_000_000).directory_bytes == 6_400_000_000
    assert hash_pages(100_000_000).lookup_page_reads == 3
    assert btree(100_000_000).single_update_rewrite_bytes == 64 * 1024


if __name__ == "__main__":
    main()
