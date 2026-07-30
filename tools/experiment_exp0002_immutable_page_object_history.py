#!/usr/bin/env python3
"""Complete-object insertion, deletion, and verified-history boundaries."""

from __future__ import annotations

from dataclasses import dataclass

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_objects as objects
import experiment_exp0002_immutable_page_recursive_delete as recursive_delete
import experiment_exp0002_immutable_page_splits as tree

OBJECTS = 10_000
INSERTED_ID = 101


@dataclass(frozen=True)
class HistoryReport:
    sequences: tuple[int, ...]
    object_counts: tuple[int, ...]


def publish_after(
    output: bytearray,
    previous: objects.CompleteReport,
    root: cow.PageRef,
    page_count: int,
) -> bytes:
    cow.publish(
        output,
        previous.structural.sequence + 1,
        root,
        previous.structural.snapshot_digest,
        previous.structural.footer_offset,
        page_count,
    )
    return bytes(output)


def append_insert(data: bytes, value: objects.ObjectInput) -> bytes:
    previous = objects.validate_complete(data)
    if value.object_id in previous.object_payloads:
        raise ValueError("duplicate object insertion")

    with_object = bytearray(data)
    locator = objects.append_object(with_object, value)
    page_start = len(with_object)
    tree_bytes, root = tree.insert(
        bytes(with_object), previous.structural.root, locator
    )
    output = bytearray(tree_bytes)
    page_bytes = len(output) - page_start
    if page_bytes % cow.PAGE_SIZE:
        raise AssertionError("inserted page bytes are not aligned")
    return publish_after(
        output,
        previous,
        root,
        page_bytes // cow.PAGE_SIZE,
    )


def append_delete(data: bytes, object_id: int) -> bytes:
    previous = objects.validate_complete(data)
    if object_id not in previous.object_payloads:
        raise KeyError(object_id)
    tree_bytes, root = recursive_delete.delete(
        data, previous.structural.root, object_id
    )
    output = bytearray(tree_bytes)
    page_bytes = len(output) - len(data)
    if page_bytes % cow.PAGE_SIZE:
        raise AssertionError("deleted page bytes are not aligned")
    return publish_after(
        output,
        previous,
        root,
        page_bytes // cow.PAGE_SIZE,
    )


def validate_history(data: bytes) -> HistoryReport:
    offset = len(data) - cow.FOOTER_LEN
    sequences: list[int] = []
    object_counts: list[int] = []
    expected_sequence: int | None = None

    while True:
        footer = cow.parse_footer(data, offset)
        if expected_sequence is not None and footer.sequence != expected_sequence:
            raise objects.ObjectError("history sequence")
        prefix = data[: offset + cow.FOOTER_LEN]
        report = objects.validate_complete(prefix)
        sequences.append(report.structural.sequence)
        object_counts.append(len(report.objects))
        if footer.previous_footer_offset == cow.ABSENT_OFFSET:
            break
        if footer.sequence == 0:
            raise objects.ObjectError("history underflow")
        expected_sequence = footer.sequence - 1
        offset = footer.previous_footer_offset

    if sequences[-1] != 0:
        raise objects.ObjectError("history genesis")
    return HistoryReport(tuple(sequences), tuple(object_counts))


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
    genesis_report = objects.validate_complete(genesis)

    inserted_value = objects.ObjectInput(
        INSERTED_ID, 7, b"transient inserted object"
    )
    inserted = append_insert(genesis, inserted_value)
    inserted_again = append_insert(genesis, inserted_value)
    assert inserted == inserted_again
    inserted_report = objects.validate_complete(inserted)
    assert inserted_report.structural.sequence == 1
    assert inserted_report.object_payloads[INSERTED_ID] == inserted_value.payload
    assert len(inserted_report.objects) == OBJECTS + 1

    deleted = append_delete(inserted, INSERTED_ID)
    deleted_again = append_delete(inserted, INSERTED_ID)
    assert deleted == deleted_again
    deleted_report = objects.validate_complete(deleted)
    assert deleted_report.structural.sequence == 2
    assert INSERTED_ID not in deleted_report.object_payloads
    assert tuple(locator.object_id for locator in deleted_report.objects) == tuple(
        locator.object_id for locator in genesis_report.objects
    )

    history = validate_history(deleted)
    assert history.sequences == (2, 1, 0)
    assert history.object_counts == (OBJECTS, OBJECTS + 1, OBJECTS)

    transient_locator = next(
        locator
        for locator in inserted_report.objects
        if locator.object_id == INSERTED_ID
    )
    tampered = bytearray(deleted)
    tampered[transient_locator.record_offset + objects.OBJECT_HEADER_LEN] ^= 1

    # Recompute only the exact sequence-1 commit digest. The active sequence-2
    # commit begins after that footer and authenticates the parent's snapshot
    # identity, not the parent's commit digest. This drives verified history past
    # the ancestor commit check to the stale locator's object-digest check.
    ancestor_end = inserted_report.structural.footer_offset + cow.FOOTER_LEN
    ancestor = bytearray(tampered[:ancestor_end])
    objects.reauthenticate_footer(ancestor)
    tampered[
        inserted_report.structural.footer_offset : ancestor_end
    ] = ancestor[inserted_report.structural.footer_offset : ancestor_end]

    # The latest active snapshot no longer references the deleted object. Active
    # validation therefore remains valid, while verified history must reject the
    # corrupted ancestor prefix.
    active_after_tamper = objects.validate_complete(bytes(tampered))
    assert INSERTED_ID not in active_after_tamper.object_payloads
    try:
        validate_history(bytes(tampered))
    except objects.ObjectError as error:
        history_error = str(error)
    else:
        raise AssertionError("tampered deleted object passed verified history")
    assert history_error == "object digest"

    interrupted = deleted[:-cow.FOOTER_LEN // 2]
    try:
        objects.validate_complete(interrupted)
    except objects.ObjectError:
        pass
    else:
        raise AssertionError("interrupted deletion validated")
    objects.validate_complete(inserted)

    insert_new_pages = len(
        inserted_report.structural.reachable_pages
        - genesis_report.structural.reachable_pages
    )
    delete_new_pages = len(
        deleted_report.structural.reachable_pages
        - inserted_report.structural.reachable_pages
    )
    print(f"genesis_objects={OBJECTS:,}")
    print(f"inserted_objects={len(inserted_report.objects):,}")
    print(f"deleted_objects={len(deleted_report.objects):,}")
    print(f"insert_new_pages={insert_new_pages}")
    print(f"delete_new_pages={delete_new_pages}")
    print(f"history_sequences={history.sequences}")
    print(f"history_object_counts={history.object_counts}")
    print(f"tampered_deleted_object_history_error={history_error}")
    print("deterministic_insert_delete=pass")
    print("active_snapshot_after_deleted_object_tamper=valid")
    print("verified_history_after_deleted_object_tamper=rejected")
    print("interrupted_delete_previous_prefix=valid")
    print("finding=active validation and verified history have intentionally different assurance scopes")
    print("finding=unreferenced historical objects require explicit ancestor validation")


if __name__ == "__main__":
    main()
