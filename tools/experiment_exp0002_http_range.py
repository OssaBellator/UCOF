#!/usr/bin/env python3
"""Cold-cache localhost HTTP Range measurements for EXP-0002 Candidate 1."""

from __future__ import annotations

import hashlib
import http.server
import json
import struct
import threading
import time
import urllib.request
from dataclasses import asdict, dataclass
from typing import Sequence

from exp0002_codec import (
    ABSENT_OFFSET,
    COMMIT_DOMAIN,
    FILE_HEADER_LEN,
    FOOTER_LEN,
    OBJECT_DOMAIN,
    OBJECT_HEADER_LEN,
    PAGE_DOMAIN,
    PAGE_SIZE,
    SNAPSHOT_DOMAIN,
    FileHeader,
    Footer,
    InternalEntry,
    LeafEntry,
    ObjectInput,
    PageLocator,
    Snapshot,
    _digest,
    _parse_page,
    build_append,
    build_genesis,
    validate_strict,
)

BLOCK_BYTES = 64 * 1024
LARGE_PAYLOAD_BYTES = 1024 * 1024


@dataclass
class Counters:
    requests: int = 0
    bytes_sent: int = 0
    ranges: list[tuple[int, int]] | None = None

    def reset(self) -> None:
        self.requests = 0
        self.bytes_sent = 0
        self.ranges = []


@dataclass(frozen=True)
class Measurement:
    operation: str
    requests: int
    bytes_transferred: int
    elapsed_ms: float
    pages_read: int
    objects_hashed: int


class RangeHandler(http.server.BaseHTTPRequestHandler):
    archive: bytes = b""
    counters = Counters(ranges=[])

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        if self.path != "/archive.ucof":
            self.send_error(404)
            return
        header = self.headers.get("Range")
        if header is None or not header.startswith("bytes=") or "," in header:
            self.send_error(400, "one byte range is required")
            return
        start_text, end_text = header[6:].split("-", 1)
        start = int(start_text)
        end = int(end_text)
        if start < 0 or end < start or end >= len(self.archive):
            self.send_response(416)
            self.send_header("Content-Range", f"bytes */{len(self.archive)}")
            self.end_headers()
            return
        body = self.archive[start : end + 1]
        self.counters.requests += 1
        self.counters.bytes_sent += len(body)
        assert self.counters.ranges is not None
        self.counters.ranges.append((start, end + 1))
        self.send_response(206)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Content-Range", f"bytes {start}-{end}/{len(self.archive)}")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _format: str, *_args: object) -> None:
        return


class RangeClient:
    def __init__(self, base_url: str, length: int) -> None:
        self.url = f"{base_url}/archive.ucof"
        self.length = length

    def read(self, offset: int, length: int) -> bytes:
        if offset < 0 or length < 0 or offset + length > self.length:
            raise ValueError("range is outside the source")
        if length == 0:
            return b""
        request = urllib.request.Request(
            self.url,
            headers={"Range": f"bytes={offset}-{offset + length - 1}"},
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            if response.status != 206:
                raise ValueError(f"unexpected HTTP status {response.status}")
            body = response.read()
        if len(body) != length:
            raise ValueError("short range response")
        return body

    def read_chunked(self, offset: int, length: int) -> bytes:
        output = bytearray()
        cursor = 0
        while cursor < length:
            take = min(BLOCK_BYTES, length - cursor)
            output.extend(self.read(offset + cursor, take))
            cursor += take
        return bytes(output)


def parse_active(client: RangeClient) -> tuple[int, Footer, Snapshot]:
    FileHeader.parse(client.read(0, FILE_HEADER_LEN))
    footer_offset = client.length - FOOTER_LEN
    footer = Footer.parse(client.read(footer_offset, FOOTER_LEN))
    if footer.commit_start + footer.commit_len != footer_offset:
        raise ValueError("invalid commit range")
    snapshot_bytes = client.read_chunked(footer.snapshot_offset, footer.snapshot_len)
    if _digest(SNAPSHOT_DOMAIN, snapshot_bytes) != footer.snapshot_digest:
        raise ValueError("snapshot digest mismatch")
    snapshot = Snapshot.parse(snapshot_bytes)
    if snapshot.sequence != footer.sequence or snapshot.previous_footer_offset != footer.previous_footer_offset:
        raise ValueError("snapshot/footer mismatch")
    if footer.previous_footer_offset == ABSENT_OFFSET:
        if footer.sequence != 0 or snapshot.parent_snapshot_digest != bytes(32):
            raise ValueError("invalid genesis parent")
    else:
        previous = Footer.parse(client.read(footer.previous_footer_offset, FOOTER_LEN))
        if previous.snapshot_digest != snapshot.parent_snapshot_digest or previous.sequence + 1 != footer.sequence:
            raise ValueError("invalid parent")
    return footer_offset, footer, snapshot


def verify_commit(client: RangeClient, footer: Footer) -> None:
    digest = hashlib.sha256()
    digest.update(COMMIT_DOMAIN)
    cursor = 0
    while cursor < footer.commit_len:
        take = min(BLOCK_BYTES, footer.commit_len - cursor)
        digest.update(client.read(footer.commit_start + cursor, take))
        cursor += take
    digest.update(footer.semantics())
    if digest.digest() != footer.commit_digest:
        raise ValueError("commit digest mismatch")


def read_page(client: RangeClient, locator: PageLocator, sequence: int):
    page = client.read(locator.offset, PAGE_SIZE)
    if _digest(PAGE_DOMAIN, page) != locator.digest:
        raise ValueError("page digest mismatch")
    parsed = _parse_page(page)
    if parsed.level != locator.level or parsed.sequence != sequence:
        raise ValueError("page reference mismatch")
    if locator.min_key and parsed.minimum != locator.min_key:
        raise ValueError("page minimum mismatch")
    if locator.max_key != ABSENT_OFFSET and parsed.maximum != locator.max_key:
        raise ValueError("page maximum mismatch")
    return parsed


def find_object(client: RangeClient, snapshot: Snapshot, object_id: int) -> tuple[LeafEntry | None, int]:
    locator = PageLocator(
        0,
        ABSENT_OFFSET,
        snapshot.directory_root_offset,
        snapshot.directory_root_level,
        snapshot.directory_root_digest,
    )
    pages = 0
    while True:
        parsed = read_page(client, locator, snapshot.sequence)
        pages += 1
        if parsed.kind == 1:
            entries = [entry for entry in parsed.entries if isinstance(entry, LeafEntry)]
            return next((entry for entry in entries if entry.object_id == object_id), None), pages
        children = [entry for entry in parsed.entries if isinstance(entry, InternalEntry)]
        child = next(
            (entry for entry in children if entry.min_key <= object_id <= entry.max_key),
            None,
        )
        if child is None:
            return None, pages
        locator = PageLocator(
            child.min_key,
            child.max_key,
            child.page_offset,
            child.level,
            child.page_digest,
        )


def collect_objects(client: RangeClient, snapshot: Snapshot) -> tuple[list[LeafEntry], int]:
    stack = [
        PageLocator(
            0,
            ABSENT_OFFSET,
            snapshot.directory_root_offset,
            snapshot.directory_root_level,
            snapshot.directory_root_digest,
        )
    ]
    objects: list[LeafEntry] = []
    pages = 0
    while stack:
        locator = stack.pop()
        parsed = read_page(client, locator, snapshot.sequence)
        pages += 1
        if parsed.kind == 1:
            objects.extend(entry for entry in parsed.entries if isinstance(entry, LeafEntry))
        else:
            children = [entry for entry in parsed.entries if isinstance(entry, InternalEntry)]
            for child in reversed(children):
                stack.append(
                    PageLocator(
                        child.min_key,
                        child.max_key,
                        child.page_offset,
                        child.level,
                        child.page_digest,
                    )
                )
    objects.sort(key=lambda entry: entry.object_id)
    return objects, pages


def verify_object(client: RangeClient, entry: LeafEntry) -> None:
    record = client.read_chunked(entry.record_offset, entry.record_len)
    if len(record) < OBJECT_HEADER_LEN or record[:4] != b"OBJ2":
        raise ValueError("invalid object")
    object_id, payload_len, logical_len = struct.unpack_from("<QQQ", record, 12)
    kind = struct.unpack_from("<H", record, 6)[0]
    if (
        object_id != entry.object_id
        or kind != entry.kind
        or payload_len != logical_len
        or logical_len != entry.logical_len
        or OBJECT_HEADER_LEN + payload_len != entry.record_len
    ):
        raise ValueError("object locator mismatch")
    if _digest(OBJECT_DOMAIN, record) != entry.record_digest:
        raise ValueError("object digest mismatch")


def measure_lookup(client: RangeClient, object_id: int) -> tuple[int, int]:
    _footer_offset, footer, snapshot = parse_active(client)
    verify_commit(client, footer)
    entry, pages = find_object(client, snapshot, object_id)
    if entry is None:
        raise ValueError("missing benchmark object")
    verify_object(client, entry)
    return pages, 1


def measure_strict(client: RangeClient) -> tuple[int, int]:
    _footer_offset, footer, snapshot = parse_active(client)
    verify_commit(client, footer)
    objects, pages = collect_objects(client, snapshot)
    for entry in objects:
        verify_object(client, entry)
    if not snapshot.roots or any(root not in {entry.object_id for entry in objects} for root in snapshot.roots):
        raise ValueError("invalid roots")
    return pages, len(objects)


def run_measurement(
    operation: str,
    client: RangeClient,
    counters: Counters,
    action,
) -> Measurement:
    counters.reset()
    start = time.perf_counter()
    pages, objects = action(client)
    elapsed_ms = (time.perf_counter() - start) * 1000
    return Measurement(
        operation,
        counters.requests,
        counters.bytes_sent,
        elapsed_ms,
        pages,
        objects,
    )


def overlaps(ranges: Sequence[tuple[int, int]], start: int, end: int) -> bool:
    return any(left < end and start < right for left, right in ranges)


def main() -> None:
    header = FileHeader(b"http-range-test!", b"http-nonce-0002!")
    genesis = build_genesis(
        header,
        [
            ObjectInput(1, 1, b"root", True),
            ObjectInput(2, 1, bytes([0xA5]) * LARGE_PAYLOAD_BYTES, False),
        ],
    )
    archive = build_append(
        genesis,
        [ObjectInput(3, 1, b"append", False)],
        [1, 3],
    )
    verified = validate_strict(archive)
    large = next(entry for entry in verified.objects if entry.object_id == 2)
    large_start = large.record_offset
    large_end = large.record_offset + large.record_len

    RangeHandler.archive = archive
    RangeHandler.counters = Counters(ranges=[])
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), RangeHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        host, port = server.server_address
        client = RangeClient(f"http://{host}:{port}", len(archive))
        lookup = run_measurement(
            "targeted_lookup_object_1",
            client,
            RangeHandler.counters,
            lambda source: measure_lookup(source, 1),
        )
        lookup_ranges = list(RangeHandler.counters.ranges or [])
        strict = run_measurement(
            "full_strict_validation",
            client,
            RangeHandler.counters,
            measure_strict,
        )
        strict_ranges = list(RangeHandler.counters.ranges or [])
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)

    assert not overlaps(lookup_ranges, large_start, large_end)
    assert overlaps(strict_ranges, large_start, large_end)
    assert lookup.bytes_transferred < strict.bytes_transferred
    assert lookup.bytes_transferred < LARGE_PAYLOAD_BYTES // 4
    assert strict.bytes_transferred > LARGE_PAYLOAD_BYTES
    assert lookup.objects_hashed == 1
    assert strict.objects_hashed == 3

    print(
        "| Operation | Requests | Bytes transferred | Pages read | Objects hashed | Elapsed ms |"
    )
    print("|---|---:|---:|---:|---:|---:|")
    for measurement in (lookup, strict):
        print(
            f"| {measurement.operation} | {measurement.requests} | "
            f"{measurement.bytes_transferred} | {measurement.pages_read} | "
            f"{measurement.objects_hashed} | {measurement.elapsed_ms:.3f} |"
        )
    print(json.dumps({"lookup": asdict(lookup), "strict": asdict(strict)}, sort_keys=True))
    print(f"archive_bytes={len(archive)}")
    print(f"large_historical_record_range={large_start}:{large_end}")
    print("lookup_read_large_historical_payload=false")
    print("strict_read_large_historical_payload=true")


if __name__ == "__main__":
    main()
