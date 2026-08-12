#!/usr/bin/env python3
"""Independent provisional UCOF-EXP-0002 writer and strict validator.

This module intentionally does not import the Rust implementation. It exists to
reproduce the experimental bytes from docs/spec/EXP_0002_BYTE_CANDIDATE.md.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence

FILE_HEADER_LEN = 64
OBJECT_HEADER_LEN = 48
PAGE_HEADER_LEN = 64
PAGE_SIZE = 16 * 1024
LEAF_ENTRY_LEN = 88
INTERNAL_ENTRY_LEN = 64
LEAF_CAPACITY = (PAGE_SIZE - PAGE_HEADER_LEN) // LEAF_ENTRY_LEN
INTERNAL_CAPACITY = (PAGE_SIZE - PAGE_HEADER_LEN) // INTERNAL_ENTRY_LEN
SNAPSHOT_HEADER_LEN = 160
FOOTER_LEN = 160
ABSENT_OFFSET = (1 << 64) - 1
DIGEST_SHA256 = 1

FILE_MAGIC = b"UCOF2\r\n\x1a"
OBJECT_MAGIC = b"OBJ2"
PAGE_MAGIC = b"PG02"
SNAPSHOT_MAGIC = b"SNP2"
FOOTER_MAGIC = b"UCOF2END"

OBJECT_DOMAIN = b"UCOF-EXP-0002-OBJECT\0"
PAGE_DOMAIN = b"UCOF-EXP-0002-PAGE\0"
SNAPSHOT_DOMAIN = b"UCOF-EXP-0002-SNAPSHOT\0"
COMMIT_DOMAIN = b"UCOF-EXP-0002-COMMIT\0"


class Exp0002Error(ValueError):
    """Raised when provisional EXP-0002 bytes fail closed."""


def _u16(value: int) -> bytes:
    return struct.pack("<H", value)


def _u32(value: int) -> bytes:
    return struct.pack("<I", value)


def _u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def _read_u16(data: bytes, offset: int) -> int:
    return struct.unpack_from("<H", data, offset)[0]


def _read_u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def _read_u64(data: bytes, offset: int) -> int:
    return struct.unpack_from("<Q", data, offset)[0]


def _digest(domain: bytes, data: bytes) -> bytes:
    return hashlib.sha256(domain + data).digest()


def _require_zero(data: bytes, name: str) -> None:
    if any(data):
        raise Exp0002Error(f"non-zero reserved {name} bytes")


def _slice(data: bytes, offset: int, length: int, name: str) -> bytes:
    if offset < 0 or length < 0 or offset + length > len(data):
        raise Exp0002Error(f"truncated {name}")
    return data[offset : offset + length]


def _sorted_unique(values: Iterable[int], name: str, *, allow_empty: bool = True) -> list[int]:
    result = sorted(values)
    if not allow_empty and not result:
        raise Exp0002Error(f"empty {name}")
    if result and result[0] == 0:
        raise Exp0002Error(f"zero {name}")
    if any(left >= right for left, right in zip(result, result[1:])):
        raise Exp0002Error(f"duplicate or unordered {name}")
    return result


@dataclass(frozen=True)
class FileHeader:
    file_id: bytes
    creation_nonce: bytes

    def encode(self) -> bytes:
        if len(self.file_id) != 16 or len(self.creation_nonce) != 16:
            raise Exp0002Error("header identifiers must be 16 bytes")
        data = bytearray(FILE_HEADER_LEN)
        data[0:8] = FILE_MAGIC
        data[8:10] = _u16(2)
        data[10:12] = _u16(FILE_HEADER_LEN)
        data[12:16] = _u32(0)
        data[16:20] = _u32(PAGE_SIZE)
        data[20:22] = _u16(DIGEST_SHA256)
        data[24:40] = self.file_id
        data[40:56] = self.creation_nonce
        return bytes(data)

    @staticmethod
    def parse(data: bytes) -> "FileHeader":
        if len(data) < FILE_HEADER_LEN:
            raise Exp0002Error("truncated file header")
        header = data[:FILE_HEADER_LEN]
        if header[0:8] != FILE_MAGIC:
            raise Exp0002Error("invalid file magic")
        if _read_u16(header, 8) != 2 or _read_u16(header, 10) != FILE_HEADER_LEN:
            raise Exp0002Error("invalid file version or header length")
        if _read_u32(header, 12) != 0 or _read_u32(header, 16) != PAGE_SIZE:
            raise Exp0002Error("invalid file flags or page size")
        if _read_u16(header, 20) != DIGEST_SHA256:
            raise Exp0002Error("unsupported digest")
        _require_zero(header[22:24], "file header")
        _require_zero(header[56:64], "file header")
        return FileHeader(header[24:40], header[40:56])


@dataclass(frozen=True)
class ObjectInput:
    object_id: int
    kind: int
    payload: bytes
    is_root: bool = False


@dataclass(frozen=True)
class LeafEntry:
    object_id: int
    kind: int
    record_offset: int
    record_len: int
    logical_len: int
    record_digest: bytes

    def encode(self) -> bytes:
        data = bytearray(LEAF_ENTRY_LEN)
        data[0:8] = _u64(self.object_id)
        data[8:10] = _u16(self.kind)
        data[16:24] = _u64(self.record_offset)
        data[24:32] = _u64(self.record_len)
        data[32:40] = _u64(self.logical_len)
        data[40:72] = self.record_digest
        return bytes(data)

    @staticmethod
    def parse(data: bytes) -> "LeafEntry":
        if len(data) != LEAF_ENTRY_LEN:
            raise Exp0002Error("invalid leaf entry length")
        object_id = _read_u64(data, 0)
        kind = _read_u16(data, 8)
        if object_id == 0 or kind == 0:
            raise Exp0002Error("invalid leaf object")
        if _read_u16(data, 10) != 0 or _read_u32(data, 12) != 0:
            raise Exp0002Error("invalid leaf flags")
        _require_zero(data[72:88], "leaf entry")
        return LeafEntry(
            object_id,
            kind,
            _read_u64(data, 16),
            _read_u64(data, 24),
            _read_u64(data, 32),
            data[40:72],
        )


@dataclass(frozen=True)
class PageLocator:
    min_key: int
    max_key: int
    offset: int
    level: int
    digest: bytes


@dataclass(frozen=True)
class InternalEntry:
    min_key: int
    max_key: int
    page_offset: int
    page_len: int
    level: int
    page_digest: bytes

    def encode(self) -> bytes:
        data = bytearray(INTERNAL_ENTRY_LEN)
        data[0:8] = _u64(self.min_key)
        data[8:16] = _u64(self.max_key)
        data[16:24] = _u64(self.page_offset)
        data[24:28] = _u32(self.page_len)
        data[28:30] = _u16(self.level)
        data[32:64] = self.page_digest
        return bytes(data)

    @staticmethod
    def parse(data: bytes) -> "InternalEntry":
        if len(data) != INTERNAL_ENTRY_LEN:
            raise Exp0002Error("invalid internal entry length")
        minimum = _read_u64(data, 0)
        maximum = _read_u64(data, 8)
        if minimum == 0 or minimum > maximum:
            raise Exp0002Error("invalid child range")
        if _read_u32(data, 24) != PAGE_SIZE or _read_u16(data, 30) != 0:
            raise Exp0002Error("invalid child locator")
        return InternalEntry(
            minimum,
            maximum,
            _read_u64(data, 16),
            _read_u32(data, 24),
            _read_u16(data, 28),
            data[32:64],
        )


@dataclass(frozen=True)
class ParsedPage:
    kind: int
    level: int
    minimum: int
    maximum: int
    sequence: int
    entries: Sequence[LeafEntry | InternalEntry]


@dataclass(frozen=True)
class Snapshot:
    sequence: int
    parent_snapshot_digest: bytes
    previous_footer_offset: int
    directory_root_offset: int
    directory_root_level: int
    directory_root_digest: bytes
    roots: Sequence[int]
    required_capabilities: Sequence[int] = ()
    optional_capabilities: Sequence[int] = ()

    def encode(self) -> bytes:
        roots = _sorted_unique(self.roots, "roots", allow_empty=False)
        required = _sorted_unique(self.required_capabilities, "required capabilities")
        optional = _sorted_unique(self.optional_capabilities, "optional capabilities")
        if set(required).intersection(optional):
            raise Exp0002Error("overlapping capability sets")
        data = bytearray(SNAPSHOT_HEADER_LEN + 8 * (len(roots) + len(required) + len(optional)))
        data[0:4] = SNAPSHOT_MAGIC
        data[4:6] = _u16(SNAPSHOT_HEADER_LEN)
        data[8:16] = _u64(self.sequence)
        if len(self.parent_snapshot_digest) != 32 or len(self.directory_root_digest) != 32:
            raise Exp0002Error("snapshot digest fields must be 32 bytes")
        data[16:48] = self.parent_snapshot_digest
        data[48:56] = _u64(self.previous_footer_offset)
        data[56:64] = _u64(self.directory_root_offset)
        data[64:68] = _u32(PAGE_SIZE)
        data[68:70] = _u16(self.directory_root_level)
        data[70:72] = _u16(DIGEST_SHA256)
        data[72:104] = self.directory_root_digest
        data[104:108] = _u32(len(roots))
        data[108:112] = _u32(len(required))
        data[112:116] = _u32(len(optional))
        cursor = SNAPSHOT_HEADER_LEN
        for value in [*roots, *required, *optional]:
            data[cursor : cursor + 8] = _u64(value)
            cursor += 8
        return bytes(data)

    @staticmethod
    def parse(data: bytes) -> "Snapshot":
        if len(data) < SNAPSHOT_HEADER_LEN or data[0:4] != SNAPSHOT_MAGIC:
            raise Exp0002Error("invalid snapshot")
        if _read_u16(data, 4) != SNAPSHOT_HEADER_LEN or _read_u16(data, 6) != 0:
            raise Exp0002Error("invalid snapshot header")
        if _read_u32(data, 64) != PAGE_SIZE or _read_u16(data, 70) != DIGEST_SHA256:
            raise Exp0002Error("invalid snapshot root locator")
        _require_zero(data[116:160], "snapshot")
        counts = (_read_u32(data, 104), _read_u32(data, 108), _read_u32(data, 112))
        if len(data) != SNAPSHOT_HEADER_LEN + 8 * sum(counts):
            raise Exp0002Error("invalid snapshot length")
        cursor = SNAPSHOT_HEADER_LEN
        arrays: list[list[int]] = []
        for count in counts:
            values = [_read_u64(data, cursor + 8 * index) for index in range(count)]
            cursor += 8 * count
            arrays.append(values)
        roots = _sorted_unique(arrays[0], "roots", allow_empty=False)
        required = _sorted_unique(arrays[1], "required capabilities")
        optional = _sorted_unique(arrays[2], "optional capabilities")
        if set(required).intersection(optional):
            raise Exp0002Error("overlapping capability sets")
        return Snapshot(
            _read_u64(data, 8),
            data[16:48],
            _read_u64(data, 48),
            _read_u64(data, 56),
            _read_u16(data, 68),
            data[72:104],
            roots,
            required,
            optional,
        )


@dataclass(frozen=True)
class Footer:
    commit_start: int
    commit_len: int
    snapshot_offset: int
    snapshot_len: int
    sequence: int
    previous_footer_offset: int
    record_count: int
    snapshot_digest: bytes
    commit_digest: bytes = bytes(32)

    def semantics(self) -> bytes:
        data = bytearray(104)
        data[0:2] = _u16(FOOTER_LEN)
        data[2:4] = _u16(2)
        data[8:16] = _u64(self.commit_start)
        data[16:24] = _u64(self.commit_len)
        data[24:32] = _u64(self.snapshot_offset)
        data[32:40] = _u64(self.snapshot_len)
        data[40:48] = _u64(self.sequence)
        data[48:56] = _u64(self.previous_footer_offset)
        data[56:64] = _u64(self.record_count)
        data[64:66] = _u16(DIGEST_SHA256)
        data[72:104] = self.snapshot_digest
        return bytes(data)

    def encode(self) -> bytes:
        if len(self.snapshot_digest) != 32 or len(self.commit_digest) != 32:
            raise Exp0002Error("footer digests must be 32 bytes")
        data = bytearray(FOOTER_LEN)
        data[0:8] = FOOTER_MAGIC
        data[8:112] = self.semantics()
        data[112:144] = self.commit_digest
        return bytes(data)

    @staticmethod
    def parse(data: bytes) -> "Footer":
        if len(data) != FOOTER_LEN or data[0:8] != FOOTER_MAGIC:
            raise Exp0002Error("invalid footer")
        if _read_u16(data, 8) != FOOTER_LEN or _read_u16(data, 10) != 2:
            raise Exp0002Error("invalid footer version")
        if _read_u32(data, 12) != 0 or _read_u16(data, 72) != DIGEST_SHA256:
            raise Exp0002Error("invalid footer flags or digest")
        _require_zero(data[74:80], "footer")
        _require_zero(data[144:160], "footer")
        return Footer(
            _read_u64(data, 16),
            _read_u64(data, 24),
            _read_u64(data, 32),
            _read_u64(data, 40),
            _read_u64(data, 48),
            _read_u64(data, 56),
            _read_u64(data, 64),
            data[80:112],
            data[112:144],
        )


@dataclass(frozen=True)
class Verified:
    footer_offset: int
    footer: Footer
    snapshot: Snapshot
    objects: Sequence[LeafEntry]
    pages_verified: int


def _object_bytes(value: ObjectInput) -> tuple[bytes, bytes]:
    if value.object_id == 0 or value.kind == 0:
        raise Exp0002Error("invalid object")
    payload_len = len(value.payload)
    header = bytearray(OBJECT_HEADER_LEN)
    header[0:4] = OBJECT_MAGIC
    header[4:6] = _u16(OBJECT_HEADER_LEN)
    header[6:8] = _u16(value.kind)
    header[12:20] = _u64(value.object_id)
    header[20:28] = _u64(payload_len)
    header[28:36] = _u64(payload_len)
    record = bytes(header) + value.payload
    return record, _digest(OBJECT_DOMAIN, record)


def _encode_page_header(kind: int, level: int, entries: int, entry_size: int, minimum: int, maximum: int, sequence: int) -> bytearray:
    data = bytearray(PAGE_SIZE)
    data[0:4] = PAGE_MAGIC
    data[4] = kind
    data[5] = level
    data[6:8] = _u16(PAGE_HEADER_LEN)
    data[8:10] = _u16(entries)
    data[10:12] = _u16(entry_size)
    data[16:24] = _u64(minimum)
    data[24:32] = _u64(maximum)
    data[32:40] = _u64(sequence)
    return data


def _encode_leaf_page(entries: Sequence[LeafEntry], sequence: int) -> bytes:
    if not entries or len(entries) > LEAF_CAPACITY:
        raise Exp0002Error("invalid leaf count")
    if any(left.object_id >= right.object_id for left, right in zip(entries, entries[1:])):
        raise Exp0002Error("unordered leaf entries")
    data = _encode_page_header(1, 0, len(entries), LEAF_ENTRY_LEN, entries[0].object_id, entries[-1].object_id, sequence)
    for index, entry in enumerate(entries):
        start = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN
        data[start : start + LEAF_ENTRY_LEN] = entry.encode()
    return bytes(data)


def _encode_internal_page(children: Sequence[PageLocator], sequence: int, level: int) -> bytes:
    if not children or len(children) > INTERNAL_CAPACITY or not 0 < level <= 255:
        raise Exp0002Error("invalid internal page")
    if any(left.max_key >= right.min_key for left, right in zip(children, children[1:])):
        raise Exp0002Error("overlapping child ranges")
    data = _encode_page_header(2, level, len(children), INTERNAL_ENTRY_LEN, children[0].min_key, children[-1].max_key, sequence)
    for index, child in enumerate(children):
        entry = InternalEntry(child.min_key, child.max_key, child.offset, PAGE_SIZE, child.level, child.digest)
        start = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN
        data[start : start + INTERNAL_ENTRY_LEN] = entry.encode()
    return bytes(data)


def _write_directory(output: bytearray, entries: Sequence[LeafEntry], sequence: int) -> PageLocator:
    level: list[PageLocator] = []
    for start in range(0, len(entries), LEAF_CAPACITY):
        chunk = entries[start : start + LEAF_CAPACITY]
        page = _encode_leaf_page(chunk, sequence)
        locator = PageLocator(chunk[0].object_id, chunk[-1].object_id, len(output), 0, _digest(PAGE_DOMAIN, page))
        output.extend(page)
        level.append(locator)
    while len(level) > 1:
        next_level: list[PageLocator] = []
        for start in range(0, len(level), INTERNAL_CAPACITY):
            chunk = level[start : start + INTERNAL_CAPACITY]
            parent_level = chunk[0].level + 1
            page = _encode_internal_page(chunk, sequence, parent_level)
            locator = PageLocator(chunk[0].min_key, chunk[-1].max_key, len(output), parent_level, _digest(PAGE_DOMAIN, page))
            output.extend(page)
            next_level.append(locator)
        level = next_level
    if not level:
        raise Exp0002Error("empty directory")
    return level[0]


def _write_objects(output: bytearray, objects: Sequence[ObjectInput]) -> list[LeafEntry]:
    ordered = sorted(objects, key=lambda value: value.object_id)
    ids = _sorted_unique((value.object_id for value in ordered), "object identifiers", allow_empty=False)
    if len(ids) != len(ordered):
        raise Exp0002Error("duplicate objects")
    entries: list[LeafEntry] = []
    for value in ordered:
        record, digest = _object_bytes(value)
        offset = len(output)
        output.extend(record)
        entries.append(LeafEntry(value.object_id, value.kind, offset, len(record), len(value.payload), digest))
    return entries


def _finish_commit(
    output: bytearray,
    entries: Sequence[LeafEntry],
    roots: Sequence[int],
    sequence: int,
    parent_digest: bytes,
    previous_footer_offset: int,
    commit_start: int,
    record_count: int,
) -> bytes:
    root = _write_directory(output, entries, sequence)
    snapshot = Snapshot(sequence, parent_digest, previous_footer_offset, root.offset, root.level, root.digest, roots)
    snapshot_bytes = snapshot.encode()
    snapshot_offset = len(output)
    snapshot_digest = _digest(SNAPSHOT_DOMAIN, snapshot_bytes)
    output.extend(snapshot_bytes)
    footer_offset = len(output)
    footer = Footer(
        commit_start,
        footer_offset - commit_start,
        snapshot_offset,
        len(snapshot_bytes),
        sequence,
        previous_footer_offset,
        record_count,
        snapshot_digest,
    )
    commit_digest = hashlib.sha256(COMMIT_DOMAIN + bytes(output[commit_start:footer_offset]) + footer.semantics()).digest()
    output.extend(Footer(**{**footer.__dict__, "commit_digest": commit_digest}).encode())
    return bytes(output)


def build_genesis(header: FileHeader, objects: Sequence[ObjectInput]) -> bytes:
    if not objects:
        raise Exp0002Error("empty object set")
    roots = _sorted_unique((value.object_id for value in objects if value.is_root), "roots", allow_empty=False)
    output = bytearray(header.encode())
    entries = _write_objects(output, objects)
    return _finish_commit(output, entries, roots, 0, bytes(32), ABSENT_OFFSET, 0, len(entries))


def build_append(previous_file: bytes, objects: Sequence[ObjectInput], roots: Sequence[int]) -> bytes:
    previous = validate_strict(previous_file)
    root_values = _sorted_unique(roots, "roots", allow_empty=False)
    existing = list(previous.objects)
    known = {entry.object_id for entry in existing}
    if any(value.object_id in known for value in objects):
        raise Exp0002Error("duplicate append object")
    output = bytearray(previous_file)
    added = _write_objects(output, objects)
    entries = sorted([*existing, *added], key=lambda entry: entry.object_id)
    available = {entry.object_id for entry in entries}
    if any(root not in available for root in root_values):
        raise Exp0002Error("missing append root")
    return _finish_commit(
        output,
        entries,
        root_values,
        previous.snapshot.sequence + 1,
        previous.footer.snapshot_digest,
        previous.footer_offset,
        len(previous_file),
        len(added),
    )


def _parse_page(page: bytes) -> ParsedPage:
    if len(page) != PAGE_SIZE or page[0:4] != PAGE_MAGIC:
        raise Exp0002Error("invalid page")
    kind = page[4]
    level = page[5]
    count = _read_u16(page, 8)
    entry_size = _read_u16(page, 10)
    minimum = _read_u64(page, 16)
    maximum = _read_u64(page, 24)
    sequence = _read_u64(page, 32)
    if _read_u16(page, 6) != PAGE_HEADER_LEN or _read_u32(page, 12) != 0 or not count:
        raise Exp0002Error("invalid page header")
    if minimum == 0 or minimum > maximum:
        raise Exp0002Error("invalid page range")
    _require_zero(page[40:64], "page header")
    if kind == 1:
        if level != 0 or entry_size != LEAF_ENTRY_LEN or count > LEAF_CAPACITY:
            raise Exp0002Error("invalid leaf page")
        entries = [LeafEntry.parse(page[PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN : PAGE_HEADER_LEN + (index + 1) * LEAF_ENTRY_LEN]) for index in range(count)]
        used = PAGE_HEADER_LEN + count * LEAF_ENTRY_LEN
        _require_zero(page[used:], "leaf padding")
        keys = [entry.object_id for entry in entries]
        if keys[0] != minimum or keys[-1] != maximum or any(left >= right for left, right in zip(keys, keys[1:])):
            raise Exp0002Error("invalid leaf order")
        return ParsedPage(kind, level, minimum, maximum, sequence, entries)
    if kind == 2:
        if level == 0 or entry_size != INTERNAL_ENTRY_LEN or count > INTERNAL_CAPACITY:
            raise Exp0002Error("invalid internal page")
        entries = [InternalEntry.parse(page[PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN : PAGE_HEADER_LEN + (index + 1) * INTERNAL_ENTRY_LEN]) for index in range(count)]
        used = PAGE_HEADER_LEN + count * INTERNAL_ENTRY_LEN
        _require_zero(page[used:], "internal padding")
        if entries[0].min_key != minimum or entries[-1].max_key != maximum:
            raise Exp0002Error("invalid internal range")
        if any(entry.level + 1 != level for entry in entries):
            raise Exp0002Error("invalid internal level")
        if any(left.max_key >= right.min_key for left, right in zip(entries, entries[1:])):
            raise Exp0002Error("overlapping internal entries")
        return ParsedPage(kind, level, minimum, maximum, sequence, entries)
    raise Exp0002Error("invalid page kind")


def _parse_object(data: bytes, entry: LeafEntry) -> tuple[int, int]:
    record = _slice(data, entry.record_offset, entry.record_len, "object")
    if len(record) < OBJECT_HEADER_LEN or record[0:4] != OBJECT_MAGIC:
        raise Exp0002Error("invalid object record")
    if _read_u16(record, 4) != OBJECT_HEADER_LEN or _read_u32(record, 8) != 0:
        raise Exp0002Error("invalid object header")
    object_id = _read_u64(record, 12)
    payload_len = _read_u64(record, 20)
    logical_len = _read_u64(record, 28)
    kind = _read_u16(record, 6)
    _require_zero(record[36:48], "object")
    if object_id != entry.object_id or kind != entry.kind or payload_len != logical_len or logical_len != entry.logical_len:
        raise Exp0002Error("object locator mismatch")
    if OBJECT_HEADER_LEN + payload_len != entry.record_len:
        raise Exp0002Error("object length mismatch")
    if _digest(OBJECT_DOMAIN, record) != entry.record_digest:
        raise Exp0002Error("object digest mismatch")
    return entry.record_offset, entry.record_offset + entry.record_len


def validate_strict(data: bytes) -> Verified:
    if len(data) < FILE_HEADER_LEN + FOOTER_LEN:
        raise Exp0002Error("truncated file")
    FileHeader.parse(data)
    footer_offset = len(data) - FOOTER_LEN
    footer = Footer.parse(data[footer_offset:])
    if footer.commit_start + footer.commit_len != footer_offset:
        raise Exp0002Error("invalid commit range")
    snapshot_bytes = _slice(data, footer.snapshot_offset, footer.snapshot_len, "snapshot")
    if footer.snapshot_offset < footer.commit_start or footer.snapshot_offset + footer.snapshot_len > footer_offset:
        raise Exp0002Error("invalid snapshot range")
    if _digest(SNAPSHOT_DOMAIN, snapshot_bytes) != footer.snapshot_digest:
        raise Exp0002Error("snapshot digest mismatch")
    snapshot = Snapshot.parse(snapshot_bytes)
    if snapshot.sequence != footer.sequence or snapshot.previous_footer_offset != footer.previous_footer_offset:
        raise Exp0002Error("snapshot/footer mismatch")
    if footer.previous_footer_offset == ABSENT_OFFSET:
        if footer.sequence != 0 or footer.commit_start != 0 or snapshot.parent_snapshot_digest != bytes(32):
            raise Exp0002Error("invalid genesis parent")
    else:
        if footer.previous_footer_offset >= footer_offset or footer.commit_start != footer.previous_footer_offset + FOOTER_LEN:
            raise Exp0002Error("invalid previous footer")
        previous_footer = Footer.parse(_slice(data, footer.previous_footer_offset, FOOTER_LEN, "previous footer"))
        if previous_footer.snapshot_digest != snapshot.parent_snapshot_digest or previous_footer.sequence + 1 != footer.sequence:
            raise Exp0002Error("invalid parent snapshot")
    expected_commit = hashlib.sha256(COMMIT_DOMAIN + data[footer.commit_start:footer_offset] + footer.semantics()).digest()
    if expected_commit != footer.commit_digest:
        raise Exp0002Error("commit digest mismatch")

    stack: list[PageLocator] = [PageLocator(0, ABSENT_OFFSET, snapshot.directory_root_offset, snapshot.directory_root_level, snapshot.directory_root_digest)]
    visited: set[int] = set()
    entries: list[LeafEntry] = []
    page_ranges: list[tuple[int, int]] = []
    while stack:
        expected = stack.pop()
        if expected.offset in visited:
            raise Exp0002Error("page cycle")
        visited.add(expected.offset)
        page = _slice(data, expected.offset, PAGE_SIZE, "page")
        if _digest(PAGE_DOMAIN, page) != expected.digest:
            raise Exp0002Error("page digest mismatch")
        parsed = _parse_page(page)
        if parsed.level != expected.level or parsed.sequence != snapshot.sequence:
            raise Exp0002Error("page reference mismatch")
        if expected.min_key and parsed.minimum != expected.min_key:
            raise Exp0002Error("page minimum mismatch")
        if expected.max_key != ABSENT_OFFSET and parsed.maximum != expected.max_key:
            raise Exp0002Error("page maximum mismatch")
        page_ranges.append((expected.offset, expected.offset + PAGE_SIZE))
        if parsed.kind == 1:
            entries.extend(entry for entry in parsed.entries if isinstance(entry, LeafEntry))
        else:
            children = [entry for entry in parsed.entries if isinstance(entry, InternalEntry)]
            for child in reversed(children):
                stack.append(PageLocator(child.min_key, child.max_key, child.page_offset, child.level, child.page_digest))

    entries.sort(key=lambda entry: entry.object_id)
    if any(left.object_id >= right.object_id for left, right in zip(entries, entries[1:])):
        raise Exp0002Error("duplicate directory object")
    physical = [_parse_object(data, entry) for entry in entries]
    physical.sort()
    if any(left[1] > right[0] for left, right in zip(physical, physical[1:])):
        raise Exp0002Error("overlapping objects")
    protected = [*page_ranges, (footer.snapshot_offset, footer.snapshot_offset + footer.snapshot_len), (footer_offset, len(data))]
    if any(start < other_end and other_start < end for start, end in physical for other_start, other_end in protected):
        raise Exp0002Error("object overlaps structure")
    known = {entry.object_id for entry in entries}
    if not snapshot.roots or any(root not in known for root in snapshot.roots):
        raise Exp0002Error("invalid roots")
    return Verified(footer_offset, footer, snapshot, entries, len(visited))


def canonical_cases() -> dict[str, bytes]:
    header = FileHeader(b"exp0002-file-id!", b"fixed-nonce-0002")
    genesis = build_genesis(
        header,
        [
            ObjectInput(2, 1, b"second", False),
            ObjectInput(1, 1, b"first", True),
        ],
    )
    append = build_append(genesis, [ObjectInput(3, 1, b"third", False)], [1, 3])
    multi_leaf = build_genesis(
        header,
        [ObjectInput(index, 1, bytes([index % 251]), index == 1) for index in range(1, 401)],
    )
    return {"genesis-two-object": genesis, "append-add-third": append, "multi-leaf-400": multi_leaf}


def vector_manifest(cases: dict[str, bytes]) -> dict[str, object]:
    return {
        "epoch": "UCOF-EXP-0002",
        "candidate": 1,
        "vectors": {
            name: {
                "length": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
                "footer_offset": len(data) - FOOTER_LEN,
                "hex_file": f"{name}.hex",
            }
            for name, data in sorted(cases.items())
        },
    }


def write_vectors(directory: Path) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    cases = canonical_cases()
    for name, data in cases.items():
        validate_strict(data)
        (directory / f"{name}.hex").write_text(data.hex() + "\n", encoding="ascii")
    (directory / "manifest.json").write_text(
        json.dumps(vector_manifest(cases), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def verify_vectors(directory: Path) -> None:
    manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
    generated = canonical_cases()
    expected = manifest["vectors"]
    if set(generated) != set(expected):
        raise Exp0002Error("vector name mismatch")
    for name, data in generated.items():
        vector_path = directory / expected[name]["hex_file"]
        stored = bytes.fromhex(vector_path.read_text(encoding="ascii"))
        if stored != data:
            raise Exp0002Error(f"vector byte mismatch: {name}")
        if len(data) != expected[name]["length"] or hashlib.sha256(data).hexdigest() != expected[name]["sha256"]:
            raise Exp0002Error(f"vector manifest mismatch: {name}")
        validate_strict(stored)


def self_test() -> None:
    cases = canonical_cases()
    for name, data in cases.items():
        validate_strict(data)
        for cut in range(max(FILE_HEADER_LEN, len(data) - FOOTER_LEN - 32), len(data)):
            try:
                validate_strict(data[:cut])
            except Exp0002Error:
                pass
            else:
                raise Exp0002Error(f"truncation accepted for {name} at {cut}")
        mutated = bytearray(data)
        mutated[len(mutated) // 2] ^= 1
        try:
            validate_strict(bytes(mutated))
        except Exp0002Error:
            pass
        else:
            raise Exp0002Error(f"mutation accepted for {name}")
    print(f"validated {len(cases)} independent EXP-0002 cases")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write-vectors", type=Path)
    parser.add_argument("--verify-vectors", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    selected = sum(value is not None and value is not False for value in (args.write_vectors, args.verify_vectors, args.self_test))
    if selected != 1:
        parser.error("select exactly one operation")
    if args.write_vectors:
        write_vectors(args.write_vectors)
    elif args.verify_vectors:
        verify_vectors(args.verify_vectors)
    else:
        self_test()


if __name__ == "__main__":
    main()
