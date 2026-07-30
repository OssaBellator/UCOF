#!/usr/bin/env python3
"""Generate and verify pinned invalid/interrupted EXP-0002 Candidate 1 vectors.

The public contract is strict rejection plus a coarse intended validation layer.
Exact exception strings and validation-order-specific subcategories remain
implementation-local while FCP-0002 is Draft.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from dataclasses import dataclass
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

ROOT = Path(__file__).resolve().parents[1]
VALID = ROOT / "tests" / "vectors" / "exp-0002"


@dataclass(frozen=True)
class InvalidCase:
    name: str
    layer: str
    description: str
    data: bytes


def read_valid(name: str) -> bytearray:
    return bytearray.fromhex((VALID / f"{name}.hex").read_text(encoding="ascii"))


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
    root = read_u64(data, start + 56)
    data[start + 72 : start + 104] = hashlib.sha256(
        PAGE_DOMAIN + bytes(data[root : root + PAGE_SIZE])
    ).digest()
    refresh_snapshot_and_commit(data)


def cases() -> list[InvalidCase]:
    result: list[InvalidCase] = []

    data = read_valid("genesis-two-object")
    data[22] = 1
    result.append(
        InvalidCase(
            "header-reserved-nonzero",
            "bootstrap",
            "file-header reserved byte is non-zero",
            bytes(data),
        )
    )

    data = read_valid("genesis-two-object")
    first_record = 64
    payload_len = read_u64(data, first_record + 20)
    put_u64(data, first_record + 28, payload_len + 1)
    refresh_commit(data)
    result.append(
        InvalidCase(
            "object-logical-length-mismatch",
            "object",
            "object logical length differs from payload length after a valid outer commit digest",
            bytes(data),
        )
    )

    data = read_valid("genesis-two-object")
    snapshot, _ = snapshot_range(data)
    root = read_u64(data, snapshot + 56)
    data[root + PAGE_SIZE - 1] = 1
    refresh_root_snapshot_and_commit(data)
    result.append(
        InvalidCase(
            "leaf-padding-nonzero",
            "directory-page",
            "authenticated leaf page has non-zero unused padding",
            bytes(data),
        )
    )

    data = read_valid("multi-leaf-400")
    snapshot, _ = snapshot_range(data)
    root = read_u64(data, snapshot + 56)
    second_entry = root + PAGE_HEADER_LEN + INTERNAL_ENTRY_LEN
    first_max = read_u64(data, root + PAGE_HEADER_LEN + 8)
    put_u64(data, second_entry, first_max)
    refresh_root_snapshot_and_commit(data)
    result.append(
        InvalidCase(
            "internal-child-range-overlap",
            "directory-page",
            "authenticated internal page contains overlapping child ranges",
            bytes(data),
        )
    )

    data = read_valid("genesis-two-object")
    snapshot, _ = snapshot_range(data)
    root = read_u64(data, snapshot + 56)
    leaf_entry = root + PAGE_HEADER_LEN
    put_u64(data, leaf_entry + 16, root)
    refresh_root_snapshot_and_commit(data)
    result.append(
        InvalidCase(
            "object-overlaps-directory-page",
            "physical-layout",
            "leaf object locator points into its authenticating directory page",
            bytes(data),
        )
    )

    data = read_valid("genesis-two-object")
    snapshot, _ = snapshot_range(data)
    data[snapshot + 116] = 1
    refresh_snapshot_and_commit(data)
    result.append(
        InvalidCase(
            "snapshot-reserved-nonzero",
            "snapshot",
            "snapshot reserved byte is non-zero under refreshed snapshot and commit digests",
            bytes(data),
        )
    )

    data = read_valid("append-add-third")
    snapshot, _ = snapshot_range(data)
    footer = footer_offset(data)
    forward = footer + 1
    put_u64(data, snapshot + 48, forward)
    put_u64(data, footer + 56, forward)
    refresh_snapshot_and_commit(data)
    result.append(
        InvalidCase(
            "previous-footer-forward-pointer",
            "parent-chain",
            "snapshot and footer agree on a previous-footer pointer that points forward",
            bytes(data),
        )
    )

    data = read_valid("append-add-third")
    snapshot, _ = snapshot_range(data)
    data[snapshot + 16] ^= 1
    refresh_snapshot_and_commit(data)
    result.append(
        InvalidCase(
            "parent-snapshot-digest-mismatch",
            "parent-chain",
            "child snapshot names a parent digest that does not match the previous footer",
            bytes(data),
        )
    )

    data = read_valid("genesis-two-object")
    data.extend(b"tail")
    result.append(
        InvalidCase(
            "strict-trailing-bytes",
            "exact-end",
            "valid genesis bytes are followed by an unpublished tail",
            bytes(data),
        )
    )

    data = read_valid("genesis-two-object")
    result.append(
        InvalidCase(
            "footer-truncated-one-byte",
            "publication",
            "the exact-end footer is missing its final byte",
            bytes(data[:-1]),
        )
    )

    genesis = read_valid("genesis-two-object")
    append = read_valid("append-add-third")
    append_footer = footer_offset(append)
    cuts = (
        ("append-cut-after-object-header", len(genesis) + OBJECT_HEADER_LEN),
        ("append-cut-before-snapshot-complete", append_footer - 1),
        ("append-cut-footer-prefix", append_footer + 32),
    )
    for name, cut in cuts:
        result.append(
            InvalidCase(
                name,
                "publication",
                f"append is truncated at deterministic byte offset {cut}",
                bytes(append[:cut]),
            )
        )

    return result


def manifest(cases_: list[InvalidCase]) -> dict[str, object]:
    return {
        "epoch": "UCOF-EXP-0002",
        "candidate": 1,
        "contract": {
            "outcome": "strict rejection",
            "layer": "diagnostic intent only; exact error categories remain implementation-local",
        },
        "vectors": {
            case.name: {
                "description": case.description,
                "expectation": "invalid",
                "layer": case.layer,
                "length": len(case.data),
                "sha256": hashlib.sha256(case.data).hexdigest(),
                "hex_file": f"{case.name}.hex",
            }
            for case in cases_
        },
    }


def assert_rejected(case: InvalidCase) -> None:
    try:
        validate_strict(case.data)
    except Exp0002Error:
        return
    raise Exp0002Error(f"invalid vector was accepted: {case.name}")


def write_vectors(directory: Path) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    generated = cases()
    for case in generated:
        assert_rejected(case)
        (directory / f"{case.name}.hex").write_text(
            case.data.hex() + "\n", encoding="ascii"
        )
    (directory / "manifest.json").write_text(
        json.dumps(manifest(generated), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def verify_vectors(directory: Path) -> None:
    expected = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
    generated = {case.name: case for case in cases()}
    if set(generated) != set(expected["vectors"]):
        raise Exp0002Error("invalid-vector name mismatch")
    for name, case in generated.items():
        metadata = expected["vectors"][name]
        stored = bytes.fromhex(
            (directory / metadata["hex_file"]).read_text(encoding="ascii")
        )
        if stored != case.data:
            raise Exp0002Error(f"invalid-vector byte mismatch: {name}")
        if metadata["expectation"] != "invalid" or metadata["layer"] != case.layer:
            raise Exp0002Error(f"invalid-vector metadata mismatch: {name}")
        if len(stored) != metadata["length"]:
            raise Exp0002Error(f"invalid-vector length mismatch: {name}")
        if hashlib.sha256(stored).hexdigest() != metadata["sha256"]:
            raise Exp0002Error(f"invalid-vector hash mismatch: {name}")
        assert_rejected(InvalidCase(name, case.layer, case.description, stored))
    print(f"verified {len(generated)} pinned invalid/interrupted EXP-0002 vectors")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write-vectors", type=Path)
    parser.add_argument("--verify-vectors", type=Path)
    args = parser.parse_args()
    if (args.write_vectors is None) == (args.verify_vectors is None):
        parser.error("select exactly one operation")
    if args.write_vectors is not None:
        write_vectors(args.write_vectors)
    else:
        verify_vectors(args.verify_vectors)


if __name__ == "__main__":
    main()
