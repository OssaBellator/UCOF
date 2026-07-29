#!/usr/bin/env python3
"""Model lower-bound EXP-0001 metadata costs at UC-02 object counts."""

from __future__ import annotations

HEADER_LEN = 32
RECORD_HEADER_LEN = 40
FOOTER_LEN = 80


def cbor_uint_bytes(value: int) -> int:
    if value <= 23:
        return 1
    if value <= 0xFF:
        return 2
    if value <= 0xFFFF:
        return 3
    if value <= 0xFFFF_FFFF:
        return 5
    return 9


def sum_cbor_widths(count: int, start: int, step: int) -> int:
    if count < 0 or start < 0 or step <= 0:
        raise ValueError("invalid arithmetic sequence")

    def values_at_most(limit: int) -> int:
        if count == 0 or start > limit:
            return 0
        return min(count, (limit - start) // step + 1)

    through_23 = values_at_most(23)
    through_255 = values_at_most(0xFF)
    through_65_535 = values_at_most(0xFFFF)
    through_u32 = values_at_most(0xFFFF_FFFF)

    return (
        through_23
        + (through_255 - through_23) * 2
        + (through_65_535 - through_255) * 3
        + (through_u32 - through_65_535) * 5
        + (count - through_u32) * 9
    )


def directory_payload_bytes(record_count: int) -> int:
    # Wrapper: one-pair map, encoded "entries" key, and array head.
    wrapper = 9 + cbor_uint_bytes(record_count)
    fixed_entry_bytes = record_count * 42
    identifier_bytes = sum_cbor_widths(record_count, start=1, step=1)
    offset_bytes = sum_cbor_widths(
        record_count, start=HEADER_LEN, step=RECORD_HEADER_LEN
    )
    return wrapper + fixed_entry_bytes + identifier_bytes + offset_bytes


def lower_bound_file_bytes(record_count: int) -> tuple[int, int, int]:
    headers = record_count * RECORD_HEADER_LEN
    directory = directory_payload_bytes(record_count)
    total = HEADER_LEN + headers + directory + FOOTER_LEN
    return headers, directory, total


def main() -> None:
    print(
        "| Logical objects | Record header bytes | Directory payload bytes | "
        "Lower-bound file bytes |"
    )
    print("|---:|---:|---:|---:|")
    expected = {
        1: (40, 55, 207),
        1_000: (40_000, 47_728, 87_840),
        1_000_000: (40_000_000, 51_865_384, 91_865_496),
        100_000_000: (4_000_000_000, 5_199_865_384, 9_199_865_496),
    }
    for count in expected:
        result = lower_bound_file_bytes(count)
        assert result == expected[count]
        headers, directory, total = result
        print(f"| {count:,} | {headers:,} | {directory:,} | {total:,} |")


if __name__ == "__main__":
    main()
