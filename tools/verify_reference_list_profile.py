#!/usr/bin/env python3
"""Independent codec and malformed-case verifier for Experiment 0065."""

from __future__ import annotations

import hashlib
import json
import struct
from typing import Any

EXPECTED_AGGREGATE = "9b3a9ccb579162e64431679e43d4ebd431b308c3ea78be6fe57decad8fe48409"


def encode_reference_list(identifiers: list[int], maximum: int) -> bytes:
    if len(identifiers) > maximum:
        raise ValueError("reference count")
    previous: int | None = None
    for identifier in identifiers:
        if identifier == 0 or (previous is not None and previous >= identifier):
            raise ValueError("reference order")
        previous = identifier
    return (
        struct.pack("<I", len(identifiers))
        + bytes(4)
        + b"".join(struct.pack("<Q", identifier) for identifier in identifiers)
    )


def decode_reference_list(payload: bytes, maximum: int) -> list[int]:
    if len(payload) < 8 or payload[4:8] != bytes(4):
        raise ValueError("reference header")
    count = struct.unpack("<I", payload[:4])[0]
    if count > maximum:
        raise ValueError("reference count")
    if len(payload) != 8 + count * 8:
        raise ValueError("reference length")

    identifiers: list[int] = []
    previous: int | None = None
    for index in range(count):
        start = 8 + index * 8
        identifier = struct.unpack("<Q", payload[start : start + 8])[0]
        if identifier == 0 or (previous is not None and previous >= identifier):
            raise ValueError("reference order")
        identifiers.append(identifier)
        previous = identifier
    return identifiers


def evaluate() -> list[dict[str, Any]]:
    empty = "0000000000000000"
    three = "0300000000000000020000000000000007000000000000000900000000000000"
    maximum = (
        "040000000000000001000000000000000300000000000000"
        "05000000000000000700000000000000"
    )
    cases: list[tuple[str, str, Any, int, str | None]] = [
        ("encode-empty", "encode", [], 8, empty),
        ("encode-three", "encode", [2, 7, 9], 8, three),
        ("encode-max", "encode", [1, 3, 5, 7], 4, maximum),
        ("decode-empty", "decode", empty, 8, None),
        ("decode-three", "decode", three, 8, None),
        ("decode-max", "decode", maximum, 4, None),
        ("reserved", "decode", "01000000010000000200000000000000", 8, None),
        ("truncated", "decode", "0100000000000000", 8, None),
        (
            "trailing",
            "decode",
            "0100000000000000020000000000000000",
            8,
            None,
        ),
        (
            "duplicate",
            "decode",
            "020000000000000002000000000000000200000000000000",
            8,
            None,
        ),
        (
            "descending",
            "decode",
            "020000000000000003000000000000000200000000000000",
            8,
            None,
        ),
        ("zero", "decode", "01000000000000000000000000000000", 8, None),
        (
            "over-limit",
            "decode",
            "020000000000000001000000000000000200000000000000",
            1,
            None,
        ),
        ("encode-order", "encode", [4, 3], 8, None),
        ("encode-count", "encode", [1, 2, 3], 2, None),
    ]

    results: list[dict[str, Any]] = []
    for name, operation, value, limit, expected in cases:
        try:
            if operation == "encode":
                encoded = encode_reference_list(value, limit).hex()
                if expected is not None and encoded != expected:
                    raise ValueError("encoder mismatch")
                results.append({"name": name, "ok": True, "hex": encoded})
            else:
                identifiers = decode_reference_list(bytes.fromhex(value), limit)
                results.append({"name": name, "ok": True, "ids": identifiers})
        except ValueError as error:
            results.append({"name": name, "ok": False, "error": str(error)})
    return results


def main() -> None:
    first = evaluate()
    second = evaluate()
    if first != second:
        raise SystemExit("reference-list verifier is nondeterministic")
    encoded = json.dumps(first, sort_keys=True, separators=(",", ":")).encode()
    aggregate = hashlib.sha256(encoded).hexdigest()
    if aggregate != EXPECTED_AGGREGATE:
        raise SystemExit(
            f"reference-list aggregate mismatch: {aggregate} != {EXPECTED_AGGREGATE}"
        )
    print(f"verified {len(first)} reference-list cases: {aggregate}")


if __name__ == "__main__":
    main()
