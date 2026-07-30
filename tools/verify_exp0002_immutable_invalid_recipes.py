#!/usr/bin/env python3
"""Materialize and verify the pinned immutable-successor invalid recipes."""

from __future__ import annotations

from hashlib import sha256
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_objects as objects

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "tests" / "vectors" / "exp-0002-immutable-invalid" / "cases.json"
REQUIRED_CASE_FIELDS = {
    "name",
    "operation",
    "expected",
    "decoded_bytes",
    "sha256",
}


def load_base(contract: dict) -> bytes:
    base_path = (CORPUS.parent / contract["base_vector"]).resolve()
    data = bytes.fromhex(base_path.read_text(encoding="ascii"))
    actual = sha256(data).hexdigest()
    if actual != contract["base_sha256"]:
        raise AssertionError(
            f"base vector SHA-256 mismatch: expected {contract['base_sha256']}, received {actual}"
        )
    objects.validate_complete(data)
    return data


def footer(data: bytes | bytearray) -> cow.FooterRecord:
    return cow.parse_footer(bytes(data), len(data) - cow.FOOTER_LEN)


def reauthenticate_root_and_footer(data: bytearray) -> None:
    current = footer(data)
    fields = list(cow.SNAPSHOT.unpack_from(data, current.snapshot_offset))
    page_offset = fields[2]
    page = bytes(data[page_offset : page_offset + cow.PAGE_SIZE])
    fields[4] = cow.digest(cow.PAGE_DOMAIN, page)
    cow.SNAPSHOT.pack_into(data, current.snapshot_offset, *fields)
    objects.reauthenticate_footer(data)


def materialize(operation: str, base: bytes) -> bytes:
    report = objects.validate_complete(base)
    first = report.objects[0]
    current_footer = footer(base)
    page_offset = report.structural.root.offset

    if operation == "header_magic":
        output = bytearray(base)
        output[0] ^= 1
        return bytes(output)

    if operation == "footer_reserved":
        output = bytearray(base)
        output[-1] = 1
        return bytes(output)

    if operation == "commit_digest":
        output = bytearray(base)
        output[len(base) - cow.FOOTER_LEN + 80] ^= 1
        return bytes(output)

    if operation == "object_header_reserved":
        output = bytearray(base)
        output[first.record_offset + 40] = 1
        objects.reauthenticate_footer(output)
        return bytes(output)

    if operation == "object_payload_digest":
        output = bytearray(base)
        output[first.record_offset + objects.OBJECT_HEADER_LEN] ^= 1
        objects.reauthenticate_footer(output)
        return bytes(output)

    if operation == "leaf_order":
        output = bytearray(base)
        first_entry = page_offset + cow.PAGE_HEADER_LEN
        second_entry = first_entry + cow.LEAF_ENTRY_LEN
        left = bytes(output[first_entry : first_entry + cow.LEAF_ENTRY_LEN])
        right = bytes(output[second_entry : second_entry + cow.LEAF_ENTRY_LEN])
        output[first_entry : first_entry + cow.LEAF_ENTRY_LEN] = right
        output[second_entry : second_entry + cow.LEAF_ENTRY_LEN] = left
        reauthenticate_root_and_footer(output)
        return bytes(output)

    if operation == "leaf_padding":
        output = bytearray(base)
        output[page_offset + cow.PAGE_SIZE - 1] = 1
        reauthenticate_root_and_footer(output)
        return bytes(output)

    if operation == "leaf_header_reserved":
        output = bytearray(base)
        output[page_offset + 10] = 1
        reauthenticate_root_and_footer(output)
        return bytes(output)

    if operation == "object_overlaps_page":
        output = bytearray(base)
        entry_offset = page_offset + cow.PAGE_HEADER_LEN
        values = list(cow.LEAF_ENTRY.unpack_from(output, entry_offset))
        values[3] = page_offset
        values[4] = objects.OBJECT_HEADER_LEN
        values[5] = 0
        values[6] = bytes(32)
        cow.LEAF_ENTRY.pack_into(output, entry_offset, *values)
        reauthenticate_root_and_footer(output)
        return bytes(output)

    if operation == "snapshot_root_digest":
        output = bytearray(base)
        fields = list(cow.SNAPSHOT.unpack_from(output, current_footer.snapshot_offset))
        fields[4] = bytes([9]) * 32
        cow.SNAPSHOT.pack_into(output, current_footer.snapshot_offset, *fields)
        objects.reauthenticate_footer(output)
        return bytes(output)

    if operation == "genesis_parent_nonzero":
        output = bytearray(base)
        fields = list(cow.SNAPSHOT.unpack_from(output, current_footer.snapshot_offset))
        fields[5] = bytes([1]) + bytes(31)
        cow.SNAPSHOT.pack_into(output, current_footer.snapshot_offset, *fields)
        objects.reauthenticate_footer(output)
        return bytes(output)

    if operation == "trailing_byte":
        return base + b"x"

    if operation == "interrupted_footer_half":
        return base[: -cow.FOOTER_LEN // 2]

    raise AssertionError(f"unknown invalid recipe operation: {operation}")


def reject(case: dict, data: bytes) -> str:
    try:
        objects.validate_complete(data)
    except cow.FormatError as error:
        actual = str(error)
    else:
        raise AssertionError(f"{case['name']}: malformed bytes validated")
    if actual != case["expected"]:
        raise AssertionError(
            f"{case['name']}: expected {case['expected']!r}, received {actual!r}"
        )
    return actual


def main() -> None:
    contract = json.loads(CORPUS.read_text(encoding="utf-8"))
    if contract["status"] != "non-normative successor invalid corpus recipes":
        raise AssertionError("invalid corpus status")
    cases = contract["cases"]
    if any(set(case) != REQUIRED_CASE_FIELDS for case in cases):
        raise AssertionError("invalid corpus case fields")
    names = [case["name"] for case in cases]
    operations = [case["operation"] for case in cases]
    if len(cases) != 13 or len(names) != len(set(names)):
        raise AssertionError("invalid corpus case count or duplicate name")
    if len(operations) != len(set(operations)):
        raise AssertionError("duplicate invalid corpus operation")

    base = load_base(contract)
    aggregate = sha256()
    for case in cases:
        first = materialize(case["operation"], base)
        second = materialize(case["operation"], base)
        if first != second:
            raise AssertionError(f"{case['name']}: mutation was not deterministic")
        actual = reject(case, first)
        digest = sha256(first).hexdigest()
        if len(first) != case["decoded_bytes"]:
            raise AssertionError(
                f"{case['name']}: byte length expected {case['decoded_bytes']}, received {len(first)}"
            )
        if digest != case["sha256"]:
            raise AssertionError(
                f"{case['name']}: SHA-256 expected {case['sha256']}, received {digest}"
            )
        aggregate.update(case["name"].encode("utf-8"))
        aggregate.update(bytes.fromhex(digest))
        print(
            f"{case['name']}: expected={actual} bytes={len(first)} sha256={digest}"
        )

    aggregate_digest = aggregate.hexdigest()
    if aggregate_digest != contract["aggregate_sha256"]:
        raise AssertionError(
            f"aggregate SHA-256 expected {contract['aggregate_sha256']}, received {aggregate_digest}"
        )
    print(f"base_bytes={len(base)}")
    print(f"base_sha256={sha256(base).hexdigest()}")
    print(f"invalid_cases={len(cases)}")
    print(f"aggregate_sha256={aggregate_digest}")
    print("deterministic_materialization=pass")
    print("coarse_rejection_layers=pass")
    print("cryptographic_recipe_pins=pass")


if __name__ == "__main__":
    main()
