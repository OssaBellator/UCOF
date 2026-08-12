#!/usr/bin/env python3
"""Complete object records integrated with immutable-page snapshots."""

from __future__ import annotations

import struct
from dataclasses import dataclass

import experiment_exp0002_immutable_page_cow as cow

OBJECT_MAGIC = b"UCOBOBJ2"
OBJECT_HEADER_LEN = 48
OBJECT_HEADER = struct.Struct("<8sHHIQQQ8s")
OBJECTS = 10_000


class ObjectError(cow.FormatError):
    pass


@dataclass(frozen=True)
class ObjectInput:
    object_id: int
    kind: int
    payload: bytes


@dataclass(frozen=True)
class CompleteReport:
    structural: cow.ValidationReport
    objects: tuple[cow.Locator, ...]
    object_payloads: dict[int, bytes]


def encode_object(value: ObjectInput) -> bytes:
    if value.object_id == 0 or value.kind == 0:
        raise ValueError("object identifiers and kinds must be non-zero")
    header = OBJECT_HEADER.pack(
        OBJECT_MAGIC,
        OBJECT_HEADER_LEN,
        value.kind,
        0,
        value.object_id,
        len(value.payload),
        len(value.payload),
        bytes(8),
    )
    return header + value.payload


def append_object(output: bytearray, value: ObjectInput) -> cow.Locator:
    record = encode_object(value)
    offset = len(output)
    output.extend(record)
    return cow.Locator(
        object_id=value.object_id,
        kind=value.kind,
        record_offset=offset,
        record_len=len(record),
        logical_len=len(value.payload),
        digest=cow.digest(cow.OBJECT_DOMAIN, record),
    )


def build_genesis(values: list[ObjectInput]) -> bytes:
    ordered = sorted(values, key=lambda value: value.object_id)
    if not ordered or any(
        left.object_id == right.object_id for left, right in zip(ordered, ordered[1:])
    ):
        raise ValueError("objects must contain unique identifiers")
    output = bytearray(cow.FILE_HEADER_LEN)
    output[: len(cow.FILE_MAGIC)] = cow.FILE_MAGIC
    locators = [append_object(output, value) for value in ordered]
    page_start = len(output)
    root = cow.build_tree(output, locators)
    page_count = (len(output) - page_start) // cow.PAGE_SIZE
    cow.publish(output, 0, root, bytes(32), cow.ABSENT_OFFSET, page_count)
    return bytes(output)


def root_reference(data: bytes, footer: cow.FooterRecord) -> cow.PageRef:
    snapshot = cow.checked_slice(data, footer.snapshot_offset, footer.snapshot_len, "snapshot")
    magic, sequence, root_offset, root_level, root_digest, _parent = cow.SNAPSHOT.unpack(snapshot)
    if magic != cow.SNAPSHOT_MAGIC or sequence != footer.sequence:
        raise ObjectError("snapshot")
    page = cow.checked_slice(data, root_offset, cow.PAGE_SIZE, "root page")
    page_fields = cow.PAGE_HEADER.unpack_from(page)
    minimum, maximum = page_fields[6], page_fields[7]
    return cow.PageRef(minimum, maximum, root_offset, root_level, root_digest)


def parse_object(data: bytes, locator: cow.Locator) -> bytes:
    record = cow.checked_slice(data, locator.record_offset, locator.record_len, "object")
    if len(record) < OBJECT_HEADER_LEN:
        raise ObjectError("object header")
    magic, header_len, kind, flags, object_id, payload_len, logical_len, reserved = OBJECT_HEADER.unpack_from(record)
    if (
        magic != OBJECT_MAGIC
        or header_len != OBJECT_HEADER_LEN
        or kind == 0
        or flags != 0
        or object_id == 0
        or any(reserved)
    ):
        raise ObjectError("object header")
    if payload_len != logical_len or OBJECT_HEADER_LEN + payload_len != len(record):
        raise ObjectError("object length")
    if (
        object_id != locator.object_id
        or kind != locator.kind
        or logical_len != locator.logical_len
    ):
        raise ObjectError("object locator")
    if cow.digest(cow.OBJECT_DOMAIN, record) != locator.digest:
        raise ObjectError("object digest")
    return record[OBJECT_HEADER_LEN:]


def validate_complete(data: bytes) -> CompleteReport:
    if len(data) < cow.FILE_HEADER_LEN + OBJECT_HEADER_LEN + cow.PAGE_SIZE + cow.SNAPSHOT_LEN + cow.FOOTER_LEN:
        raise ObjectError("file too short")
    header = cow.checked_slice(data, 0, cow.FILE_HEADER_LEN, "header")
    if header[: len(cow.FILE_MAGIC)] != cow.FILE_MAGIC or any(header[len(cow.FILE_MAGIC) :]):
        raise ObjectError("header")

    footer_offset = len(data) - cow.FOOTER_LEN
    footer = cow.parse_footer(data, footer_offset)
    if footer.snapshot_len != cow.SNAPSHOT_LEN or footer.snapshot_offset + footer.snapshot_len != footer_offset:
        raise ObjectError("snapshot locator")
    snapshot = cow.checked_slice(data, footer.snapshot_offset, footer.snapshot_len, "snapshot")
    if cow.digest(cow.SNAPSHOT_DOMAIN, snapshot) != footer.snapshot_digest:
        raise ObjectError("snapshot digest")
    magic, sequence, _root_offset, _root_level, _root_digest, parent_snapshot_digest = cow.SNAPSHOT.unpack(snapshot)
    if magic != cow.SNAPSHOT_MAGIC or sequence != footer.sequence:
        raise ObjectError("snapshot")
    if footer.previous_footer_offset == cow.ABSENT_OFFSET:
        if sequence != 0 or any(parent_snapshot_digest):
            raise ObjectError("genesis linkage")
        commit_start = 0
    else:
        if footer.previous_footer_offset + cow.FOOTER_LEN > footer.snapshot_offset:
            raise ObjectError("previous footer")
        parent = cow.parse_footer(data, footer.previous_footer_offset)
        if sequence != parent.sequence + 1 or parent.snapshot_digest != parent_snapshot_digest:
            raise ObjectError("parent linkage")
        commit_start = footer.previous_footer_offset + cow.FOOTER_LEN
    semantics = cow.footer_semantics(
        footer.sequence,
        footer.snapshot_offset,
        footer.snapshot_len,
        footer.previous_footer_offset,
        footer.page_count_current,
        footer.snapshot_digest,
    )
    if cow.digest(cow.COMMIT_DOMAIN, data[commit_start:footer_offset] + semantics) != footer.commit_digest:
        raise ObjectError("commit digest")

    root = root_reference(data, footer)
    stack = [root]
    seen_pages: set[int] = set()
    locators: list[cow.Locator] = []
    structural_ranges: list[tuple[int, int]] = [
        (footer.snapshot_offset, footer.snapshot_offset + footer.snapshot_len),
        (footer_offset, len(data)),
    ]
    while stack:
        reference = stack.pop()
        if reference.offset in seen_pages:
            raise ObjectError("page cycle")
        if reference.offset < cow.FILE_HEADER_LEN or reference.offset + cow.PAGE_SIZE > footer.snapshot_offset:
            raise ObjectError("page range")
        seen_pages.add(reference.offset)
        structural_ranges.append((reference.offset, reference.offset + cow.PAGE_SIZE))
        kind, entries = cow.decode_page(data, reference)
        if kind == 1:
            locators.extend(entry for entry in entries if isinstance(entry, cow.Locator))
        else:
            stack.extend(
                reversed([entry for entry in entries if isinstance(entry, cow.PageRef)])
            )

    if not locators or any(
        left.object_id >= right.object_id for left, right in zip(locators, locators[1:])
    ):
        raise ObjectError("object order")
    if (locators[0].object_id, locators[-1].object_id) != (root.minimum, root.maximum):
        raise ObjectError("root object range")

    object_ranges: list[tuple[int, int, int]] = []
    payloads: dict[int, bytes] = {}
    for locator in locators:
        end = locator.record_offset + locator.record_len
        if locator.record_offset < cow.FILE_HEADER_LEN or end > footer.snapshot_offset:
            raise ObjectError("object range")
        for structural_start, structural_end in structural_ranges:
            if locator.record_offset < structural_end and structural_start < end:
                raise ObjectError("object structural overlap")
        payloads[locator.object_id] = parse_object(data, locator)
        object_ranges.append((locator.record_offset, end, locator.object_id))

    ordered_ranges = sorted(object_ranges)
    for left, right in zip(ordered_ranges, ordered_ranges[1:]):
        if left[1] > right[0]:
            raise ObjectError("object overlap")

    structural = cow.ValidationReport(
        footer.sequence,
        footer_offset,
        root,
        footer.snapshot_digest,
        footer.commit_digest,
        frozenset(seen_pages),
    )
    return CompleteReport(structural, tuple(locators), payloads)


def append_replacement(data: bytes, value: ObjectInput) -> bytes:
    verified = validate_complete(data)
    if value.object_id not in verified.object_payloads:
        raise KeyError(value.object_id)
    output = bytearray(data)
    replacement = append_object(output, value)
    page_start = len(output)
    root = cow.rewrite_node(
        output,
        data,
        verified.structural.root,
        {value.object_id: replacement},
    )
    page_count = (len(output) - page_start) // cow.PAGE_SIZE
    cow.publish(
        output,
        verified.structural.sequence + 1,
        root,
        verified.structural.snapshot_digest,
        verified.structural.footer_offset,
        page_count,
    )
    return bytes(output)


def reauthenticate_footer(data: bytearray) -> None:
    offset = len(data) - cow.FOOTER_LEN
    footer = cow.parse_footer(bytes(data), offset)
    snapshot = bytes(data[footer.snapshot_offset : footer.snapshot_offset + footer.snapshot_len])
    snapshot_digest = cow.digest(cow.SNAPSHOT_DOMAIN, snapshot)
    semantics = cow.footer_semantics(
        footer.sequence,
        footer.snapshot_offset,
        footer.snapshot_len,
        footer.previous_footer_offset,
        footer.page_count_current,
        snapshot_digest,
    )
    commit_start = (
        0
        if footer.previous_footer_offset == cow.ABSENT_OFFSET
        else footer.previous_footer_offset + cow.FOOTER_LEN
    )
    commit_digest = cow.digest(
        cow.COMMIT_DOMAIN, bytes(data[commit_start:offset]) + semantics
    )
    cow.FOOTER.pack_into(
        data,
        offset,
        cow.FOOTER_MAGIC,
        footer.sequence,
        footer.snapshot_offset,
        footer.snapshot_len,
        footer.previous_footer_offset,
        footer.page_count_current,
        snapshot_digest,
        commit_digest,
        bytes(16),
    )


def forge_authenticated_structural_overlap(
    data: bytes, report: CompleteReport, object_id: int
) -> bytes:
    output = bytearray(data)
    footer = cow.parse_footer(data, len(data) - cow.FOOTER_LEN)
    root = report.structural.root
    if root.level != 1:
        raise AssertionError("overlap fixture expects a height-one root")
    kind, root_entries = cow.decode_page(data, root)
    if kind != 2:
        raise AssertionError("root is not internal")
    children = [entry for entry in root_entries if isinstance(entry, cow.PageRef)]
    child_index = next(
        index
        for index, child in enumerate(children)
        if child.minimum <= object_id <= child.maximum
    )
    leaf_ref = children[child_index]
    leaf = bytearray(data[leaf_ref.offset : leaf_ref.offset + cow.PAGE_SIZE])
    _magic, _kind, _level, _reserved, count, _entry_size, _minimum, _maximum, _tail = cow.PAGE_HEADER.unpack_from(leaf)
    entry_index = None
    for index in range(count):
        entry_offset = cow.PAGE_HEADER_LEN + index * cow.LEAF_ENTRY_LEN
        values = list(cow.LEAF_ENTRY.unpack_from(leaf, entry_offset))
        if values[0] == object_id:
            entry_index = index
            values[3] = leaf_ref.offset
            values[4] = OBJECT_HEADER_LEN
            values[5] = 0
            values[6] = bytes(32)
            cow.LEAF_ENTRY.pack_into(leaf, entry_offset, *values)
            break
    if entry_index is None:
        raise AssertionError("object was not found in leaf")
    output[leaf_ref.offset : leaf_ref.offset + cow.PAGE_SIZE] = leaf
    leaf_digest = cow.digest(cow.PAGE_DOMAIN, bytes(leaf))

    root_page = bytearray(data[root.offset : root.offset + cow.PAGE_SIZE])
    child_entry_offset = cow.PAGE_HEADER_LEN + child_index * cow.INTERNAL_ENTRY_LEN
    child_values = list(cow.INTERNAL_ENTRY.unpack_from(root_page, child_entry_offset))
    child_values[4] = leaf_digest
    cow.INTERNAL_ENTRY.pack_into(root_page, child_entry_offset, *child_values)
    output[root.offset : root.offset + cow.PAGE_SIZE] = root_page
    root_digest = cow.digest(cow.PAGE_DOMAIN, bytes(root_page))

    snapshot_values = list(cow.SNAPSHOT.unpack_from(output, footer.snapshot_offset))
    snapshot_values[4] = root_digest
    cow.SNAPSHOT.pack_into(output, footer.snapshot_offset, *snapshot_values)
    reauthenticate_footer(output)
    return bytes(output)


def main() -> None:
    values = [
        ObjectInput(object_id, 1 + object_id % 3, f"payload:{object_id}".encode("ascii"))
        for object_id in range(1, OBJECTS + 1)
    ]
    genesis = build_genesis(values)
    genesis_report = validate_complete(genesis)
    assert len(genesis_report.objects) == OBJECTS
    assert genesis_report.object_payloads[1] == b"payload:1"

    replacement = ObjectInput(1, 2, b"replacement payload for object one")
    appended = append_replacement(genesis, replacement)
    repeated = append_replacement(genesis, replacement)
    assert appended == repeated
    appended_report = validate_complete(appended)
    assert appended_report.structural.sequence == 1
    assert appended_report.object_payloads[1] == replacement.payload
    assert appended_report.object_payloads[2] == b"payload:2"

    new_pages = (
        appended_report.structural.reachable_pages
        - genesis_report.structural.reachable_pages
    )
    reused_pages = (
        appended_report.structural.reachable_pages
        & genesis_report.structural.reachable_pages
    )
    retired_pages = (
        genesis_report.structural.reachable_pages
        - appended_report.structural.reachable_pages
    )
    assert len(new_pages) == genesis_report.structural.root.level + 1
    assert len(retired_pages) == len(new_pages)
    assert len(reused_pages) == len(genesis_report.structural.reachable_pages) - len(new_pages)

    old_object_offset = genesis_report.objects[0].record_offset
    new_object_offset = appended_report.objects[0].record_offset
    assert new_object_offset == len(genesis)
    assert old_object_offset != new_object_offset

    corrupted_historical = bytearray(appended)
    historical_locator = appended_report.objects[1]
    corrupted_historical[historical_locator.record_offset + OBJECT_HEADER_LEN] ^= 1
    try:
        validate_complete(bytes(corrupted_historical))
    except ObjectError as error:
        corruption_error = str(error)
    else:
        raise AssertionError("historical object corruption was not detected")
    assert corruption_error == "object digest"

    forged_overlap = forge_authenticated_structural_overlap(
        appended, appended_report, 2
    )
    try:
        validate_complete(forged_overlap)
    except ObjectError as error:
        overlap_error = str(error)
    else:
        raise AssertionError("authenticated structural overlap validated")
    assert overlap_error == "object structural overlap"

    interrupted = appended[:-cow.FOOTER_LEN // 2]
    try:
        validate_complete(interrupted)
    except cow.FormatError:
        pass
    else:
        raise AssertionError("interrupted object append validated")
    validate_complete(genesis)

    print(f"objects={OBJECTS:,}")
    print(f"genesis_bytes={len(genesis):,}")
    print(f"append_bytes={len(appended):,}")
    print(f"genesis_pages={len(genesis_report.structural.reachable_pages):,}")
    print(f"replacement_new_pages={len(new_pages)}")
    print(f"replacement_reused_pages={len(reused_pages):,}")
    print(f"replacement_retired_pages={len(retired_pages)}")
    print(f"old_object_offset={old_object_offset}")
    print(f"new_object_offset={new_object_offset}")
    print(f"historical_object_corruption={corruption_error}")
    print(f"authenticated_overlap_rejection={overlap_error}")
    print("deterministic_replacement=pass")
    print("object_page_structural_overlap_checks=pass")
    print("interrupted_replacement_previous_prefix=valid")
    print("finding=immutable pages can authenticate real current and historical object records")
    print("finding=object replacement appends one new object and one page per directory level")
    print("finding=current commit hashing plus object/page digests detects historical-byte mutation")


if __name__ == "__main__":
    main()
