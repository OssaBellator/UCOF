#!/usr/bin/env python3
"""Measure when mirrored locator metadata beats object-header reads."""

from __future__ import annotations

from dataclasses import dataclass
from math import ceil

PAGE_BYTES = 16 * 1024
PAGE_HEADER_BYTES = 64
INTERNAL_ENTRY_BYTES = 64
INTERNAL_FANOUT = (PAGE_BYTES - PAGE_HEADER_BYTES) // INTERNAL_ENTRY_BYTES
OBJECT_HEADER_BYTES = 48
OBJECTS = 100_000_000
FRACTIONS = (0.0, 0.1, 1 / 3, 0.5, 2 / 3, 1.0)


@dataclass(frozen=True)
class Variant:
    name: str
    entry_bytes: int
    identifier_bits: int
    mirrors_inventory_fields: bool


VARIANTS = (
    Variant("Candidate 1 baseline", 88, 64, True),
    Variant("Tight same fields", 72, 64, True),
    Variant("Minimal authenticated", 56, 64, False),
    Variant("Minimal authenticated 128-bit ID", 64, 128, False),
    Variant("Baseline fields 128-bit ID", 96, 128, True),
)


def directory_bytes(objects: int, entry_bytes: int) -> int:
    capacity = (PAGE_BYTES - PAGE_HEADER_BYTES) // entry_bytes
    level = ceil(objects / capacity)
    pages = level
    while level > 1:
        level = ceil(level / INTERNAL_FANOUT)
        pages += level
    return pages * PAGE_BYTES


def inventory_transfer(variant: Variant, fraction: float) -> int:
    base = directory_bytes(OBJECTS, variant.entry_bytes)
    if variant.mirrors_inventory_fields:
        return base
    headers = ceil(OBJECTS * fraction)
    return base + headers * OBJECT_HEADER_BYTES


def crossover(mirrored: Variant, minimal: Variant) -> float:
    saved_directory_bytes = directory_bytes(OBJECTS, mirrored.entry_bytes) - directory_bytes(
        OBJECTS, minimal.entry_bytes
    )
    return saved_directory_bytes / (OBJECTS * OBJECT_HEADER_BYTES)


def gib(value: int) -> str:
    return f"{value / (1024 ** 3):.3f}"


def main() -> None:
    baseline, tight, minimal, minimal_128, baseline_128 = VARIANTS
    tight_cross = crossover(tight, minimal)
    baseline_cross = crossover(baseline, minimal)
    baseline_128_cross = crossover(baseline_128, minimal_128)

    assert 0.338 < tight_cross < 0.339
    assert 0.674 < baseline_cross < 0.675
    assert 0.671 < baseline_128_cross < 0.672

    print("| Variant | Entry bytes | ID bits | Directory GiB | Mirrored inventory fields |")
    print("|---|---:|---:|---:|---|")
    for variant in VARIANTS:
        print(
            f"| {variant.name} | {variant.entry_bytes} | {variant.identifier_bits} | "
            f"{gib(directory_bytes(OBJECTS, variant.entry_bytes))} | "
            f"{'yes' if variant.mirrors_inventory_fields else 'no'} |"
        )

    print()
    print("| Inventory fraction | Tight 72-byte GiB | Minimal 56-byte GiB | Minimal extra header requests | Winner by bytes |")
    print("|---:|---:|---:|---:|---|")
    for fraction in FRACTIONS:
        tight_bytes = inventory_transfer(tight, fraction)
        minimal_bytes = inventory_transfer(minimal, fraction)
        requests = ceil(OBJECTS * fraction)
        winner = "minimal" if minimal_bytes < tight_bytes else "tight mirrored"
        print(
            f"| {fraction:.3f} | {gib(tight_bytes)} | {gib(minimal_bytes)} | "
            f"{requests:,} | {winner} |"
        )

    assert inventory_transfer(minimal, 0.0) < inventory_transfer(tight, 0.0)
    assert inventory_transfer(minimal, 1 / 3) < inventory_transfer(tight, 1 / 3)
    assert inventory_transfer(minimal, 0.5) > inventory_transfer(tight, 0.5)
    assert inventory_transfer(minimal, 1.0) > inventory_transfer(tight, 1.0)

    print()
    print(f"tight_vs_minimal_crossover_fraction={tight_cross:.6f}")
    print(f"baseline_vs_minimal_crossover_fraction={baseline_cross:.6f}")
    print(f"baseline128_vs_minimal128_crossover_fraction={baseline_128_cross:.6f}")
    print(f"minimal_full_inventory_header_bytes={OBJECTS * OBJECT_HEADER_BYTES:,}")
    print(f"minimal_full_inventory_header_requests_worst_case={OBJECTS:,}")
    print("finding=removing reserved bytes is unconditionally beneficial for the compared fields")
    print("finding=56-byte locators win transfer below roughly one-third metadata inventory coverage versus tight 72-byte locators")
    print("finding=mirrored kind and logical length win total bytes above that crossover and avoid per-object range requests")
    print("finding=identifier width and metadata mirroring are separable decisions")


if __name__ == "__main__":
    main()
