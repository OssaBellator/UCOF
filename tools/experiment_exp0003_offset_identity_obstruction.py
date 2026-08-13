#!/usr/bin/env python3
"""Demonstrate the physical-offset obstruction to history-independent root identity.

EXP-0003 authenticates absolute physical offsets inside leaf locators and
internal child references. Therefore two structurally equivalent active trees
whose objects/pages occupy different physical offsets have different page bytes
and hence different page/root digests, even if their partition boundaries and
object record bytes are otherwise identical.

The experiment pins this property for both the current first-Draft 128-bit
geometry and the tight 64-bit Review candidate. It is evidence only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import asdict, dataclass

PAGE_SIZE = 16_384
PAGE_MAGIC = b"UCPGIM03"
PAGE_DOMAIN = b"UCOF-EXP-0003-PAGE\0"
OBJECT_DOMAIN = b"UCOF-EXP-0003-OBJECT\0"
SNAPSHOT_DOMAIN = b"UCOF-EXP-0003-SNAPSHOT\0"
SNAPSHOT_MAGIC = b"UCSNIM03"


@dataclass(frozen=True)
class Geometry:
    name: str
    id_width: int
    object_header_len: int
    page_header_len: int
    leaf_entry_len: int
    internal_entry_len: int
    snapshot_len: int


GEOMETRIES = (
    Geometry(
        name="draft-128",
        id_width=16,
        object_header_len=64,
        page_header_len=80,
        leaf_entry_len=64,
        internal_entry_len=72,
        snapshot_len=96,
    ),
    Geometry(
        name="tight-64-review-candidate",
        id_width=8,
        object_header_len=40,
        page_header_len=40,
        leaf_entry_len=56,
        internal_entry_len=56,
        # 104 if the catalog-v2 snapshot proposal is ultimately accepted.
        snapshot_len=104,
    ),
)


@dataclass(frozen=True)
class Result:
    geometry: str
    object_digest_equal_after_relocation: bool
    leaf_bytes_equal_after_object_relocation: bool
    leaf_digest_equal_after_object_relocation: bool
    internal_bytes_equal_after_child_relocation: bool
    internal_digest_equal_after_child_relocation: bool
    snapshot_digest_equal_after_root_relocation: bool
    leaf_a_digest: str
    leaf_b_digest: str
    internal_a_digest: str
    internal_b_digest: str
    snapshot_a_digest: str
    snapshot_b_digest: str


def le(value: int, width: int) -> bytes:
    return value.to_bytes(width, "little", signed=False)


def object_id(value: int, width: int) -> bytes:
    # ObjectId ordering is opaque lexicographic bytes; using a big-endian test
    # value makes the byte ordering obvious without making integer semantics
    # normative.
    return value.to_bytes(width, "big", signed=False)


def sha256(domain: bytes, payload: bytes) -> bytes:
    return hashlib.sha256(domain + payload).digest()


def make_object_record(geometry: Geometry, oid: bytes, payload: bytes) -> bytes:
    if geometry.id_width == 16:
        header = (
            b"UCOBOBJ3"
            + le(geometry.object_header_len, 2)
            + le(2, 2)
            + le(0, 4)
            + oid
            + le(len(payload), 8)
            + le(len(payload), 8)
            + bytes(geometry.object_header_len - 48)
        )
    else:
        header = (
            b"UCOBOBJ3"
            + le(geometry.object_header_len, 2)
            + le(2, 2)
            + le(0, 4)
            + oid
            + le(len(payload), 8)
            + le(len(payload), 8)
        )
    assert len(header) == geometry.object_header_len
    return header + payload


def make_page_header(
    geometry: Geometry,
    *,
    kind: int,
    level: int,
    entry_count: int,
    entry_width: int,
    minimum: bytes,
    maximum: bytes,
) -> bytes:
    semantic = (
        PAGE_MAGIC
        + bytes((kind, level))
        + le(geometry.page_header_len, 2)
        + le(entry_count, 4)
        + le(entry_width, 2)
        + le(0, 2)
        + minimum
        + maximum
    )
    assert len(semantic) <= geometry.page_header_len
    return semantic + bytes(geometry.page_header_len - len(semantic))


def make_leaf_page(
    geometry: Geometry,
    *,
    oid: bytes,
    object_offset: int,
    object_record: bytes,
    object_digest: bytes,
) -> bytes:
    header = make_page_header(
        geometry,
        kind=1,
        level=0,
        entry_count=1,
        entry_width=geometry.leaf_entry_len,
        minimum=oid,
        maximum=oid,
    )
    entry = oid + le(object_offset, 8) + le(len(object_record), 8) + object_digest
    assert len(entry) == geometry.leaf_entry_len
    page = header + entry
    assert len(page) <= PAGE_SIZE
    return page + bytes(PAGE_SIZE - len(page))


def make_internal_page(
    geometry: Geometry,
    *,
    left_oid: bytes,
    right_oid: bytes,
    left_offset: int,
    right_offset: int,
    left_digest: bytes,
    right_digest: bytes,
) -> bytes:
    header = make_page_header(
        geometry,
        kind=2,
        level=1,
        entry_count=2,
        entry_width=geometry.internal_entry_len,
        minimum=left_oid,
        maximum=right_oid,
    )
    left = left_oid + left_oid + le(left_offset, 8) + left_digest
    right = right_oid + right_oid + le(right_offset, 8) + right_digest
    assert len(left) == geometry.internal_entry_len
    assert len(right) == geometry.internal_entry_len
    page = header + left + right
    assert len(page) <= PAGE_SIZE
    return page + bytes(PAGE_SIZE - len(page))


def make_snapshot(
    geometry: Geometry,
    *,
    root_offset: int,
    root_digest: bytes,
    catalog_oid: bytes,
) -> bytes:
    if geometry.snapshot_len == 96:
        # Current first Draft: u64 root level and no catalog field.
        snapshot = (
            SNAPSHOT_MAGIC
            + le(0, 8)
            + le(root_offset, 8)
            + le(0, 8)
            + root_digest
            + bytes(32)
        )
    else:
        # Catalog-v2 + tight-64 review shape: u8 root level + 7 zero bytes,
        # 8-byte catalog ObjectId, then parent snapshot digest.
        snapshot = (
            SNAPSHOT_MAGIC
            + le(0, 8)
            + le(root_offset, 8)
            + bytes((0,))
            + bytes(7)
            + root_digest
            + catalog_oid
            + bytes(32)
        )
    assert len(snapshot) == geometry.snapshot_len
    return snapshot


def run_one(geometry: Geometry) -> Result:
    oid1 = object_id(1, geometry.id_width)
    oid2 = object_id(2, geometry.id_width)
    catalog_oid = object_id(9, geometry.id_width)

    object_record = make_object_record(geometry, oid1, b"same logical payload")
    object_digest = sha256(OBJECT_DOMAIN, object_record)

    leaf_a = make_leaf_page(
        geometry,
        oid=oid1,
        object_offset=4_096,
        object_record=object_record,
        object_digest=object_digest,
    )
    leaf_b = make_leaf_page(
        geometry,
        oid=oid1,
        object_offset=8_192,
        object_record=object_record,
        object_digest=object_digest,
    )
    leaf_a_digest = sha256(PAGE_DOMAIN, leaf_a)
    leaf_b_digest = sha256(PAGE_DOMAIN, leaf_b)

    # Use identical child page content digests in both internal pages; change
    # only one child's physical offset in the parent reference.
    fixed_left_digest = hashlib.sha256(b"left child bytes").digest()
    fixed_right_digest = hashlib.sha256(b"right child bytes").digest()
    internal_a = make_internal_page(
        geometry,
        left_oid=oid1,
        right_oid=oid2,
        left_offset=16_384,
        right_offset=32_768,
        left_digest=fixed_left_digest,
        right_digest=fixed_right_digest,
    )
    internal_b = make_internal_page(
        geometry,
        left_oid=oid1,
        right_oid=oid2,
        left_offset=49_152,
        right_offset=32_768,
        left_digest=fixed_left_digest,
        right_digest=fixed_right_digest,
    )
    internal_a_digest = sha256(PAGE_DOMAIN, internal_a)
    internal_b_digest = sha256(PAGE_DOMAIN, internal_b)

    # Even if a hypothetical root digest were held constant, snapshot identity
    # is separately placement-sensitive because the root physical offset is in
    # the snapshot bytes.
    common_root_digest = hashlib.sha256(b"same hypothetical root page").digest()
    snapshot_a = make_snapshot(
        geometry,
        root_offset=65_536,
        root_digest=common_root_digest,
        catalog_oid=catalog_oid,
    )
    snapshot_b = make_snapshot(
        geometry,
        root_offset=81_920,
        root_digest=common_root_digest,
        catalog_oid=catalog_oid,
    )
    snapshot_a_digest = sha256(SNAPSHOT_DOMAIN, snapshot_a)
    snapshot_b_digest = sha256(SNAPSHOT_DOMAIN, snapshot_b)

    result = Result(
        geometry=geometry.name,
        object_digest_equal_after_relocation=(
            sha256(OBJECT_DOMAIN, object_record) == object_digest
        ),
        leaf_bytes_equal_after_object_relocation=(leaf_a == leaf_b),
        leaf_digest_equal_after_object_relocation=(leaf_a_digest == leaf_b_digest),
        internal_bytes_equal_after_child_relocation=(internal_a == internal_b),
        internal_digest_equal_after_child_relocation=(
            internal_a_digest == internal_b_digest
        ),
        snapshot_digest_equal_after_root_relocation=(
            snapshot_a_digest == snapshot_b_digest
        ),
        leaf_a_digest=leaf_a_digest.hex(),
        leaf_b_digest=leaf_b_digest.hex(),
        internal_a_digest=internal_a_digest.hex(),
        internal_b_digest=internal_b_digest.hex(),
        snapshot_a_digest=snapshot_a_digest.hex(),
        snapshot_b_digest=snapshot_b_digest.hex(),
    )

    assert result.object_digest_equal_after_relocation
    assert not result.leaf_bytes_equal_after_object_relocation
    assert not result.leaf_digest_equal_after_object_relocation
    assert not result.internal_bytes_equal_after_child_relocation
    assert not result.internal_digest_equal_after_child_relocation
    assert not result.snapshot_digest_equal_after_root_relocation
    return result


def results() -> list[Result]:
    return [run_one(geometry) for geometry in GEOMETRIES]


def print_csv(rows: list[Result]) -> None:
    print(
        "geometry,object_digest_same,leaf_bytes_same,leaf_digest_same,"
        "internal_bytes_same,internal_digest_same,snapshot_digest_same"
    )
    for row in rows:
        print(
            f"{row.geometry},{int(row.object_digest_equal_after_relocation)},"
            f"{int(row.leaf_bytes_equal_after_object_relocation)},"
            f"{int(row.leaf_digest_equal_after_object_relocation)},"
            f"{int(row.internal_bytes_equal_after_child_relocation)},"
            f"{int(row.internal_digest_equal_after_child_relocation)},"
            f"{int(row.snapshot_digest_equal_after_root_relocation)}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    rows = results()
    if args.json:
        print(json.dumps([asdict(row) for row in rows], indent=2, sort_keys=True))
    else:
        print_csv(rows)


if __name__ == "__main__":
    main()
