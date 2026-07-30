#!/usr/bin/env python3
"""Byte-level immutable-page copy-on-write prototype for a successor to Candidate 1."""

from __future__ import annotations

import hashlib
import struct
from dataclasses import dataclass
from math import ceil

FILE_HEADER_LEN = 64
PAGE_SIZE = 16 * 1024
PAGE_HEADER_LEN = 64
LEAF_ENTRY_LEN = 88
INTERNAL_ENTRY_LEN = 64
SNAPSHOT_LEN = 96
FOOTER_LEN = 128
ABSENT_OFFSET = (1 << 64) - 1

FILE_MAGIC = b"UCOFIM02"
PAGE_MAGIC = b"UCPGIM02"
SNAPSHOT_MAGIC = b"UCSNIM02"
FOOTER_MAGIC = b"UCFTIM02"

PAGE_DOMAIN = b"UCOF-IMMUTABLE-PAGE\x00"
SNAPSHOT_DOMAIN = b"UCOF-IMMUTABLE-SNAPSHOT\x00"
COMMIT_DOMAIN = b"UCOF-IMMUTABLE-COMMIT\x00"
OBJECT_DOMAIN = b"UCOF-IMMUTABLE-OBJECT\x00"

PAGE_HEADER = struct.Struct("<8sBBHIIQQ28s")
LEAF_ENTRY = struct.Struct("<QH6sQQQ32s16s")
INTERNAL_ENTRY = struct.Struct("<QQQQ32s")
SNAPSHOT = struct.Struct("<8sQQQ32s32s")
FOOTER = struct.Struct("<8sQQQQQ32s32s16s")

OBJECTS = 100_000
LEAF_CAPACITY = (PAGE_SIZE - PAGE_HEADER_LEN) // LEAF_ENTRY_LEN
INTERNAL_FANOUT = (PAGE_SIZE - PAGE_HEADER_LEN) // INTERNAL_ENTRY_LEN


class FormatError(ValueError):
    pass


@dataclass(frozen=True)
class Locator:
    object_id: int
    kind: int
    record_offset: int
    record_len: int
    logical_len: int
    digest: bytes


@dataclass(frozen=True)
class PageRef:
    minimum: int
    maximum: int
    offset: int
    level: int
    digest: bytes


@dataclass(frozen=True)
class FooterRecord:
    sequence: int
    snapshot_offset: int
    snapshot_len: int
    previous_footer_offset: int
    page_count_current: int
    snapshot_digest: bytes
    commit_digest: bytes


@dataclass(frozen=True)
class ValidationReport:
    sequence: int
    footer_offset: int
    root: PageRef
    snapshot_digest: bytes
    commit_digest: bytes
    reachable_pages: frozenset[int]


def digest(domain: bytes, data: bytes) -> bytes:
    return hashlib.sha256(domain + data).digest()


def checked_slice(data: bytes, offset: int, length: int, label: str) -> bytes:
    if offset < 0 or length < 0 or offset + length > len(data):
        raise FormatError(f"{label} range")
    return data[offset : offset + length]


def locator(object_id: int, generation: int = 0) -> Locator:
    payload = f"object:{object_id}:generation:{generation}".encode("ascii")
    return Locator(
        object_id=object_id,
        kind=1,
        record_offset=object_id * 256 + generation * 16,
        record_len=128,
        logical_len=80,
        digest=digest(OBJECT_DOMAIN, payload),
    )


def encode_leaf(entries: list[Locator]) -> bytes:
    if not entries or len(entries) > LEAF_CAPACITY:
        raise ValueError("invalid leaf entry count")
    if any(left.object_id >= right.object_id for left, right in zip(entries, entries[1:])):
        raise ValueError("leaf identifiers must be strictly ordered")
    body = bytearray(PAGE_SIZE)
    PAGE_HEADER.pack_into(
        body,
        0,
        PAGE_MAGIC,
        1,
        0,
        0,
        len(entries),
        LEAF_ENTRY_LEN,
        entries[0].object_id,
        entries[-1].object_id,
        bytes(28),
    )
    for index, entry in enumerate(entries):
        LEAF_ENTRY.pack_into(
            body,
            PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN,
            entry.object_id,
            entry.kind,
            bytes(6),
            entry.record_offset,
            entry.record_len,
            entry.logical_len,
            entry.digest,
            bytes(16),
        )
    return bytes(body)


def encode_internal(children: list[PageRef], level: int) -> bytes:
    if not children or len(children) > INTERNAL_FANOUT or level == 0:
        raise ValueError("invalid internal page")
    if any(left.maximum >= right.minimum for left, right in zip(children, children[1:])):
        raise ValueError("child ranges overlap")
    if any(child.level + 1 != level for child in children):
        raise ValueError("child level mismatch")
    body = bytearray(PAGE_SIZE)
    PAGE_HEADER.pack_into(
        body,
        0,
        PAGE_MAGIC,
        2,
        level,
        0,
        len(children),
        INTERNAL_ENTRY_LEN,
        children[0].minimum,
        children[-1].maximum,
        bytes(28),
    )
    for index, child in enumerate(children):
        INTERNAL_ENTRY.pack_into(
            body,
            PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN,
            child.minimum,
            child.maximum,
            child.offset,
            PAGE_SIZE,
            child.digest,
        )
    return bytes(body)


def append_page(output: bytearray, page: bytes) -> PageRef:
    if len(page) != PAGE_SIZE:
        raise ValueError("wrong page size")
    magic, kind, level, reserved, count, entry_size, minimum, maximum, tail = PAGE_HEADER.unpack_from(page)
    if magic != PAGE_MAGIC or reserved != 0 or any(tail) or count == 0:
        raise ValueError("invalid page header")
    expected = LEAF_ENTRY_LEN if kind == 1 else INTERNAL_ENTRY_LEN
    if entry_size != expected:
        raise ValueError("entry size")
    offset = len(output)
    output.extend(page)
    return PageRef(minimum, maximum, offset, level, digest(PAGE_DOMAIN, page))


def build_tree(output: bytearray, entries: list[Locator]) -> PageRef:
    leaves = [
        append_page(output, encode_leaf(entries[start : start + LEAF_CAPACITY]))
        for start in range(0, len(entries), LEAF_CAPACITY)
    ]
    current = leaves
    level = 1
    while len(current) > 1:
        current = [
            append_page(output, encode_internal(current[start : start + INTERNAL_FANOUT], level))
            for start in range(0, len(current), INTERNAL_FANOUT)
        ]
        level += 1
    return current[0]


def encode_snapshot(sequence: int, root: PageRef, parent_snapshot_digest: bytes) -> bytes:
    return SNAPSHOT.pack(
        SNAPSHOT_MAGIC,
        sequence,
        root.offset,
        root.level,
        root.digest,
        parent_snapshot_digest,
    )


def footer_semantics(
    sequence: int,
    snapshot_offset: int,
    snapshot_len: int,
    previous_footer_offset: int,
    page_count_current: int,
    snapshot_digest: bytes,
) -> bytes:
    return struct.pack(
        "<QQQQQ32s",
        sequence,
        snapshot_offset,
        snapshot_len,
        previous_footer_offset,
        page_count_current,
        snapshot_digest,
    )


def publish(
    output: bytearray,
    sequence: int,
    root: PageRef,
    parent_snapshot_digest: bytes,
    previous_footer_offset: int,
    page_count_current: int,
) -> None:
    snapshot = encode_snapshot(sequence, root, parent_snapshot_digest)
    snapshot_offset = len(output)
    output.extend(snapshot)
    snapshot_digest = digest(SNAPSHOT_DOMAIN, snapshot)
    footer_offset = len(output)
    commit_start = 0 if previous_footer_offset == ABSENT_OFFSET else previous_footer_offset + FOOTER_LEN
    semantics = footer_semantics(
        sequence,
        snapshot_offset,
        len(snapshot),
        previous_footer_offset,
        page_count_current,
        snapshot_digest,
    )
    commit_digest = digest(COMMIT_DOMAIN, bytes(output[commit_start:footer_offset]) + semantics)
    output.extend(
        FOOTER.pack(
            FOOTER_MAGIC,
            sequence,
            snapshot_offset,
            len(snapshot),
            previous_footer_offset,
            page_count_current,
            snapshot_digest,
            commit_digest,
            bytes(16),
        )
    )


def build_genesis(entries: list[Locator]) -> bytes:
    if not entries:
        raise ValueError("empty directory")
    output = bytearray(FILE_HEADER_LEN)
    output[: len(FILE_MAGIC)] = FILE_MAGIC
    root = build_tree(output, entries)
    page_count = (len(output) - FILE_HEADER_LEN) // PAGE_SIZE
    publish(output, 0, root, bytes(32), ABSENT_OFFSET, page_count)
    return bytes(output)


def parse_footer(data: bytes, offset: int) -> FooterRecord:
    raw = checked_slice(data, offset, FOOTER_LEN, "footer")
    (
        magic,
        sequence,
        snapshot_offset,
        snapshot_len,
        previous_footer_offset,
        page_count_current,
        snapshot_digest,
        commit_digest,
        reserved,
    ) = FOOTER.unpack(raw)
    if magic != FOOTER_MAGIC or any(reserved):
        raise FormatError("footer")
    return FooterRecord(
        sequence,
        snapshot_offset,
        snapshot_len,
        previous_footer_offset,
        page_count_current,
        snapshot_digest,
        commit_digest,
    )


def decode_page(data: bytes, reference: PageRef) -> tuple[int, list[Locator] | list[PageRef]]:
    page = checked_slice(data, reference.offset, PAGE_SIZE, "page")
    if digest(PAGE_DOMAIN, page) != reference.digest:
        raise FormatError("page digest")
    magic, kind, level, reserved, count, entry_size, minimum, maximum, tail = PAGE_HEADER.unpack_from(page)
    if magic != PAGE_MAGIC or reserved != 0 or any(tail) or count == 0:
        raise FormatError("page header")
    if (minimum, maximum, level) != (reference.minimum, reference.maximum, reference.level):
        raise FormatError("page reference")
    if kind == 1:
        if level != 0 or entry_size != LEAF_ENTRY_LEN or count > LEAF_CAPACITY:
            raise FormatError("leaf shape")
        entries: list[Locator] = []
        for index in range(count):
            unpacked = LEAF_ENTRY.unpack_from(page, PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN)
            object_id, object_kind, pad, record_offset, record_len, logical_len, object_digest, tail_pad = unpacked
            if object_id == 0 or object_kind == 0 or any(pad) or any(tail_pad):
                raise FormatError("leaf entry")
            entries.append(Locator(object_id, object_kind, record_offset, record_len, logical_len, object_digest))
        if any(left.object_id >= right.object_id for left, right in zip(entries, entries[1:])):
            raise FormatError("leaf order")
        if (entries[0].object_id, entries[-1].object_id) != (minimum, maximum):
            raise FormatError("leaf range")
        used = PAGE_HEADER_LEN + count * LEAF_ENTRY_LEN
        if any(page[used:]):
            raise FormatError("leaf padding")
        return kind, entries
    if kind == 2:
        if level == 0 or entry_size != INTERNAL_ENTRY_LEN or count > INTERNAL_FANOUT:
            raise FormatError("internal shape")
        children: list[PageRef] = []
        for index in range(count):
            child_min, child_max, child_offset, child_len, child_digest = INTERNAL_ENTRY.unpack_from(
                page, PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN
            )
            if child_len != PAGE_SIZE or child_min > child_max:
                raise FormatError("child entry")
            children.append(PageRef(child_min, child_max, child_offset, level - 1, child_digest))
        if any(left.maximum >= right.minimum for left, right in zip(children, children[1:])):
            raise FormatError("child order")
        if (children[0].minimum, children[-1].maximum) != (minimum, maximum):
            raise FormatError("internal range")
        used = PAGE_HEADER_LEN + count * INTERNAL_ENTRY_LEN
        if any(page[used:]):
            raise FormatError("internal padding")
        return kind, children
    raise FormatError("page kind")


def validate_strict(data: bytes) -> ValidationReport:
    if len(data) < FILE_HEADER_LEN + PAGE_SIZE + SNAPSHOT_LEN + FOOTER_LEN:
        raise FormatError("file too short")
    header = checked_slice(data, 0, FILE_HEADER_LEN, "header")
    if header[: len(FILE_MAGIC)] != FILE_MAGIC or any(header[len(FILE_MAGIC) :]):
        raise FormatError("header")
    footer_offset = len(data) - FOOTER_LEN
    footer = parse_footer(data, footer_offset)
    if footer.snapshot_len != SNAPSHOT_LEN or footer.snapshot_offset + SNAPSHOT_LEN != footer_offset:
        raise FormatError("snapshot locator")
    snapshot = checked_slice(data, footer.snapshot_offset, SNAPSHOT_LEN, "snapshot")
    if digest(SNAPSHOT_DOMAIN, snapshot) != footer.snapshot_digest:
        raise FormatError("snapshot digest")
    magic, sequence, root_offset, root_level, root_digest, parent_snapshot_digest = SNAPSHOT.unpack(snapshot)
    if magic != SNAPSHOT_MAGIC or sequence != footer.sequence:
        raise FormatError("snapshot")
    if footer.previous_footer_offset == ABSENT_OFFSET:
        if sequence != 0 or any(parent_snapshot_digest):
            raise FormatError("genesis linkage")
        commit_start = 0
    else:
        if footer.previous_footer_offset + FOOTER_LEN > footer.snapshot_offset:
            raise FormatError("previous footer")
        parent = parse_footer(data, footer.previous_footer_offset)
        if sequence != parent.sequence + 1 or parent.snapshot_digest != parent_snapshot_digest:
            raise FormatError("parent linkage")
        commit_start = footer.previous_footer_offset + FOOTER_LEN
    semantics = footer_semantics(
        footer.sequence,
        footer.snapshot_offset,
        footer.snapshot_len,
        footer.previous_footer_offset,
        footer.page_count_current,
        footer.snapshot_digest,
    )
    expected_commit = digest(COMMIT_DOMAIN, data[commit_start:footer_offset] + semantics)
    if expected_commit != footer.commit_digest:
        raise FormatError("commit digest")

    root = PageRef(1, OBJECTS, root_offset, root_level, root_digest)
    stack = [root]
    seen: set[int] = set()
    while stack:
        reference = stack.pop()
        if reference.offset in seen:
            raise FormatError("page cycle")
        if reference.offset < FILE_HEADER_LEN or reference.offset + PAGE_SIZE > footer.snapshot_offset:
            raise FormatError("page range")
        seen.add(reference.offset)
        kind, entries = decode_page(data, reference)
        if kind == 2:
            assert isinstance(entries, list)
            for child in reversed(entries):
                if not isinstance(child, PageRef):
                    raise AssertionError("internal decode")
                stack.append(child)

    return ValidationReport(
        sequence,
        footer_offset,
        root,
        footer.snapshot_digest,
        footer.commit_digest,
        frozenset(seen),
    )


def rewrite_node(output: bytearray, source: bytes, reference: PageRef, updates: dict[int, Locator]) -> PageRef:
    relevant = {key: value for key, value in updates.items() if reference.minimum <= key <= reference.maximum}
    if not relevant:
        return reference
    kind, entries = decode_page(source, reference)
    if kind == 1:
        locators = []
        found: set[int] = set()
        for entry in entries:
            if not isinstance(entry, Locator):
                raise AssertionError("leaf decode")
            replacement = relevant.get(entry.object_id)
            if replacement is None:
                locators.append(entry)
            else:
                locators.append(replacement)
                found.add(entry.object_id)
        if found != set(relevant):
            raise ValueError("update identifier is absent")
        return append_page(output, encode_leaf(locators))

    children: list[PageRef] = []
    changed = False
    for child in entries:
        if not isinstance(child, PageRef):
            raise AssertionError("internal decode")
        replacement = rewrite_node(output, source, child, relevant)
        children.append(replacement)
        changed |= replacement != child
    if not changed:
        return reference
    return append_page(output, encode_internal(children, reference.level))


def append_updates(source: bytes, updates: list[Locator]) -> bytes:
    verified = validate_strict(source)
    ordered = sorted(updates, key=lambda item: item.object_id)
    if any(left.object_id == right.object_id for left, right in zip(ordered, ordered[1:])):
        raise ValueError("duplicate updates")
    output = bytearray(source)
    page_start = len(output)
    root = rewrite_node(output, source, verified.root, {entry.object_id: entry for entry in ordered})
    if root == verified.root:
        raise ValueError("empty update")
    page_count = (len(output) - page_start) // PAGE_SIZE
    publish(
        output,
        verified.sequence + 1,
        root,
        verified.snapshot_digest,
        verified.footer_offset,
        page_count,
    )
    return bytes(output)


def main() -> None:
    entries = [locator(object_id) for object_id in range(1, OBJECTS + 1)]
    genesis = build_genesis(entries)
    genesis_report = validate_strict(genesis)

    update = locator(50_000, generation=1)
    appended = append_updates(genesis, [update])
    appended_report = validate_strict(appended)

    batch = [locator(25_000, generation=1), locator(75_000, generation=1)]
    batched = append_updates(genesis, batch)
    batched_reverse = append_updates(genesis, list(reversed(batch)))
    assert batched == batched_reverse
    batched_report = validate_strict(batched)

    expected_levels = [ceil(OBJECTS / LEAF_CAPACITY)]
    while expected_levels[-1] > 1:
        expected_levels.append(ceil(expected_levels[-1] / INTERNAL_FANOUT))
    expected_pages = sum(expected_levels)
    depth = len(expected_levels)
    assert len(genesis_report.reachable_pages) == expected_pages
    assert appended_report.sequence == 1
    assert appended_report.footer_offset > genesis_report.footer_offset

    new_pages = appended_report.reachable_pages - genesis_report.reachable_pages
    reused_pages = appended_report.reachable_pages & genesis_report.reachable_pages
    retired_pages = genesis_report.reachable_pages - appended_report.reachable_pages
    assert len(new_pages) == depth
    assert len(retired_pages) == depth
    assert len(reused_pages) == expected_pages - depth

    batch_new_pages = batched_report.reachable_pages - genesis_report.reachable_pages
    batch_reused_pages = batched_report.reachable_pages & genesis_report.reachable_pages
    batch_retired_pages = genesis_report.reachable_pages - batched_report.reachable_pages
    expected_batch_pages = 5  # two leaves, two level-1 parents, one root
    assert len(batch_new_pages) == expected_batch_pages
    assert len(batch_retired_pages) == expected_batch_pages
    assert len(batch_reused_pages) == expected_pages - expected_batch_pages

    # Historical reused pages lie outside the current commit digest, so their
    # own page digest must still detect mutation.
    reused_offset = min(reused_pages)
    corrupted = bytearray(appended)
    corrupted[reused_offset + PAGE_HEADER_LEN] ^= 1
    try:
        validate_strict(bytes(corrupted))
    except FormatError as error:
        corruption_error = str(error)
    else:
        raise AssertionError("reused-page corruption was not detected")
    assert corruption_error == "page digest"

    # An interrupted new footer does not publish the append; the previous
    # exact prefix remains strictly valid.
    interrupted = appended[:-FOOTER_LEN // 2]
    try:
        validate_strict(interrupted)
    except FormatError:
        pass
    else:
        raise AssertionError("interrupted append unexpectedly validated")
    validate_strict(genesis)

    full_rebuild_page_bytes = expected_pages * PAGE_SIZE
    cow_page_bytes = depth * PAGE_SIZE
    print(f"objects={OBJECTS:,}")
    print(f"tree_level_counts={tuple(expected_levels)}")
    print(f"tree_depth={depth}")
    print(f"genesis_reachable_pages={expected_pages:,}")
    print(f"append_new_pages={len(new_pages)}")
    print(f"append_reused_pages={len(reused_pages):,}")
    print(f"append_retired_pages={len(retired_pages)}")
    print(f"batch_new_pages={len(batch_new_pages)}")
    print(f"batch_reused_pages={len(batch_reused_pages):,}")
    print(f"batch_retired_pages={len(batch_retired_pages)}")
    print(f"full_rebuild_page_bytes={full_rebuild_page_bytes:,}")
    print(f"cow_page_bytes={cow_page_bytes:,}")
    print(f"write_amplification={full_rebuild_page_bytes / cow_page_bytes:.2f}x")
    print(f"reused_page_corruption={corruption_error}")
    print("interrupted_append_previous_prefix=valid")
    print("deterministic_two_leaf_batch_order=pass")
    print("finding=immutable page bytes permit exact mixed-age copy-on-write traversal")


if __name__ == "__main__":
    main()
