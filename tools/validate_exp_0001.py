#!/usr/bin/env python3
"""Independent stdlib validator for the UCOF-EXP-0001 experiment."""

from __future__ import annotations

import argparse
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

KIND_OPAQUE = 1
KIND_MANIFEST = 2
KIND_DIRECTORY = 3
KNOWN_KINDS = {KIND_OPAQUE, KIND_MANIFEST, KIND_DIRECTORY}


class FormatError(Exception):
    def __init__(self, category: str, detail: str) -> None:
        super().__init__(f"{category}: {detail}")
        self.category = category
        self.detail = detail


@dataclass(frozen=True)
class Limits:
    max_file_bytes: int = 64 * 1024 * 1024
    max_records: int = 100_000
    max_payload_bytes: int = 32 * 1024 * 1024
    max_metadata_bytes: int = 8 * 1024 * 1024
    max_metadata_depth: int = 64
    max_container_items: int = 100_000
    max_text_bytes: int = 1024 * 1024
    max_byte_string_bytes: int = 8 * 1024 * 1024


@dataclass(frozen=True)
class CborMap:
    entries: list[tuple[Any, Any]]


@dataclass(frozen=True)
class Record:
    kind: int
    object_id: int
    offset: int
    stored_len: int
    logical_len: int
    payload_start: int
    payload_end: int


@dataclass(frozen=True)
class ValidationResult:
    manifest_id: int
    roots: list[int]
    required: list[int]
    optional: list[int]
    records: list[Record]


class CborDecoder:
    def __init__(self, data: bytes, limits: Limits) -> None:
        if len(data) > limits.max_metadata_bytes:
            raise FormatError("limit_exceeded", "metadata bytes")
        self.data = data
        self.limits = limits
        self.offset = 0
        self.items = 0

    def decode(self) -> Any:
        value = self._value(0)
        if self.offset != len(self.data):
            raise FormatError("non_canonical_metadata", "trailing CBOR bytes")
        return value

    def _value(self, depth: int) -> Any:
        if depth > self.limits.max_metadata_depth:
            raise FormatError("limit_exceeded", "metadata depth")
        self.items += 1
        if self.items > self.limits.max_container_items:
            raise FormatError("limit_exceeded", "metadata item count")

        initial = self._byte("CBOR initial byte")
        major = initial >> 5
        additional = initial & 0x1F

        if major == 0:
            return self._argument(additional)
        if major == 2:
            length = self._argument(additional)
            if length > self.limits.max_byte_string_bytes:
                raise FormatError("limit_exceeded", "CBOR byte string")
            return self._take(length, "CBOR byte string")
        if major == 3:
            length = self._argument(additional)
            if length > self.limits.max_text_bytes:
                raise FormatError("limit_exceeded", "CBOR text")
            raw = self._take(length, "CBOR text")
            try:
                return raw.decode("utf-8")
            except UnicodeDecodeError as error:
                raise FormatError("non_canonical_metadata", "invalid UTF-8") from error
        if major == 4:
            length = self._argument(additional)
            self._container_length(length)
            return [self._value(depth + 1) for _ in range(length)]
        if major == 5:
            length = self._argument(additional)
            self._container_length(length)
            entries: list[tuple[Any, Any]] = []
            previous_key: bytes | None = None
            for _ in range(length):
                key_start = self.offset
                key = self._value(depth + 1)
                key_bytes = self.data[key_start : self.offset]
                if previous_key is not None and (
                    len(previous_key), previous_key
                ) >= (len(key_bytes), key_bytes):
                    raise FormatError(
                        "non_canonical_metadata",
                        "map keys are duplicate or out of order",
                    )
                previous_key = key_bytes
                entries.append((key, self._value(depth + 1)))
            return CborMap(entries)
        if major == 7:
            if additional == 20:
                return False
            if additional == 21:
                return True
            if additional == 22:
                return None
            raise FormatError(
                "non_canonical_metadata",
                "unsupported simple or floating-point value",
            )
        raise FormatError("non_canonical_metadata", "unsupported CBOR major type")

    def _argument(self, additional: int) -> int:
        if additional <= 23:
            return additional
        if additional == 24:
            value = self._byte("CBOR argument")
            if value < 24:
                raise FormatError("non_canonical_metadata", "non-shortest argument")
            return value
        if additional == 25:
            value = struct.unpack(">H", self._take(2, "CBOR argument"))[0]
            if value <= 0xFF:
                raise FormatError("non_canonical_metadata", "non-shortest argument")
            return value
        if additional == 26:
            value = struct.unpack(">I", self._take(4, "CBOR argument"))[0]
            if value <= 0xFFFF:
                raise FormatError("non_canonical_metadata", "non-shortest argument")
            return value
        if additional == 27:
            value = struct.unpack(">Q", self._take(8, "CBOR argument"))[0]
            if value <= 0xFFFF_FFFF:
                raise FormatError("non_canonical_metadata", "non-shortest argument")
            return value
        if additional == 31:
            raise FormatError(
                "non_canonical_metadata",
                "indefinite-length item is not permitted",
            )
        raise FormatError("non_canonical_metadata", "reserved CBOR argument")

    def _container_length(self, length: int) -> None:
        if length > self.limits.max_container_items:
            raise FormatError("limit_exceeded", "CBOR container items")

    def _byte(self, context: str) -> int:
        if self.offset >= len(self.data):
            raise FormatError("truncated", context)
        value = self.data[self.offset]
        self.offset += 1
        return value

    def _take(self, length: int, context: str) -> bytes:
        end = self.offset + length
        if end < self.offset or end > len(self.data):
            raise FormatError("truncated", context)
        value = self.data[self.offset:end]
        self.offset = end
        return value


def validate(data: bytes, limits: Limits = Limits()) -> ValidationResult:
    if len(data) > limits.max_file_bytes:
        raise FormatError("limit_exceeded", "file bytes")
    if len(data) < HEADER_LEN + FOOTER_LEN:
        raise FormatError("truncated", "file header or footer")

    if data[:8] != FILE_MAGIC:
        raise FormatError("invalid_magic", "file")
    epoch, flags, header_len = struct.unpack_from("<III", data, 8)
    if epoch != 1:
        raise FormatError("unsupported_epoch", str(epoch))
    if flags != 0:
        raise FormatError("unsupported_flags", "file")
    if header_len != HEADER_LEN:
        raise FormatError("invalid_length", "file header")
    if any(data[20:32]):
        raise FormatError("invalid_reserved", "file header")

    footer_offset = len(data) - FOOTER_LEN
    if data[footer_offset : footer_offset + 8] != FOOTER_MAGIC:
        raise FormatError("invalid_magic", "footer")
    (
        footer_len,
        footer_flags,
        directory_offset,
        directory_len,
        manifest_id,
        record_count,
    ) = struct.unpack_from("<IIQQQQ", data, footer_offset + 8)
    if footer_len != FOOTER_LEN:
        raise FormatError("invalid_length", "footer")
    if footer_flags != 0:
        raise FormatError("unsupported_flags", "footer")

    directory_end = checked_end(
        directory_offset, directory_len, footer_offset, "directory"
    )
    if directory_end != footer_offset:
        raise FormatError("invalid_record_order", "directory must end at footer")

    records = scan_records(data, footer_offset, limits)
    if len(records) != record_count:
        raise FormatError("invalid_length", "footer record count")
    if not records:
        raise FormatError("invalid_record_order", "missing directory")
    directory_record = records[-1]
    if directory_record.kind != KIND_DIRECTORY:
        raise FormatError("invalid_record_order", "last record is not directory")
    if directory_record.object_id != 0:
        raise FormatError("invalid_record_order", "directory identifier")
    if (
        directory_record.offset != directory_offset
        or RECORD_HEADER_LEN + directory_record.stored_len != directory_len
    ):
        raise FormatError("directory_mismatch", "footer location")

    expected_digest = data[footer_offset + 48 : footer_offset + 80]
    actual_digest = hashlib.sha256(data[:footer_offset]).digest()
    if actual_digest != expected_digest:
        raise FormatError("digest_mismatch", "committed prefix")

    directory_value = CborDecoder(
        data[directory_record.payload_start : directory_record.payload_end], limits
    ).decode()
    directory_entries = parse_directory(directory_value)
    compare_directory(records[:-1], directory_entries)

    manifest_records = [
        record
        for record in records[:-1]
        if record.object_id == manifest_id and record.kind == KIND_MANIFEST
    ]
    if len(manifest_records) != 1:
        raise FormatError("missing_manifest", str(manifest_id))
    manifest_record = manifest_records[0]
    manifest_value = CborDecoder(
        data[manifest_record.payload_start : manifest_record.payload_end], limits
    ).decode()
    roots, required, optional = parse_manifest(manifest_value)

    available = {record.object_id for record in records[:-1]}
    if any(root not in available for root in roots):
        raise FormatError("invalid_metadata_schema", "manifest root does not exist")
    if required:
        raise FormatError("unsupported_required_capability", str(required[0]))

    return ValidationResult(
        manifest_id=manifest_id,
        roots=roots,
        required=required,
        optional=optional,
        records=records,
    )


def scan_records(data: bytes, footer_offset: int, limits: Limits) -> list[Record]:
    records: list[Record] = []
    identifiers: set[int] = set()
    offset = HEADER_LEN

    while offset < footer_offset:
        if len(records) >= limits.max_records:
            raise FormatError("limit_exceeded", "record count")
        if footer_offset - offset < RECORD_HEADER_LEN:
            raise FormatError("truncated", "record header")
        if data[offset : offset + 4] != RECORD_MAGIC:
            raise FormatError("invalid_magic", "record")

        kind, flags, header_len = struct.unpack_from("<HHI", data, offset + 4)
        stored_len, logical_len, object_id, reserved = struct.unpack_from(
            "<QQQI", data, offset + 12
        )
        if kind not in KNOWN_KINDS:
            raise FormatError("unsupported_record_kind", str(kind))
        if flags != 0:
            raise FormatError("unsupported_flags", "record")
        if header_len != RECORD_HEADER_LEN:
            raise FormatError("invalid_length", "record header")
        if stored_len != logical_len:
            raise FormatError("invalid_length", "logical length")
        if stored_len > limits.max_payload_bytes:
            raise FormatError("limit_exceeded", "record payload")
        if reserved != 0:
            raise FormatError("invalid_reserved", "record header")

        if kind == KIND_DIRECTORY:
            if object_id != 0:
                raise FormatError("invalid_record_order", "directory identifier")
        elif object_id == 0:
            raise FormatError("invalid_record_order", "zero object identifier")
        elif object_id in identifiers:
            raise FormatError("duplicate_object_id", str(object_id))
        else:
            identifiers.add(object_id)

        payload_start = offset + RECORD_HEADER_LEN
        payload_end = checked_end(
            payload_start, stored_len, footer_offset, "record payload"
        )
        records.append(
            Record(
                kind=kind,
                object_id=object_id,
                offset=offset,
                stored_len=stored_len,
                logical_len=logical_len,
                payload_start=payload_start,
                payload_end=payload_end,
            )
        )
        offset = payload_end

    if offset != footer_offset:
        raise FormatError("invalid_record_order", "records do not end at footer")
    return records


def checked_end(offset: int, length: int, upper_bound: int, context: str) -> int:
    if offset < 0 or length < 0:
        raise FormatError("range_out_of_bounds", context)
    end = offset + length
    if end < offset or end > upper_bound:
        raise FormatError("range_out_of_bounds", context)
    return end


def exact_text_map(value: Any, keys: set[str], context: str) -> dict[str, Any]:
    if not isinstance(value, CborMap) or len(value.entries) != len(keys):
        raise FormatError("invalid_metadata_schema", context)
    result: dict[str, Any] = {}
    for key, item in value.entries:
        if not isinstance(key, str) or key not in keys or key in result:
            raise FormatError("invalid_metadata_schema", context)
        result[key] = item
    if result.keys() != keys:
        raise FormatError("invalid_metadata_schema", context)
    return result


def unsigned(value: Any, context: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise FormatError("invalid_metadata_schema", context)
    return value


def unsigned_array(value: Any, context: str, nonzero: bool = False) -> list[int]:
    if not isinstance(value, list):
        raise FormatError("invalid_metadata_schema", context)
    result = [unsigned(item, context) for item in value]
    if nonzero and any(item == 0 for item in result):
        raise FormatError("invalid_metadata_schema", context)
    if len(result) != len(set(result)):
        raise FormatError("invalid_metadata_schema", context)
    return result


def parse_manifest(value: Any) -> tuple[list[int], list[int], list[int]]:
    mapping = exact_text_map(value, {"roots", "required", "optional"}, "manifest")
    return (
        unsigned_array(mapping["roots"], "manifest roots", nonzero=True),
        unsigned_array(mapping["required"], "required capabilities"),
        unsigned_array(mapping["optional"], "optional capabilities"),
    )


def parse_directory(value: Any) -> list[dict[str, int]]:
    mapping = exact_text_map(value, {"entries"}, "directory")
    raw_entries = mapping["entries"]
    if not isinstance(raw_entries, list):
        raise FormatError("invalid_metadata_schema", "directory entries")
    entries: list[dict[str, int]] = []
    required_keys = {"id", "kind", "offset", "stored_len", "logical_len"}
    for raw_entry in raw_entries:
        entry = exact_text_map(raw_entry, required_keys, "directory entry")
        entries.append(
            {
                "id": unsigned(entry["id"], "directory id"),
                "kind": unsigned(entry["kind"], "directory kind"),
                "offset": unsigned(entry["offset"], "directory offset"),
                "stored_len": unsigned(
                    entry["stored_len"], "directory stored length"
                ),
                "logical_len": unsigned(
                    entry["logical_len"], "directory logical length"
                ),
            }
        )
    return entries


def compare_directory(records: list[Record], entries: list[dict[str, int]]) -> None:
    if len(records) != len(entries):
        raise FormatError("directory_mismatch", "entry count")
    for record, entry in zip(records, entries, strict=True):
        if (
            record.object_id != entry["id"]
            or record.kind != entry["kind"]
            or record.offset != entry["offset"]
            or record.stored_len != entry["stored_len"]
            or record.logical_len != entry["logical_len"]
        ):
            raise FormatError("directory_mismatch", "entry versus framing")


def decode_hex(path: Path) -> bytes:
    return bytes.fromhex(path.read_text(encoding="ascii"))


def check_vectors(directory: Path) -> None:
    checked = 0
    for expectation_path in sorted(directory.glob("*.json")):
        name = expectation_path.stem
        hex_path = directory / f"{name}.hex"
        expectation = json.loads(expectation_path.read_text(encoding="utf-8"))
        data = decode_hex(hex_path)
        checked += 1

        if expectation.get("expected") == "valid":
            result = validate(data)
            if "length" in expectation and len(data) != expectation["length"]:
                raise AssertionError(f"{name}: length mismatch")
            if "manifest_id" in expectation and (
                result.manifest_id != expectation["manifest_id"]
            ):
                raise AssertionError(f"{name}: manifest mismatch")
            if "sha256" in expectation and (
                hashlib.sha256(data).hexdigest() != expectation["sha256"]
            ):
                raise AssertionError(f"{name}: full-file SHA-256 mismatch")
            continue

        expected_error = expectation["expected_error"]
        try:
            validate(data)
        except FormatError as error:
            if error.category != expected_error:
                raise AssertionError(
                    f"{name}: expected {expected_error}, got {error.category}"
                ) from error
        else:
            raise AssertionError(f"{name}: invalid vector unexpectedly passed")

    print(f"validated {checked} UCOF-EXP-0001 vectors")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "path",
        nargs="?",
        type=Path,
        default=Path(__file__).resolve().parents[1]
        / "tests"
        / "vectors"
        / "exp-0001",
    )
    parser.add_argument(
        "--vectors",
        action="store_true",
        help="validate every .hex/.json pair in the directory",
    )
    arguments = parser.parse_args()

    if arguments.vectors or arguments.path.is_dir():
        check_vectors(arguments.path)
        return

    result = validate(arguments.path.read_bytes())
    print(
        json.dumps(
            {
                "epoch": 1,
                "manifest_id": result.manifest_id,
                "roots": result.roots,
                "records": [
                    {
                        "id": record.object_id,
                        "kind": record.kind,
                        "offset": record.offset,
                        "stored_len": record.stored_len,
                    }
                    for record in result.records
                ],
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
