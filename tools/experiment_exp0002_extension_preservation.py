#!/usr/bin/env python3
"""Canonical required/optional extension block with preservation semantics."""

from __future__ import annotations

import struct
from dataclasses import dataclass

MAGIC = b"UCEX0002"
HEADER = struct.Struct("<8sII")
RECORD = struct.Struct("<HHI")
ALIGNMENT = 8
FLAG_REQUIRED = 1
KNOWN_TAGS = {1}
MAX_EXTENSION_BYTES = 64 * 1024
MAX_RECORDS = 1024


class ExtensionError(ValueError):
    pass


class UnknownRequired(ExtensionError):
    pass


@dataclass(frozen=True)
class Extension:
    tag: int
    required: bool
    payload: bytes


@dataclass(frozen=True)
class ParsedExtensions:
    records: tuple[Extension, ...]
    known: dict[int, bytes]
    unknown_optional: tuple[Extension, ...]


def aligned(value: int) -> int:
    return (value + ALIGNMENT - 1) // ALIGNMENT * ALIGNMENT


def encode_extensions(records: list[Extension]) -> bytes:
    ordered = sorted(records, key=lambda record: record.tag)
    if ordered != records:
        raise ExtensionError("extension records must already be sorted")
    if any(record.tag == 0 for record in records):
        raise ExtensionError("zero extension tag")
    if any(left.tag >= right.tag for left, right in zip(records, records[1:])):
        raise ExtensionError("duplicate or unordered extension tag")
    if len(records) > MAX_RECORDS:
        raise ExtensionError("extension record limit")

    body = bytearray()
    for record in records:
        flags = FLAG_REQUIRED if record.required else 0
        body.extend(RECORD.pack(record.tag, flags, len(record.payload)))
        body.extend(record.payload)
        body.extend(bytes(aligned(len(record.payload)) - len(record.payload)))

    total = HEADER.size + len(body)
    if total > MAX_EXTENSION_BYTES:
        raise ExtensionError("extension byte limit")
    return HEADER.pack(MAGIC, len(records), total) + body


def parse_extensions(data: bytes) -> ParsedExtensions:
    if len(data) < HEADER.size or len(data) > MAX_EXTENSION_BYTES:
        raise ExtensionError("extension byte limit")
    magic, count, total = HEADER.unpack_from(data)
    if magic != MAGIC or total != len(data) or count > MAX_RECORDS:
        raise ExtensionError("extension header")

    cursor = HEADER.size
    records: list[Extension] = []
    previous = 0
    known: dict[int, bytes] = {}
    unknown: list[Extension] = []
    for _ in range(count):
        if cursor + RECORD.size > len(data):
            raise ExtensionError("truncated extension record")
        tag, flags, payload_len = RECORD.unpack_from(data, cursor)
        cursor += RECORD.size
        if tag == 0 or tag <= previous:
            raise ExtensionError("duplicate or unordered extension tag")
        if flags & ~FLAG_REQUIRED:
            raise ExtensionError("unknown extension flags")
        payload_end = cursor + payload_len
        padded_end = cursor + aligned(payload_len)
        if padded_end > len(data):
            raise ExtensionError("truncated extension payload")
        payload = data[cursor:payload_end]
        if any(data[payload_end:padded_end]):
            raise ExtensionError("non-zero extension padding")
        required = bool(flags & FLAG_REQUIRED)
        record = Extension(tag, required, payload)
        records.append(record)
        if tag in KNOWN_TAGS:
            known[tag] = payload
        elif required:
            raise UnknownRequired(f"unknown required extension {tag}")
        else:
            unknown.append(record)
        previous = tag
        cursor = padded_end

    if cursor != len(data):
        raise ExtensionError("trailing extension bytes")
    return ParsedExtensions(tuple(records), known, tuple(unknown))


def rewrite_known(data: bytes, updates: dict[int, bytes]) -> bytes:
    parsed = parse_extensions(data)
    if any(tag not in KNOWN_TAGS for tag in updates):
        raise ExtensionError("cannot rewrite unknown extension")
    rewritten = [
        Extension(record.tag, record.required, updates.get(record.tag, record.payload))
        for record in parsed.records
    ]
    return encode_extensions(rewritten)


def record_bytes(data: bytes, wanted_tag: int) -> bytes:
    _magic, count, _total = HEADER.unpack_from(data)
    cursor = HEADER.size
    for _ in range(count):
        start = cursor
        tag, _flags, payload_len = RECORD.unpack_from(data, cursor)
        cursor += RECORD.size + aligned(payload_len)
        if tag == wanted_tag:
            return data[start:cursor]
    raise KeyError(wanted_tag)


def expect_error(data: bytes, error_type: type[Exception]) -> None:
    try:
        parse_extensions(data)
    except error_type:
        return
    raise AssertionError(f"expected {error_type.__name__}")


def main() -> None:
    original = encode_extensions(
        [
            Extension(1, True, b"known-v1"),
            Extension(100, False, b"opaque future metadata"),
            Extension(200, False, b"\x00\x01\xfe\xff"),
        ]
    )
    parsed = parse_extensions(original)
    assert parsed.known == {1: b"known-v1"}
    assert [record.tag for record in parsed.unknown_optional] == [100, 200]
    assert encode_extensions(list(parsed.records)) == original

    rewritten = rewrite_known(original, {1: b"known-v2-expanded"})
    rewritten_parsed = parse_extensions(rewritten)
    assert rewritten_parsed.known == {1: b"known-v2-expanded"}
    assert record_bytes(rewritten, 100) == record_bytes(original, 100)
    assert record_bytes(rewritten, 200) == record_bytes(original, 200)

    unknown_required = encode_extensions([Extension(101, True, b"future required")])
    expect_error(unknown_required, UnknownRequired)

    duplicate = bytearray(original)
    second_tag_offset = HEADER.size + RECORD.size + aligned(len(b"known-v1"))
    struct.pack_into("<H", duplicate, second_tag_offset, 1)
    expect_error(bytes(duplicate), ExtensionError)

    unordered = bytearray(original)
    struct.pack_into("<H", unordered, second_tag_offset, 0)
    expect_error(bytes(unordered), ExtensionError)

    bad_padding = bytearray(encode_extensions([Extension(1, True, b"x")]))
    bad_padding[HEADER.size + RECORD.size + 1] = 1
    expect_error(bytes(bad_padding), ExtensionError)

    bad_flags = bytearray(original)
    struct.pack_into("<H", bad_flags, HEADER.size + 2, 2)
    expect_error(bytes(bad_flags), ExtensionError)

    trailing = original + b"\x00"
    expect_error(trailing, ExtensionError)

    print(f"canonical_block_bytes={len(original)}")
    print(f"rewritten_block_bytes={len(rewritten)}")
    print("known_required_parse=pass")
    print("unknown_optional_preservation=pass")
    print("unknown_required_rejection=pass")
    print("duplicate_and_order_rejection=pass")
    print("padding_and_flag_rejection=pass")
    print("resource_bounds=pass")
    print("finding=reserved-zero bytes are not an unknown-field preservation mechanism")
    print("finding=canonical length-delimited optional records can be copied without interpretation")
    print("finding=required criticality belongs on each extension record and fails closed")
    print("finding=rewrite tools must preserve unknown optional records unless policy explicitly drops them")


if __name__ == "__main__":
    main()
