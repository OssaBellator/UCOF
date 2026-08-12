#!/usr/bin/env python3
"""Layer-targeted adversarial tests for the provisional EXP-0002 candidate."""

from __future__ import annotations

import hashlib
import struct
from pathlib import Path

from exp0002_codec import (
    COMMIT_DOMAIN,
    FOOTER_LEN,
    INTERNAL_ENTRY_LEN,
    OBJECT_HEADER_LEN,
    PAGE_DOMAIN,
    PAGE_HEADER_LEN,
    PAGE_SIZE,
    SNAPSHOT_DOMAIN,
    Exp0002Error,
    Footer,
    validate_strict,
)

VECTORS = Path(__file__).resolve().parents[1] / "tests" / "vectors" / "exp-0002"


def read_vector(name: str) -> bytearray:
    return bytearray.fromhex((VECTORS / f"{name}.hex").read_text(encoding="ascii"))


def read_u64(data: bytes | bytearray, offset: int) -> int:
    return struct.unpack_from("<Q", data, offset)[0]


def put_u64(data: bytearray, offset: int, value: int) -> None:
    struct.pack_into("<Q", data, offset, value)


def footer_offset(data: bytes | bytearray) -> int:
    return len(data) - FOOTER_LEN


def snapshot_range(data: bytes | bytearray) -> tuple[int, int]:
    footer = footer_offset(data)
    start = read_u64(data, footer + 32)
    length = read_u64(data, footer + 40)
    return start, start + length


def refresh_commit(data: bytearray) -> None:
    footer = footer_offset(data)
    parsed = Footer.parse(bytes(data[footer:]))
    digest = hashlib.sha256(
        COMMIT_DOMAIN
        + bytes(data[parsed.commit_start:footer])
        + bytes(data[footer + 8 : footer + 112])
    ).digest()
    data[footer + 112 : footer + 144] = digest


def refresh_snapshot_and_commit(data: bytearray) -> None:
    footer = footer_offset(data)
    start, end = snapshot_range(data)
    data[footer + 80 : footer + 112] = hashlib.sha256(
        SNAPSHOT_DOMAIN + bytes(data[start:end])
    ).digest()
    refresh_commit(data)


def refresh_root_snapshot_and_commit(data: bytearray) -> None:
    start, _ = snapshot_range(data)
    root_offset = read_u64(data, start + 56)
    root_digest = hashlib.sha256(
        PAGE_DOMAIN + bytes(data[root_offset : root_offset + PAGE_SIZE])
    ).digest()
    data[start + 72 : start + 104] = root_digest
    refresh_snapshot_and_commit(data)


def expect_invalid(name: str, data: bytes | bytearray) -> None:
    try:
        validate_strict(bytes(data))
    except Exp0002Error:
        return
    raise AssertionError(f"adversarial case was accepted: {name}")


def header_cases() -> int:
    count = 0
    data = read_vector("genesis-two-object")
    data[22] = 1
    expect_invalid("file-header-reserved", data)
    count += 1

    data = read_vector("genesis-two-object")
    data[20:22] = b"\xff\xff"
    expect_invalid("unsupported-header-digest", data)
    count += 1

    data = read_vector("genesis-two-object")
    data[0] ^= 1
    expect_invalid("file-magic", data)
    count += 1
    return count


def object_cases() -> int:
    count = 0
    data = read_vector("genesis-two-object")
    first_record = 64
    data[first_record + OBJECT_HEADER_LEN] ^= 1
    refresh_commit(data)
    expect_invalid("object-digest-after-valid-commit", data)
    count += 1

    data = read_vector("genesis-two-object")
    first_record = 64
    payload_len = read_u64(data, first_record + 20)
    put_u64(data, first_record + 28, payload_len + 1)
    refresh_commit(data)
    expect_invalid("object-logical-length", data)
    count += 1

    data = read_vector("genesis-two-object")
    data[first_record + 36] = 1
    refresh_commit(data)
    expect_invalid("object-reserved", data)
    count += 1
    return count


def page_cases() -> int:
    count = 0
    data = read_vector("multi-leaf-400")
    snapshot, _ = snapshot_range(data)
    root = read_u64(data, snapshot + 56)
    data[root + PAGE_HEADER_LEN] ^= 1
    refresh_commit(data)
    expect_invalid("root-page-digest", data)
    count += 1

    data = read_vector("multi-leaf-400")
    snapshot, _ = snapshot_range(data)
    root = read_u64(data, snapshot + 56)
    child = read_u64(data, root + PAGE_HEADER_LEN + 16)
    data[child + PAGE_SIZE - 1] = 1
    child_digest = hashlib.sha256(
        PAGE_DOMAIN + bytes(data[child : child + PAGE_SIZE])
    ).digest()
    data[root + PAGE_HEADER_LEN + 32 : root + PAGE_HEADER_LEN + 64] = child_digest
    refresh_root_snapshot_and_commit(data)
    expect_invalid("authenticated-leaf-padding", data)
    count += 1

    data = read_vector("multi-leaf-400")
    snapshot, _ = snapshot_range(data)
    root = read_u64(data, snapshot + 56)
    child_digest_offset = root + PAGE_HEADER_LEN + 32
    data[child_digest_offset] ^= 1
    refresh_root_snapshot_and_commit(data)
    expect_invalid("child-page-digest", data)
    count += 1

    data = read_vector("multi-leaf-400")
    snapshot, _ = snapshot_range(data)
    root = read_u64(data, snapshot + 56)
    second_entry = root + PAGE_HEADER_LEN + INTERNAL_ENTRY_LEN
    first_max = read_u64(data, root + PAGE_HEADER_LEN + 8)
    put_u64(data, second_entry, first_max)
    refresh_root_snapshot_and_commit(data)
    expect_invalid("overlapping-child-ranges", data)
    count += 1
    return count


def snapshot_cases() -> int:
    count = 0
    data = read_vector("genesis-two-object")
    start, _ = snapshot_range(data)
    data[start + 116] = 1
    refresh_snapshot_and_commit(data)
    expect_invalid("snapshot-reserved", data)
    count += 1

    data = read_vector("append-add-third")
    start, _ = snapshot_range(data)
    data[start + 16] ^= 1
    refresh_snapshot_and_commit(data)
    expect_invalid("parent-snapshot-digest", data)
    count += 1

    data = read_vector("append-add-third")
    start, _ = snapshot_range(data)
    footer = footer_offset(data)
    forward = footer + 1
    put_u64(data, start + 48, forward)
    put_u64(data, footer + 56, forward)
    refresh_snapshot_and_commit(data)
    expect_invalid("forward-previous-footer", data)
    count += 1

    data = read_vector("append-add-third")
    footer = footer_offset(data)
    put_u64(data, footer + 48, read_u64(data, footer + 48) + 1)
    refresh_commit(data)
    expect_invalid("snapshot-footer-sequence-mismatch", data)
    count += 1
    return count


def footer_and_exact_end_cases() -> int:
    count = 0
    data = read_vector("genesis-two-object")
    footer = footer_offset(data)
    data[footer] ^= 1
    expect_invalid("footer-magic", data)
    count += 1

    data = read_vector("genesis-two-object")
    footer = footer_offset(data)
    data[footer + 144] = 1
    expect_invalid("footer-reserved", data)
    count += 1

    data = read_vector("genesis-two-object")
    footer = footer_offset(data)
    data[footer + 112] ^= 1
    expect_invalid("commit-digest", data)
    count += 1

    data = read_vector("genesis-two-object")
    data.extend(b"tail")
    expect_invalid("strict-trailing-bytes", data)
    count += 1

    data = read_vector("append-add-third")
    genesis_len = len(read_vector("genesis-two-object"))
    cuts = {
        genesis_len + 1,
        genesis_len + OBJECT_HEADER_LEN,
        len(data) - FOOTER_LEN - 1,
        len(data) - FOOTER_LEN + 1,
        len(data) - 1,
    }
    for cut in sorted(cuts):
        expect_invalid(f"append-truncation-{cut}", data[:cut])
        count += 1
    return count


def main() -> None:
    total = sum(
        function()
        for function in (
            header_cases,
            object_cases,
            page_cases,
            snapshot_cases,
            footer_and_exact_end_cases,
        )
    )
    print(f"rejected {total} layer-targeted EXP-0002 adversarial cases")


if __name__ == "__main__":
    main()
