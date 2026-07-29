#!/usr/bin/env python3
"""Deterministic framing-size comparison for UCOF-EXP-0001.

This is not a proposal for a replacement wire format. It compares the current
40-byte fixed record header with a deliberately compact strawman that uses an
8-byte fixed prefix followed by unsigned LEB128 lengths and object identifiers.
"""

from __future__ import annotations

from dataclasses import dataclass

FIXED_HEADER_BYTES = 40
COMPACT_FIXED_PREFIX_BYTES = 8


@dataclass(frozen=True)
class Workload:
    name: str
    records: list[tuple[int, int]]


def uleb128_bytes(value: int) -> int:
    if value < 0:
        raise ValueError("ULEB128 input must be non-negative")
    width = 1
    while value >= 0x80:
        value >>= 7
        width += 1
    return width


def compact_header_bytes(object_id: int, payload_bytes: int) -> int:
    return (
        COMPACT_FIXED_PREFIX_BYTES
        + uleb128_bytes(payload_bytes)
        + uleb128_bytes(payload_bytes)
        + uleb128_bytes(object_id)
    )


def workloads() -> list[Workload]:
    return [
        Workload("minimal", [(1, 32), (2, 160)]),
        Workload(
            "small-archive",
            [(index + 1, 256) for index in range(1_000)]
            + [(1_001, 64), (1_002, 64_000)],
        ),
        Workload(
            "table-pages",
            [(index + 1, 65_536) for index in range(10_000)]
            + [(10_001, 128), (10_002, 600_000)],
        ),
        Workload(
            "large-media",
            [(index + 1, 64 * 1024 * 1024) for index in range(1_000)]
            + [(1_001, 256), (1_002, 96_000)],
        ),
    ]


def rows() -> list[tuple[str, int, int, int, float, float]]:
    result = []
    for workload in workloads():
        fixed = FIXED_HEADER_BYTES * len(workload.records)
        compact = sum(
            compact_header_bytes(object_id, payload_bytes)
            for object_id, payload_bytes in workload.records
        )
        payload = sum(payload_bytes for _, payload_bytes in workload.records)
        header_saving = 100.0 * (fixed - compact) / fixed
        file_saving = 100.0 * (fixed - compact) / (fixed + payload)
        result.append(
            (
                workload.name,
                len(workload.records),
                fixed,
                compact,
                header_saving,
                file_saving,
            )
        )
    return result


def main() -> None:
    print(
        "| Workload | Records | Fixed header bytes | Compact candidate bytes | "
        "Header saving | Whole-file saving |"
    )
    print("|---|---:|---:|---:|---:|---:|")
    for name, count, fixed, compact, header_saving, file_saving in rows():
        print(
            f"| {name} | {count:,} | {fixed:,} | {compact:,} | "
            f"{header_saving:.1f}% | {file_saving:.4f}% |"
        )

    expected = {
        "minimal": (80, 24),
        "small-archive": (40_080, 13_901),
        "table-pages": (400_080, 159_903),
        "large-media": (40_080, 17_903),
    }
    for name, _, fixed, compact, _, _ in rows():
        assert (fixed, compact) == expected[name]


if __name__ == "__main__":
    main()
