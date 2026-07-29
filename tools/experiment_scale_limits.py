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


def directory_payload_bytes(record_count: int) -> int:
    # Wrapper: one-pair map, encoded "entries" key, and array head.
    total = 9 + cbor_uint_bytes(record_count)
    for object_id in range(1, record_count + 1):
        offset = HEADER_LEN + (object_id - 1) * RECORD_HEADER_LEN
        # Five-pair canonical map with fixed text keys and zero lengths.
        total += 42 + cbor_uint_bytes(object_id) + cbor_uint_bytes(offset)
    return total


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
