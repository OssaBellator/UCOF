#!/usr/bin/env python3
"""Bounded arbitrary-depth mixed-operation immutable-page batch planner."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from hashlib import sha256
import random

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_objects as objects

OBJECTS = 100_000
MAX_OPERATIONS = 1024
MAX_NEW_OBJECT_BYTES = 16 * 1024 * 1024
MAX_NEW_PAGES = 100_000
MAX_OUTPUT_BYTES = 512 * 1024 * 1024


class OperationKind(Enum):
    INSERT = "insert"
    REPLACE = "replace"
    DELETE = "delete"


@dataclass(frozen=True)
class Operation:
    kind: OperationKind
    object_id: int
    object_kind: int = 0
    payload: bytes = b""


@dataclass(frozen=True)
class BatchReport:
    bytes: bytes
    sequence: int
    final_objects: int
    new_object_records: int
    new_object_bytes: int
    new_pages: int
    reused_pages: int
    retired_pages: int
    root_level: int
    final_root_digest: bytes


def page_reference(data: bytes, offset: int) -> cow.PageRef:
    page = data[offset : offset + cow.PAGE_SIZE]
    if len(page) != cow.PAGE_SIZE:
        raise ValueError("page range")
    fields = cow.PAGE_HEADER.unpack_from(page)
    return cow.PageRef(
        fields[6],
        fields[7],
        offset,
        fields[2],
        cow.digest(cow.PAGE_DOMAIN, page),
    )


def old_page_index(
    data: bytes, report: objects.CompleteReport
) -> dict[tuple[int, int, int, bytes], cow.PageRef]:
    result: dict[tuple[int, int, int, bytes], cow.PageRef] = {}
    for offset in report.structural.reachable_pages:
        reference = page_reference(data, offset)
        key = (
            reference.level,
            reference.minimum,
            reference.maximum,
            reference.digest,
        )
        if key in result:
            raise AssertionError("duplicate old page identity")
        result[key] = reference
    return result


def canonical_operations(operations: list[Operation]) -> list[Operation]:
    if len(operations) > MAX_OPERATIONS:
        raise ValueError("operation limit")
    ordered = sorted(operations, key=lambda operation: operation.object_id)
    if any(operation.object_id <= 0 for operation in ordered):
        raise ValueError("object identifier")
    if any(
        left.object_id == right.object_id
        for left, right in zip(ordered, ordered[1:])
    ):
        raise ValueError("conflicting operations")
    for operation in ordered:
        if operation.kind in (OperationKind.INSERT, OperationKind.REPLACE):
            if operation.object_kind <= 0:
                raise ValueError("object kind")
        elif operation.object_kind != 0 or operation.payload:
            raise ValueError("delete payload")
    return ordered


def append_or_reuse(
    output: bytearray,
    page: bytes,
    old_pages: dict[tuple[int, int, int, bytes], cow.PageRef],
    emitted: list[cow.PageRef],
) -> cow.PageRef:
    fields = cow.PAGE_HEADER.unpack_from(page)
    digest = cow.digest(cow.PAGE_DOMAIN, page)
    key = (fields[2], fields[6], fields[7], digest)
    existing = old_pages.get(key)
    if existing is not None:
        if bytes(
            output[existing.offset : existing.offset + cow.PAGE_SIZE]
        ) != page:
            raise AssertionError("digest collision in experiment")
        return existing
    if len(emitted) >= MAX_NEW_PAGES:
        raise ValueError("new-page limit")
    reference = cow.append_page(output, page)
    emitted.append(reference)
    return reference


def build_canonical_reusing(
    output: bytearray,
    locators: list[cow.Locator],
    old_pages: dict[tuple[int, int, int, bytes], cow.PageRef],
) -> tuple[cow.PageRef, list[cow.PageRef]]:
    if not locators:
        raise ValueError("empty final directory")
    if any(
        left.object_id >= right.object_id
        for left, right in zip(locators, locators[1:])
    ):
        raise ValueError("final object order")

    emitted: list[cow.PageRef] = []
    level = [
        append_or_reuse(
            output,
            cow.encode_leaf(locators[index : index + cow.LEAF_CAPACITY]),
            old_pages,
            emitted,
        )
        for index in range(0, len(locators), cow.LEAF_CAPACITY)
    ]
    while len(level) > 1:
        next_level: list[cow.PageRef] = []
        for index in range(0, len(level), cow.INTERNAL_FANOUT):
            children = level[index : index + cow.INTERNAL_FANOUT]
            page = cow.encode_internal(children, children[0].level + 1)
            next_level.append(
                append_or_reuse(output, page, old_pages, emitted)
            )
        level = next_level
    return level[0], emitted


def apply_batch(data: bytes, operations: list[Operation]) -> BatchReport:
    previous = objects.validate_complete(data)
    ordered = canonical_operations(operations)
    active = {locator.object_id: locator for locator in previous.objects}
    output = bytearray(data)
    new_records = 0
    new_object_bytes = 0

    for operation in ordered:
        exists = operation.object_id in active
        if operation.kind == OperationKind.INSERT and exists:
            raise ValueError("insert existing object")
        if operation.kind in (OperationKind.REPLACE, OperationKind.DELETE) and not exists:
            raise ValueError("operation missing object")
        if operation.kind == OperationKind.DELETE:
            del active[operation.object_id]
            continue

        before = len(output)
        locator = objects.append_object(
            output,
            objects.ObjectInput(
                operation.object_id,
                operation.object_kind,
                operation.payload,
            ),
        )
        new_records += 1
        new_object_bytes += len(output) - before
        if new_object_bytes > MAX_NEW_OBJECT_BYTES:
            raise ValueError("new-object byte limit")
        active[operation.object_id] = locator

    locators = [active[identifier] for identifier in sorted(active)]
    old_pages = old_page_index(data, previous)
    root, emitted = build_canonical_reusing(output, locators, old_pages)
    if len(output) > MAX_OUTPUT_BYTES:
        raise ValueError("output byte limit")

    cow.publish(
        output,
        previous.structural.sequence + 1,
        root,
        previous.structural.snapshot_digest,
        previous.structural.footer_offset,
        len(emitted),
    )
    result = bytes(output)
    verified = objects.validate_complete(result)
    if tuple(locator.object_id for locator in verified.objects) != tuple(
        locator.object_id for locator in locators
    ):
        raise AssertionError("verified final identifiers differ")

    new_reachable = (
        verified.structural.reachable_pages
        - previous.structural.reachable_pages
    )
    reused = (
        verified.structural.reachable_pages
        & previous.structural.reachable_pages
    )
    retired = (
        previous.structural.reachable_pages
        - verified.structural.reachable_pages
    )
    if len(new_reachable) != len(emitted):
        raise AssertionError("emitted page accounting")
    return BatchReport(
        result,
        verified.structural.sequence,
        len(verified.objects),
        new_records,
        new_object_bytes,
        len(new_reachable),
        len(reused),
        len(retired),
        verified.structural.root.level,
        verified.structural.root.digest,
    )


def operation_set() -> list[Operation]:
    return [
        Operation(
            OperationKind.REPLACE,
            2,
            9,
            b"replacement:2",
        ),
        Operation(
            OperationKind.REPLACE,
            20_000,
            8,
            b"replacement:20000",
        ),
        Operation(
            OperationKind.REPLACE,
            199_998,
            7,
            b"replacement:199998",
        ),
        Operation(
            OperationKind.INSERT,
            101,
            6,
            b"inserted:101",
        ),
        Operation(
            OperationKind.INSERT,
            100_001,
            6,
            b"inserted:100001",
        ),
        Operation(OperationKind.DELETE, 50_000),
        Operation(OperationKind.DELETE, 150_000),
    ]


def main() -> None:
    values = [
        objects.ObjectInput(
            2 * object_id,
            1 + object_id % 5,
            f"payload:{2 * object_id}".encode("ascii"),
        )
        for object_id in range(1, OBJECTS + 1)
    ]
    genesis = objects.build_genesis(values)
    genesis_report = objects.validate_complete(genesis)
    assert genesis_report.structural.root.level == 2

    operations = operation_set()
    first = apply_batch(genesis, operations)
    shuffled = operations[:]
    random.Random(20260730).shuffle(shuffled)
    second = apply_batch(genesis, shuffled)
    assert first == second
    assert first.sequence == 1
    assert first.final_objects == OBJECTS
    assert first.new_object_records == 5
    assert first.bytes.startswith(genesis)

    final = objects.validate_complete(first.bytes)
    assert final.object_payloads[2] == b"replacement:2"
    assert final.object_payloads[20_000] == b"replacement:20000"
    assert final.object_payloads[199_998] == b"replacement:199998"
    assert final.object_payloads[101] == b"inserted:101"
    assert final.object_payloads[100_001] == b"inserted:100001"
    assert 50_000 not in final.object_payloads
    assert 150_000 not in final.object_payloads

    no_op = apply_batch(genesis, [])
    assert no_op.new_pages == 0
    assert no_op.reused_pages == len(
        genesis_report.structural.reachable_pages
    )
    assert no_op.final_root_digest == genesis_report.structural.root.digest

    replacement_only = apply_batch(
        genesis,
        [
            operation
            for operation in operations
            if operation.kind == OperationKind.REPLACE
        ],
    )
    assert replacement_only.new_pages < first.new_pages

    failure_cases = [
        (
            [Operation(OperationKind.INSERT, 2, 1, b"duplicate")],
            "insert existing object",
        ),
        (
            [Operation(OperationKind.DELETE, 3)],
            "operation missing object",
        ),
        (
            [
                Operation(OperationKind.DELETE, 2),
                Operation(OperationKind.REPLACE, 2, 1, b"conflict"),
            ],
            "conflicting operations",
        ),
    ]
    for invalid, expected in failure_cases:
        try:
            apply_batch(genesis, invalid)
        except ValueError as error:
            actual = str(error)
        else:
            raise AssertionError("invalid mixed batch was accepted")
        assert actual == expected

    print(f"genesis_bytes={len(genesis):,}")
    print(
        "genesis_pages="
        f"{len(genesis_report.structural.reachable_pages):,}"
    )
    print(f"genesis_root_level={genesis_report.structural.root.level}")
    print(f"operations={len(operations)}")
    print(f"new_object_records={first.new_object_records}")
    print(f"new_object_bytes={first.new_object_bytes:,}")
    print(f"mixed_new_pages={first.new_pages:,}")
    print(f"mixed_reused_pages={first.reused_pages:,}")
    print(f"mixed_retired_pages={first.retired_pages:,}")
    print(f"replacement_only_new_pages={replacement_only.new_pages:,}")
    print(f"final_root_level={first.root_level}")
    print(f"final_sha256={sha256(first.bytes).hexdigest()}")
    print("shuffled_operation_order_same_bytes=pass")
    print("no_op_reuses_exact_root_and_all_pages=pass")
    print("invalid_operation_conflicts_fail_closed=pass")
    print("finding=mixed operations can be canonicalized before one immutable publication")
    print("finding=exact content reuse naturally shares unaffected pages across the whole batch")
    print("finding=insertions and deletions can cause wider canonical repacking than replacements")


if __name__ == "__main__":
    main()
