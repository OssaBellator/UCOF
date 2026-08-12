#!/usr/bin/env python3
"""Adversarial tests for the independent UCOF-EXP-0001 Python parser."""

from __future__ import annotations

import hashlib
import struct
from collections.abc import Callable

from generate_exp_0001_vectors import build
from validate_exp_0001 import (
    CborDecoder,
    FOOTER_LEN,
    HEADER_LEN,
    RECORD_HEADER_LEN,
    FormatError,
    Limits,
    validate,
)


def expect_error(category: str, action: Callable[[], object]) -> None:
    try:
        action()
    except FormatError as error:
        assert error.category == category, (category, error.category, error.detail)
    else:
        raise AssertionError(f"expected {category}")


def mutated(data: bytes, offset: int, replacement: bytes) -> bytes:
    result = bytearray(data)
    result[offset : offset + len(replacement)] = replacement
    return bytes(result)


def reseal(data: bytes) -> bytes:
    result = bytearray(data)
    footer_offset = len(result) - FOOTER_LEN
    result[footer_offset + 48 : footer_offset + 80] = hashlib.sha256(
        result[:footer_offset]
    ).digest()
    return bytes(result)


def test_canonical_cbor() -> None:
    defaults = Limits()
    invalid = [
        b"\x18\x17",
        b"\x9f\xff",
        b"\xa2\x61b\x00\x61a\x00",
        b"\xa2\x61a\x00\x61a\x01",
        b"\x61\xff",
        b"\x20",
        b"\xfa\x00\x00\x00\x00",
    ]
    for encoded in invalid:
        expect_error(
            "non_canonical_metadata",
            lambda encoded=encoded: CborDecoder(encoded, defaults).decode(),
        )

    expect_error(
        "limit_exceeded",
        lambda: CborDecoder(
            b"\x81\x81\x00", Limits(max_metadata_depth=1)
        ).decode(),
    )
    expect_error(
        "limit_exceeded",
        lambda: CborDecoder(
            b"\x82\x00\x00", Limits(max_container_items=2)
        ).decode(),
    )
    expect_error(
        "limit_exceeded",
        lambda: CborDecoder(b"\x62ab", Limits(max_text_bytes=1)).decode(),
    )
    expect_error(
        "limit_exceeded",
        lambda: CborDecoder(b"\x42ab", Limits(max_byte_string_bytes=1)).decode(),
    )
    expect_error(
        "limit_exceeded",
        lambda: CborDecoder(b"\x00", Limits(max_metadata_bytes=0)).decode(),
    )


def test_file_header_fields(base: bytes) -> None:
    for offset in range(8):
        value = bytes([base[offset] ^ 1])
        expect_error(
            "invalid_magic",
            lambda offset=offset, value=value: validate(mutated(base, offset, value)),
        )

    expect_error(
        "unsupported_epoch", lambda: validate(mutated(base, 8, struct.pack("<I", 2)))
    )
    expect_error(
        "unsupported_flags", lambda: validate(mutated(base, 12, struct.pack("<I", 1)))
    )
    expect_error(
        "invalid_length",
        lambda: validate(mutated(base, 16, struct.pack("<I", HEADER_LEN + 1))),
    )
    for offset in range(20, HEADER_LEN):
        expect_error(
            "invalid_reserved",
            lambda offset=offset: validate(mutated(base, offset, b"\x01")),
        )


def test_record_header_fields(base: bytes) -> None:
    offset = HEADER_LEN
    expect_error("invalid_magic", lambda: validate(mutated(base, offset, b"X")))
    expect_error(
        "unsupported_record_kind",
        lambda: validate(mutated(base, offset + 4, struct.pack("<H", 99))),
    )
    expect_error(
        "unsupported_flags",
        lambda: validate(mutated(base, offset + 6, struct.pack("<H", 1))),
    )
    expect_error(
        "invalid_length",
        lambda: validate(
            mutated(base, offset + 8, struct.pack("<I", RECORD_HEADER_LEN + 1))
        ),
    )
    expect_error(
        "invalid_length",
        lambda: validate(mutated(base, offset + 12, struct.pack("<Q", 6))),
    )
    expect_error(
        "invalid_length",
        lambda: validate(mutated(base, offset + 20, struct.pack("<Q", 6))),
    )
    changed_id = reseal(mutated(base, offset + 28, struct.pack("<Q", 9)))
    expect_error("directory_mismatch", lambda: validate(changed_id))
    expect_error(
        "invalid_reserved",
        lambda: validate(mutated(base, offset + 36, struct.pack("<I", 1))),
    )


def test_footer_fields(base: bytes) -> None:
    footer = len(base) - FOOTER_LEN
    for relative in range(8):
        offset = footer + relative
        value = bytes([base[offset] ^ 1])
        expect_error(
            "invalid_magic",
            lambda offset=offset, value=value: validate(mutated(base, offset, value)),
        )

    expect_error(
        "invalid_length",
        lambda: validate(mutated(base, footer + 8, struct.pack("<I", FOOTER_LEN + 1))),
    )
    expect_error(
        "unsupported_flags",
        lambda: validate(mutated(base, footer + 12, struct.pack("<I", 1))),
    )
    expect_error(
        "range_out_of_bounds",
        lambda: validate(mutated(base, footer + 16, struct.pack("<Q", 2**64 - 1))),
    )
    expect_error(
        "range_out_of_bounds",
        lambda: validate(mutated(base, footer + 24, struct.pack("<Q", 2**64 - 1))),
    )
    expect_error(
        "missing_manifest",
        lambda: validate(mutated(base, footer + 32, struct.pack("<Q", 999))),
    )
    expect_error(
        "invalid_length",
        lambda: validate(mutated(base, footer + 40, struct.pack("<Q", 999))),
    )
    for relative in range(48, 80):
        offset = footer + relative
        value = bytes([base[offset] ^ 1])
        expect_error(
            "digest_mismatch",
            lambda offset=offset, value=value: validate(mutated(base, offset, value)),
        )


def test_file_limits(base: bytes) -> None:
    expect_error(
        "limit_exceeded",
        lambda: validate(base, Limits(max_file_bytes=len(base) - 1)),
    )
    expect_error("limit_exceeded", lambda: validate(base, Limits(max_records=1)))
    expect_error(
        "limit_exceeded", lambda: validate(base, Limits(max_payload_bytes=4))
    )


def main() -> None:
    base = build([(1, 1, b"hello")], [1]).data
    validate(base)
    test_canonical_cbor()
    test_file_header_fields(base)
    test_record_header_fields(base)
    test_footer_fields(base)
    test_file_limits(base)
    print("adversarial EXP-0001 tests passed")


if __name__ == "__main__":
    main()
