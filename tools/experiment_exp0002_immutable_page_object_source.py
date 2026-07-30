#!/usr/bin/env python3
"""Bounded source validation and targeted lookup for immutable-page objects."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass, replace

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_objects as objects

OBJECTS = 2_000
LARGE_OBJECT_ID = 1_500
LARGE_PAYLOAD_BYTES = 1024 * 1024


class SourceError(cow.FormatError):
    pass


@dataclass(frozen=True)
class SourceLimits:
    max_total_bytes_read: int = 64 * 1024 * 1024
    max_read_operations: int = 100_000
    max_read_request_bytes: int = cow.PAGE_SIZE
    hash_block_bytes: int = 4 * 1024
    max_pages: int = 100_000
    max_objects: int = 100_000


@dataclass
class SourceStats:
    read_operations: int = 0
    bytes_read: int = 0
    largest_request: int = 0
    commit_bytes_hashed: int = 0
    object_bytes_hashed: int = 0
    pages_read: int = 0
    objects_hashed: int = 0
    payload_bytes_materialized: int = 0


class CountingSource:
    def __init__(self, data: bytes, limits: SourceLimits) -> None:
        self.data = data
        self.limits = limits
        self.stats = SourceStats()
        self.ranges: list[tuple[int, int, str]] = []

    def __len__(self) -> int:
        return len(self.data)

    def read_exact(self, offset: int, length: int, label: str) -> bytes:
        if offset < 0 or length < 0 or offset + length > len(self.data):
            raise SourceError(f"{label} range")
        if length > self.limits.max_read_request_bytes:
            raise SourceError("source request limit")
        if self.stats.read_operations >= self.limits.max_read_operations:
            raise SourceError("source operation limit")
        if self.stats.bytes_read + length > self.limits.max_total_bytes_read:
            raise SourceError("source byte budget")
        self.stats.read_operations += 1
        self.stats.bytes_read += length
        self.stats.largest_request = max(self.stats.largest_request, length)
        self.ranges.append((offset, offset + length, label))
        return self.data[offset : offset + length]


@dataclass(frozen=True)
class SourceContext:
    footer_offset: int
    footer: cow.FooterRecord
    snapshot: bytes
    root: cow.PageRef
    root_page: bytes
    root_entries: list[cow.Locator] | list[cow.PageRef]
    commit_start: int


@dataclass(frozen=True)
class LookupReport:
    sequence: int
    object_id: int
    payload: bytes | None
    found: bool
    path_pages: int
    stats: SourceStats


@dataclass(frozen=True)
class StrictReport:
    sequence: int
    object_count: int
    page_count: int
    stats: SourceStats


def parse_footer_bytes(raw: bytes) -> cow.FooterRecord:
    if len(raw) != cow.FOOTER_LEN:
        raise SourceError("footer length")
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
    ) = cow.FOOTER.unpack(raw)
    if magic != cow.FOOTER_MAGIC or any(reserved):
        raise SourceError("footer")
    return cow.FooterRecord(
        sequence,
        snapshot_offset,
        snapshot_len,
        previous_footer_offset,
        page_count_current,
        snapshot_digest,
        commit_digest,
    )


def decode_page_bytes(
    page: bytes, reference: cow.PageRef
) -> tuple[int, list[cow.Locator] | list[cow.PageRef]]:
    if len(page) != cow.PAGE_SIZE:
        raise SourceError("page length")
    if cow.digest(cow.PAGE_DOMAIN, page) != reference.digest:
        raise SourceError("page digest")
    magic, kind, level, reserved, count, entry_size, minimum, maximum, tail = (
        cow.PAGE_HEADER.unpack_from(page)
    )
    if magic != cow.PAGE_MAGIC or reserved != 0 or any(tail) or count == 0:
        raise SourceError("page header")
    if (minimum, maximum, level) != (
        reference.minimum,
        reference.maximum,
        reference.level,
    ):
        raise SourceError("page reference")

    if kind == 1:
        if level != 0 or entry_size != cow.LEAF_ENTRY_LEN or count > cow.LEAF_CAPACITY:
            raise SourceError("leaf shape")
        entries: list[cow.Locator] = []
        for index in range(count):
            (
                object_id,
                object_kind,
                padding,
                record_offset,
                record_len,
                logical_len,
                object_digest,
                tail_padding,
            ) = cow.LEAF_ENTRY.unpack_from(
                page, cow.PAGE_HEADER_LEN + index * cow.LEAF_ENTRY_LEN
            )
            if object_id == 0 or object_kind == 0 or any(padding) or any(tail_padding):
                raise SourceError("leaf entry")
            entries.append(
                cow.Locator(
                    object_id,
                    object_kind,
                    record_offset,
                    record_len,
                    logical_len,
                    object_digest,
                )
            )
        if any(
            left.object_id >= right.object_id
            for left, right in zip(entries, entries[1:])
        ):
            raise SourceError("leaf order")
        if (entries[0].object_id, entries[-1].object_id) != (minimum, maximum):
            raise SourceError("leaf range")
        used = cow.PAGE_HEADER_LEN + count * cow.LEAF_ENTRY_LEN
        if any(page[used:]):
            raise SourceError("leaf padding")
        return kind, entries

    if kind == 2:
        if level == 0 or entry_size != cow.INTERNAL_ENTRY_LEN or count > cow.INTERNAL_FANOUT:
            raise SourceError("internal shape")
        children: list[cow.PageRef] = []
        for index in range(count):
            child_min, child_max, child_offset, child_len, child_digest = (
                cow.INTERNAL_ENTRY.unpack_from(
                    page, cow.PAGE_HEADER_LEN + index * cow.INTERNAL_ENTRY_LEN
                )
            )
            if child_len != cow.PAGE_SIZE or child_min > child_max:
                raise SourceError("child entry")
            children.append(
                cow.PageRef(
                    child_min,
                    child_max,
                    child_offset,
                    level - 1,
                    child_digest,
                )
            )
        if any(
            left.maximum >= right.minimum
            for left, right in zip(children, children[1:])
        ):
            raise SourceError("child order")
        if (children[0].minimum, children[-1].maximum) != (minimum, maximum):
            raise SourceError("internal range")
        used = cow.PAGE_HEADER_LEN + count * cow.INTERNAL_ENTRY_LEN
        if any(page[used:]):
            raise SourceError("internal padding")
        return kind, children

    raise SourceError("page kind")


def read_page(
    source: CountingSource, reference: cow.PageRef
) -> tuple[bytes, int, list[cow.Locator] | list[cow.PageRef]]:
    page = source.read_exact(reference.offset, cow.PAGE_SIZE, "page")
    source.stats.pages_read += 1
    kind, entries = decode_page_bytes(page, reference)
    return page, kind, entries


def read_context(source: CountingSource) -> SourceContext:
    minimum = (
        cow.FILE_HEADER_LEN
        + objects.OBJECT_HEADER_LEN
        + cow.PAGE_SIZE
        + cow.SNAPSHOT_LEN
        + cow.FOOTER_LEN
    )
    if len(source) < minimum:
        raise SourceError("file too short")
    header = source.read_exact(0, cow.FILE_HEADER_LEN, "header")
    if header[: len(cow.FILE_MAGIC)] != cow.FILE_MAGIC or any(
        header[len(cow.FILE_MAGIC) :]
    ):
        raise SourceError("header")

    footer_offset = len(source) - cow.FOOTER_LEN
    footer = parse_footer_bytes(
        source.read_exact(footer_offset, cow.FOOTER_LEN, "footer")
    )
    if (
        footer.snapshot_len != cow.SNAPSHOT_LEN
        or footer.snapshot_offset + footer.snapshot_len != footer_offset
    ):
        raise SourceError("snapshot locator")
    snapshot = source.read_exact(
        footer.snapshot_offset, footer.snapshot_len, "snapshot"
    )
    if cow.digest(cow.SNAPSHOT_DOMAIN, snapshot) != footer.snapshot_digest:
        raise SourceError("snapshot digest")
    (
        magic,
        sequence,
        root_offset,
        root_level,
        root_digest,
        parent_snapshot_digest,
    ) = cow.SNAPSHOT.unpack(snapshot)
    if magic != cow.SNAPSHOT_MAGIC or sequence != footer.sequence:
        raise SourceError("snapshot")

    if footer.previous_footer_offset == cow.ABSENT_OFFSET:
        if sequence != 0 or any(parent_snapshot_digest):
            raise SourceError("genesis linkage")
        commit_start = 0
    else:
        if footer.previous_footer_offset + cow.FOOTER_LEN > footer.snapshot_offset:
            raise SourceError("previous footer")
        parent = parse_footer_bytes(
            source.read_exact(
                footer.previous_footer_offset, cow.FOOTER_LEN, "parent footer"
            )
        )
        if (
            sequence != parent.sequence + 1
            or parent.snapshot_digest != parent_snapshot_digest
        ):
            raise SourceError("parent linkage")
        commit_start = footer.previous_footer_offset + cow.FOOTER_LEN

    if source.limits.hash_block_bytes <= 0:
        raise SourceError("hash block limit")
    hasher = hashlib.sha256()
    hasher.update(cow.COMMIT_DOMAIN)
    cursor = commit_start
    while cursor < footer_offset:
        take = min(
            footer_offset - cursor,
            source.limits.hash_block_bytes,
            source.limits.max_read_request_bytes,
        )
        block = source.read_exact(cursor, take, "commit")
        source.stats.commit_bytes_hashed += len(block)
        hasher.update(block)
        cursor += len(block)
    hasher.update(
        cow.footer_semantics(
            footer.sequence,
            footer.snapshot_offset,
            footer.snapshot_len,
            footer.previous_footer_offset,
            footer.page_count_current,
            footer.snapshot_digest,
        )
    )
    if hasher.digest() != footer.commit_digest:
        raise SourceError("commit digest")

    root_page = source.read_exact(root_offset, cow.PAGE_SIZE, "root page")
    source.stats.pages_read += 1
    if cow.digest(cow.PAGE_DOMAIN, root_page) != root_digest:
        raise SourceError("page digest")
    fields = cow.PAGE_HEADER.unpack_from(root_page)
    root = cow.PageRef(fields[6], fields[7], root_offset, root_level, root_digest)
    _kind, root_entries = decode_page_bytes(root_page, root)
    return SourceContext(
        footer_offset,
        footer,
        snapshot,
        root,
        root_page,
        root_entries,
        commit_start,
    )


def validate_object_range(
    locator: cow.Locator,
    snapshot_offset: int,
    structural_ranges: list[tuple[int, int]],
) -> None:
    end = locator.record_offset + locator.record_len
    if locator.record_offset < cow.FILE_HEADER_LEN or end > snapshot_offset:
        raise SourceError("object range")
    for start, stop in structural_ranges:
        if locator.record_offset < stop and start < end:
            raise SourceError("object structural overlap")


def read_object(
    source: CountingSource,
    locator: cow.Locator,
    *,
    materialize_payload: bool,
) -> bytes:
    if locator.record_len < objects.OBJECT_HEADER_LEN:
        raise SourceError("object header")
    header = source.read_exact(
        locator.record_offset, objects.OBJECT_HEADER_LEN, "object header"
    )
    (
        magic,
        header_len,
        kind,
        flags,
        object_id,
        payload_len,
        logical_len,
        reserved,
    ) = objects.OBJECT_HEADER.unpack(header)
    if (
        magic != objects.OBJECT_MAGIC
        or header_len != objects.OBJECT_HEADER_LEN
        or kind == 0
        or flags != 0
        or object_id == 0
        or any(reserved)
    ):
        raise SourceError("object header")
    if (
        payload_len != logical_len
        or objects.OBJECT_HEADER_LEN + payload_len != locator.record_len
    ):
        raise SourceError("object length")
    if (
        object_id != locator.object_id
        or kind != locator.kind
        or logical_len != locator.logical_len
    ):
        raise SourceError("object locator")

    hasher = hashlib.sha256()
    hasher.update(cow.OBJECT_DOMAIN)
    hasher.update(header)
    source.stats.object_bytes_hashed += len(header)
    payload = bytearray() if materialize_payload else None
    cursor = locator.record_offset + objects.OBJECT_HEADER_LEN
    remaining = payload_len
    while remaining:
        take = min(
            remaining,
            source.limits.hash_block_bytes,
            source.limits.max_read_request_bytes,
        )
        block = source.read_exact(cursor, take, "object payload")
        hasher.update(block)
        source.stats.object_bytes_hashed += len(block)
        if payload is not None:
            payload.extend(block)
        cursor += len(block)
        remaining -= len(block)
    if hasher.digest() != locator.digest:
        raise SourceError("object digest")
    source.stats.objects_hashed += 1
    if payload is None:
        return b""
    source.stats.payload_bytes_materialized += len(payload)
    return bytes(payload)


def snapshot_stats(stats: SourceStats) -> SourceStats:
    return replace(stats)


def targeted_lookup(source: CountingSource, object_id: int) -> LookupReport:
    context = read_context(source)
    structural_ranges = [
        (context.footer.snapshot_offset, context.footer_offset),
        (context.footer_offset, len(source)),
        (context.root.offset, context.root.offset + cow.PAGE_SIZE),
    ]
    reference = context.root
    entries = context.root_entries
    path_pages = 1

    if object_id < reference.minimum or object_id > reference.maximum:
        return LookupReport(
            context.footer.sequence,
            object_id,
            None,
            False,
            path_pages,
            snapshot_stats(source.stats),
        )

    while reference.level > 0:
        children = [entry for entry in entries if isinstance(entry, cow.PageRef)]
        child = next(
            (
                candidate
                for candidate in children
                if candidate.minimum <= object_id <= candidate.maximum
            ),
            None,
        )
        if child is None:
            return LookupReport(
                context.footer.sequence,
                object_id,
                None,
                False,
                path_pages,
                snapshot_stats(source.stats),
            )
        reference = child
        _page, _kind, entries = read_page(source, reference)
        path_pages += 1
        structural_ranges.append(
            (reference.offset, reference.offset + cow.PAGE_SIZE)
        )

    locators = [entry for entry in entries if isinstance(entry, cow.Locator)]
    locator = next(
        (entry for entry in locators if entry.object_id == object_id), None
    )
    if locator is None:
        return LookupReport(
            context.footer.sequence,
            object_id,
            None,
            False,
            path_pages,
            snapshot_stats(source.stats),
        )
    validate_object_range(
        locator, context.footer.snapshot_offset, structural_ranges
    )
    payload = read_object(source, locator, materialize_payload=True)
    return LookupReport(
        context.footer.sequence,
        object_id,
        payload,
        True,
        path_pages,
        snapshot_stats(source.stats),
    )


def strict_validate(source: CountingSource) -> StrictReport:
    context = read_context(source)
    stack: list[
        tuple[
            cow.PageRef,
            list[cow.Locator] | list[cow.PageRef] | None,
        ]
    ] = [(context.root, context.root_entries)]
    seen: set[int] = set()
    locators: list[cow.Locator] = []
    structural_ranges: list[tuple[int, int]] = [
        (context.footer.snapshot_offset, context.footer_offset),
        (context.footer_offset, len(source)),
    ]

    while stack:
        reference, cached = stack.pop()
        if reference.offset in seen:
            raise SourceError("page cycle")
        if len(seen) >= source.limits.max_pages:
            raise SourceError("page limit")
        if (
            reference.offset < cow.FILE_HEADER_LEN
            or reference.offset + cow.PAGE_SIZE > context.footer.snapshot_offset
        ):
            raise SourceError("page range")
        seen.add(reference.offset)
        structural_ranges.append(
            (reference.offset, reference.offset + cow.PAGE_SIZE)
        )
        if cached is None:
            _page, kind, entries = read_page(source, reference)
        else:
            fields = cow.PAGE_HEADER.unpack_from(context.root_page)
            kind = fields[1]
            entries = cached
        if kind == 1:
            locators.extend(
                entry for entry in entries if isinstance(entry, cow.Locator)
            )
        else:
            children = [
                entry for entry in entries if isinstance(entry, cow.PageRef)
            ]
            stack.extend((child, None) for child in reversed(children))

    if not locators or len(locators) > source.limits.max_objects:
        raise SourceError("object limit")
    if any(
        left.object_id >= right.object_id
        for left, right in zip(locators, locators[1:])
    ):
        raise SourceError("object order")
    if (locators[0].object_id, locators[-1].object_id) != (
        context.root.minimum,
        context.root.maximum,
    ):
        raise SourceError("root object range")

    ranges: list[tuple[int, int, int]] = []
    for locator in locators:
        validate_object_range(
            locator, context.footer.snapshot_offset, structural_ranges
        )
        ranges.append(
            (
                locator.record_offset,
                locator.record_offset + locator.record_len,
                locator.object_id,
            )
        )
    for left, right in zip(sorted(ranges), sorted(ranges)[1:]):
        if left[1] > right[0]:
            raise SourceError("object overlap")

    for locator in locators:
        read_object(source, locator, materialize_payload=False)

    return StrictReport(
        context.footer.sequence,
        len(locators),
        len(seen),
        snapshot_stats(source.stats),
    )


def overlaps(
    ranges: list[tuple[int, int, str]], start: int, stop: int
) -> bool:
    return any(left < stop and start < right for left, right, _label in ranges)


def main() -> None:
    values = [
        objects.ObjectInput(
            object_id,
            1 + object_id % 3,
            (
                bytes([object_id % 251]) * LARGE_PAYLOAD_BYTES
                if object_id == LARGE_OBJECT_ID
                else f"payload:{object_id}".encode("ascii")
            ),
        )
        for object_id in range(1, OBJECTS + 1)
    ]
    genesis = objects.build_genesis(values)
    appended = objects.append_replacement(
        genesis,
        objects.ObjectInput(1, 9, b"small latest replacement"),
    )
    expected = objects.validate_complete(appended)
    large = next(
        locator
        for locator in expected.objects
        if locator.object_id == LARGE_OBJECT_ID
    )
    large_start = large.record_offset
    large_stop = large.record_offset + large.record_len

    limits = SourceLimits()
    target_source = CountingSource(appended, limits)
    target = targeted_lookup(target_source, 1)
    assert target.found and target.payload == b"small latest replacement"
    assert target.stats.payload_bytes_materialized == len(target.payload)
    assert target.stats.largest_request <= limits.max_read_request_bytes
    assert not overlaps(target_source.ranges, large_start, large_stop)

    absent_source = CountingSource(appended, limits)
    absent = targeted_lookup(absent_source, OBJECTS + 10_000)
    assert not absent.found and absent.payload is None
    assert not overlaps(absent_source.ranges, large_start, large_stop)

    strict_source = CountingSource(appended, limits)
    strict = strict_validate(strict_source)
    assert strict.object_count == OBJECTS
    assert strict.stats.objects_hashed == OBJECTS
    assert strict.stats.payload_bytes_materialized == 0
    assert overlaps(strict_source.ranges, large_start, large_stop)
    assert strict.stats.bytes_read > target.stats.bytes_read + LARGE_PAYLOAD_BYTES

    tampered = bytearray(appended)
    tampered[large_start + objects.OBJECT_HEADER_LEN] ^= 1
    tampered_target = targeted_lookup(CountingSource(bytes(tampered), limits), 1)
    assert tampered_target.found
    try:
        strict_validate(CountingSource(bytes(tampered), limits))
    except SourceError as error:
        strict_error = str(error)
    else:
        raise AssertionError("full validation accepted historical payload corruption")
    assert strict_error == "object digest"

    low_limits = replace(limits, max_total_bytes_read=1_000)
    low_source = CountingSource(appended, low_limits)
    try:
        targeted_lookup(low_source, 1)
    except SourceError as error:
        budget_error = str(error)
    else:
        raise AssertionError("low source budget unexpectedly succeeded")
    assert budget_error == "source byte budget"
    assert low_source.stats.bytes_read <= low_limits.max_total_bytes_read

    print(f"objects={OBJECTS:,}")
    print(f"large_payload_bytes={LARGE_PAYLOAD_BYTES:,}")
    print(f"target_requests={target.stats.read_operations}")
    print(f"target_bytes_read={target.stats.bytes_read:,}")
    print(f"target_path_pages={target.path_pages}")
    print(f"strict_requests={strict.stats.read_operations}")
    print(f"strict_bytes_read={strict.stats.bytes_read:,}")
    print(f"strict_pages={strict.page_count}")
    print(f"strict_objects_hashed={strict.stats.objects_hashed:,}")
    print(f"strict_historical_corruption={strict_error}")
    print(f"low_budget_failure={budget_error}")
    print("target_skipped_unrelated_large_payload=pass")
    print("full_validation_read_unrelated_large_payload=pass")
    print("targeted_and_full_assurance_scopes_are_distinct=pass")
    print("finding=bounded source lookup can authenticate one current object without reading unrelated historical payloads")
    print("finding=full active validation must hash every object reachable from the active directory")


if __name__ == "__main__":
    main()
