#!/usr/bin/env python3
"""Bounded recovery and source-history traversal for immutable-page objects."""

from __future__ import annotations

from dataclasses import dataclass, replace

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_object_history as history_model
import experiment_exp0002_immutable_page_object_source as source_model
import experiment_exp0002_immutable_page_objects as objects

OBJECTS = 300
FAKE_MAGIC_COUNT = 8


@dataclass(frozen=True)
class RecoveryLimits:
    source: source_model.SourceLimits = source_model.SourceLimits(
        max_total_bytes_read=128 * 1024 * 1024,
        max_read_operations=200_000,
        max_read_request_bytes=cow.PAGE_SIZE,
        hash_block_bytes=4 * 1024,
        max_pages=100_000,
        max_objects=100_000,
    )
    max_scan_bytes: int = 16 * 1024 * 1024
    max_scan_read_operations: int = 4096
    max_magic_matches: int = 4096
    max_candidate_validations: int = 1024
    max_results: int = 64
    max_history_entries: int = 64


@dataclass(frozen=True)
class RecoveredPrefix:
    prefix_len: int
    footer_offset: int
    sequence: int
    snapshot_digest: bytes
    commit_digest: bytes


@dataclass(frozen=True)
class RecoveryReport:
    file_len: int
    scan_start: int
    scan_bytes_read: int
    scan_read_operations: int
    magic_matches: int
    truncated_footer_matches: int
    candidates_validated: int
    failed_candidates: int
    results: tuple[RecoveredPrefix, ...]
    source_stats: source_model.SourceStats


@dataclass(frozen=True)
class HistoryReport:
    sequences: tuple[int, ...]
    prefix_lengths: tuple[int, ...]
    source_stats: source_model.SourceStats


class PrefixView:
    def __init__(
        self,
        backing: source_model.CountingSource,
        length: int,
    ) -> None:
        if length < 0 or length > len(backing):
            raise source_model.SourceError("prefix range")
        self.backing = backing
        self.length = length
        self.limits = backing.limits
        self.stats = backing.stats
        self.ranges = backing.ranges

    def __len__(self) -> int:
        return self.length

    def read_exact(self, offset: int, length: int, label: str) -> bytes:
        if offset < 0 or length < 0 or offset + length > self.length:
            raise source_model.SourceError(f"{label} range")
        return self.backing.read_exact(offset, length, label)


def snapshot_stats(stats: source_model.SourceStats) -> source_model.SourceStats:
    return replace(stats)


def scan_valid_prefixes(
    data: bytes, limits: RecoveryLimits
) -> RecoveryReport:
    if (
        limits.max_scan_bytes < 0
        or limits.max_scan_read_operations <= 0
        or limits.max_magic_matches < 0
        or limits.max_candidate_validations < 0
        or limits.max_results < 0
    ):
        raise source_model.SourceError("recovery configuration")

    backing = source_model.CountingSource(data, limits.source)
    scan_len = min(len(data), limits.max_scan_bytes)
    scan_start = len(data) - scan_len
    scan = bytearray()
    cursor = scan_start
    scan_reads = 0
    while cursor < len(data):
        if scan_reads >= limits.max_scan_read_operations:
            raise source_model.SourceError("recovery scan operation limit")
        take = min(
            len(data) - cursor,
            limits.source.max_read_request_bytes,
        )
        scan.extend(backing.read_exact(cursor, take, "recovery scan"))
        cursor += take
        scan_reads += 1

    positions: list[int] = []
    search = 0
    while True:
        found = scan.find(cow.FOOTER_MAGIC, search)
        if found < 0:
            break
        if len(positions) >= limits.max_magic_matches:
            raise source_model.SourceError("recovery magic-match limit")
        positions.append(scan_start + found)
        search = found + 1

    results: list[RecoveredPrefix] = []
    candidates = 0
    failed = 0
    truncated = 0
    for footer_offset in reversed(positions):
        prefix_len = footer_offset + cow.FOOTER_LEN
        if prefix_len > len(data):
            truncated += 1
            continue
        if candidates >= limits.max_candidate_validations:
            raise source_model.SourceError("recovery candidate limit")
        candidates += 1
        view = PrefixView(backing, prefix_len)
        try:
            strict = source_model.strict_validate(view)
            footer = source_model.parse_footer_bytes(
                view.read_exact(
                    footer_offset, cow.FOOTER_LEN, "recovered footer"
                )
            )
        except cow.FormatError:
            failed += 1
            continue
        if strict.sequence != footer.sequence:
            raise source_model.SourceError("recovery sequence mismatch")
        results.append(
            RecoveredPrefix(
                prefix_len,
                footer_offset,
                footer.sequence,
                footer.snapshot_digest,
                footer.commit_digest,
            )
        )
        if len(results) > limits.max_results:
            raise source_model.SourceError("recovery result limit")

    return RecoveryReport(
        len(data),
        scan_start,
        scan_len,
        scan_reads,
        len(positions),
        truncated,
        candidates,
        failed,
        tuple(results),
        snapshot_stats(backing.stats),
    )


def validate_history_at(
    data: bytes,
    prefix_len: int,
    limits: RecoveryLimits,
) -> HistoryReport:
    backing = source_model.CountingSource(data, limits.source)
    current_len = prefix_len
    sequences: list[int] = []
    lengths: list[int] = []
    expected_sequence: int | None = None
    expected_snapshot_digest: bytes | None = None

    while True:
        if len(sequences) >= limits.max_history_entries:
            raise source_model.SourceError("history entry limit")
        view = PrefixView(backing, current_len)
        strict = source_model.strict_validate(view)
        footer_offset = current_len - cow.FOOTER_LEN
        footer = source_model.parse_footer_bytes(
            view.read_exact(footer_offset, cow.FOOTER_LEN, "history footer")
        )
        if strict.sequence != footer.sequence:
            raise source_model.SourceError("history sequence mismatch")
        if expected_sequence is not None and footer.sequence != expected_sequence:
            raise source_model.SourceError("history sequence link")
        if (
            expected_snapshot_digest is not None
            and footer.snapshot_digest != expected_snapshot_digest
        ):
            raise source_model.SourceError("history snapshot link")
        sequences.append(footer.sequence)
        lengths.append(current_len)
        if footer.previous_footer_offset == cow.ABSENT_OFFSET:
            if footer.sequence != 0:
                raise source_model.SourceError("history genesis")
            break

        snapshot = view.read_exact(
            footer.snapshot_offset,
            footer.snapshot_len,
            "history snapshot",
        )
        (
            _magic,
            _sequence,
            _root_offset,
            _root_level,
            _root_digest,
            parent_snapshot_digest,
        ) = cow.SNAPSHOT.unpack(snapshot)
        if footer.sequence == 0:
            raise source_model.SourceError("history underflow")
        expected_sequence = footer.sequence - 1
        expected_snapshot_digest = parent_snapshot_digest
        current_len = footer.previous_footer_offset + cow.FOOTER_LEN

    return HistoryReport(
        tuple(sequences),
        tuple(lengths),
        snapshot_stats(backing.stats),
    )


def main() -> None:
    values = [
        objects.ObjectInput(
            2 * object_id,
            1 + object_id % 3,
            f"payload:{2 * object_id}".encode("ascii"),
        )
        for object_id in range(1, OBJECTS + 1)
    ]
    genesis = objects.build_genesis(values)
    inserted = history_model.append_insert(
        genesis,
        objects.ObjectInput(101, 7, b"transient recovery object"),
    )
    deleted = history_model.append_delete(inserted, 101)

    fake_payload = (
        b"prefix-"
        + (cow.FOOTER_MAGIC + b"not-a-footer") * FAKE_MAGIC_COUNT
        + b"-suffix"
    )
    complete_latest = objects.append_replacement(
        deleted,
        objects.ObjectInput(2, 9, fake_payload),
    )
    interrupted = complete_latest[: -cow.FOOTER_LEN // 2]

    # Exact-end strict validation never falls back to recovery.
    try:
        source_model.strict_validate(
            source_model.CountingSource(
                interrupted, RecoveryLimits().source
            )
        )
    except cow.FormatError as error:
        strict_error = str(error)
    else:
        raise AssertionError("interrupted file passed exact-end validation")

    report = scan_valid_prefixes(interrupted, RecoveryLimits())
    sequences = tuple(result.sequence for result in report.results)
    assert sequences == (2, 1, 0)
    assert report.truncated_footer_matches == 1
    assert report.failed_candidates >= FAKE_MAGIC_COUNT
    assert report.candidates_validated == (
        report.failed_candidates + len(report.results)
    )
    assert report.source_stats.bytes_read > report.scan_bytes_read

    history = validate_history_at(
        interrupted,
        report.results[0].prefix_len,
        RecoveryLimits(),
    )
    assert history.sequences == (2, 1, 0)

    low_candidate_limits = replace(
        RecoveryLimits(), max_candidate_validations=FAKE_MAGIC_COUNT
    )
    try:
        scan_valid_prefixes(interrupted, low_candidate_limits)
    except source_model.SourceError as error:
        candidate_error = str(error)
    else:
        raise AssertionError("candidate-limited recovery claimed completeness")
    assert candidate_error == "recovery candidate limit"

    narrow = replace(
        RecoveryLimits(), max_scan_bytes=cow.FOOTER_LEN // 2
    )
    narrow_report = scan_valid_prefixes(interrupted, narrow)
    assert narrow_report.results == ()
    assert narrow_report.truncated_footer_matches == 1

    history_limit = replace(RecoveryLimits(), max_history_entries=2)
    try:
        validate_history_at(
            interrupted,
            report.results[0].prefix_len,
            history_limit,
        )
    except source_model.SourceError as error:
        history_limit_error = str(error)
    else:
        raise AssertionError("history limit was not enforced")
    assert history_limit_error == "history entry limit"

    print(f"file_bytes={len(interrupted):,}")
    print(f"scan_bytes={report.scan_bytes_read:,}")
    print(f"scan_reads={report.scan_read_operations}")
    print(f"magic_matches={report.magic_matches}")
    print(f"truncated_footer_matches={report.truncated_footer_matches}")
    print(f"candidates_validated={report.candidates_validated}")
    print(f"failed_candidates={report.failed_candidates}")
    print(f"recovered_sequences={sequences}")
    print(f"recovery_total_bytes_read={report.source_stats.bytes_read:,}")
    print(f"verified_history_sequences={history.sequences}")
    print(f"candidate_limit_failure={candidate_error}")
    print(f"history_limit_failure={history_limit_error}")
    print(f"strict_interrupted_failure={strict_error}")
    print("recovery_does_not_select_candidate=pass")
    print("failed_candidate_work_is_charged=pass")
    print("every_reported_prefix_is_strictly_validated=pass")
    print("finding=footer magic is candidate evidence and never publication authority")
    print("finding=recovery and exact-end validation require separate APIs")
    print("finding=verified source history revalidates every exact ancestor prefix")


if __name__ == "__main__":
    main()
