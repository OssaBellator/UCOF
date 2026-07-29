#!/usr/bin/env python3
"""Generate the UCOF-EXP-0001 conformance vectors using only Python stdlib.

This generator is intentionally independent from the Rust implementation. It
writes binary files, hexadecimal text copies, and JSON expectations.
"""

from __future__ import annotations

import hashlib
import json
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Any

FILE_MAGIC = b"UCOF\r\n\x1a\n"
RECORD_MAGIC = b"UCRD"
FOOTER_MAGIC = b"UCFTR001"
HEADER_LEN = 32
RECORD_HEADER_LEN = 40
FOOTER_LEN = 80


def cbor_head(major: int, argument: int) -> bytes:
    prefix = major << 5
    if argument < 24:
        return bytes([prefix | argument])
    if argument <= 0xFF:
        return bytes([prefix | 24, argument])
    if argument <= 0xFFFF:
        return bytes([prefix | 25]) + struct.pack(">H", argument)
    if argument <= 0xFFFF_FFFF:
        return bytes([prefix | 26]) + struct.pack(">I", argument)
    return bytes([prefix | 27]) + struct.pack(">Q", argument)


def encode_cbor(value: Any) -> bytes:
    if value is False:
        return b"\xf4"
    if value is True:
        return b"\xf5"
    if value is None:
        return b"\xf6"
    if isinstance(value, int):
        if value < 0:
            raise ValueError("negative integers are outside EXP-0001")
        return cbor_head(0, value)
    if isinstance(value, bytes):
        return cbor_head(2, len(value)) + value
    if isinstance(value, str):
        encoded = value.encode("utf-8")
        return cbor_head(3, len(encoded)) + encoded
    if isinstance(value, list):
        return cbor_head(4, len(value)) + b"".join(encode_cbor(item) for item in value)
    if isinstance(value, dict):
        pairs = [(encode_cbor(key), encode_cbor(item)) for key, item in value.items()]
        pairs.sort(key=lambda pair: (len(pair[0]), pair[0]))
        return cbor_head(5, len(pairs)) + b"".join(key + item for key, item in pairs)
    raise TypeError(f"unsupported value {type(value)!r}")


def file_header() -> bytes:
    return FILE_MAGIC + struct.pack("<III", 1, 0, HEADER_LEN) + bytes(12)


def record(kind: int, object_id: int, payload: bytes) -> bytes:
    header = RECORD_MAGIC + struct.pack(
        "<HHIQQQI",
        kind,
        0,
        RECORD_HEADER_LEN,
        len(payload),
        len(payload),
        object_id,
        0,
    )
    assert len(header) == RECORD_HEADER_LEN
    return header + payload


@dataclass(frozen=True)
class BuiltFile:
    data: bytes
    manifest_id: int
    entries: list[dict[str, int]]


def build(
    objects: list[tuple[int, int, bytes]],
    roots: list[int],
    required: list[int] | None = None,
    optional: list[int] | None = None,
) -> BuiltFile:
    required = required or []
    optional = optional or []
    data = bytearray(file_header())
    entries: list[dict[str, int]] = []

    for kind, object_id, payload in objects:
        offset = len(data)
        data.extend(record(kind, object_id, payload))
        entries.append(
            {
                "id": object_id,
                "kind": kind,
                "offset": offset,
                "stored_len": len(payload),
                "logical_len": len(payload),
            }
        )

    manifest_id = max([object_id for _, object_id, _ in objects] + [0]) + 1
    manifest_payload = encode_cbor(
        {"roots": roots, "required": required, "optional": optional}
    )
    manifest_offset = len(data)
    data.extend(record(2, manifest_id, manifest_payload))
    entries.append(
        {
            "id": manifest_id,
            "kind": 2,
            "offset": manifest_offset,
            "stored_len": len(manifest_payload),
            "logical_len": len(manifest_payload),
        }
    )

    directory_payload = encode_cbor({"entries": entries})
    directory_offset = len(data)
    directory_record = record(3, 0, directory_payload)
    data.extend(directory_record)

    digest = hashlib.sha256(data).digest()
    footer = FOOTER_MAGIC + struct.pack(
        "<IIQQQQ",
        FOOTER_LEN,
        0,
        directory_offset,
        len(directory_record),
        manifest_id,
        len(entries) + 1,
    ) + digest
    assert len(footer) == FOOTER_LEN
    data.extend(footer)
    return BuiltFile(bytes(data), manifest_id, entries)


def write_vector(output: Path, name: str, data: bytes, expectation: dict[str, Any]) -> None:
    output.mkdir(parents=True, exist_ok=True)
    (output / f"{name}.ucof").write_bytes(data)
    hexadecimal = data.hex()
    lines = [hexadecimal[index : index + 64] for index in range(0, len(hexadecimal), 64)]
    (output / f"{name}.hex").write_text("\n".join(lines) + "\n", encoding="ascii")
    (output / f"{name}.json").write_text(
        json.dumps(expectation, indent=2) + "\n", encoding="utf-8"
    )


def main() -> None:
    output = Path(__file__).resolve().parents[1] / "tests" / "vectors" / "exp-0001"

    minimal = build([], [])
    write_vector(
        output,
        "minimal-valid",
        minimal.data,
        {
            "length": len(minimal.data),
            "manifest_id": minimal.manifest_id,
            "sha256": hashlib.sha256(minimal.data).hexdigest(),
            "expected": "valid",
        },
    )

    two_objects = build([(1, 1, b"hello"), (1, 2, b"")], [1, 2], optional=[9001])
    write_vector(
        output,
        "two-objects",
        two_objects.data,
        {
            "length": len(two_objects.data),
            "manifest_id": two_objects.manifest_id,
            "sha256": hashlib.sha256(two_objects.data).hexdigest(),
            "entries": two_objects.entries,
            "expected": "valid",
        },
    )

    unsupported = build([(1, 1, b"x")], [1], required=[42])
    write_vector(
        output,
        "unknown-required-capability",
        unsupported.data,
        {"expected_error": "unsupported_required_capability", "capability": 42},
    )

    digest_mismatch = bytearray(two_objects.data)
    # File header (32 bytes) plus first record header (40 bytes).
    digest_mismatch[72] ^= 1
    write_vector(
        output,
        "digest-mismatch",
        bytes(digest_mismatch),
        {"expected_error": "digest_mismatch"},
    )

    write_vector(
        output,
        "truncated-footer",
        two_objects.data[:-13],
        {"expected_error": "invalid_magic"},
    )

    invalid_offset = bytearray(two_objects.data)
    footer_offset = len(invalid_offset) - FOOTER_LEN
    invalid_offset[footer_offset + 16 : footer_offset + 24] = struct.pack("<Q", 2**64 - 1)
    write_vector(
        output,
        "invalid-directory-offset",
        bytes(invalid_offset),
        {"expected_error": "range_out_of_bounds"},
    )

    print(f"generated vectors in {output}")


if __name__ == "__main__":
    main()
