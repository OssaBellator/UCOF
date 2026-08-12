#!/usr/bin/env python3
"""Independent canonical half-full construction for immutable successor pages."""

from __future__ import annotations

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_objects as objects

LEAF_MIN_OCCUPANCY = (cow.LEAF_CAPACITY + 1) // 2
INTERNAL_MIN_OCCUPANCY = (cow.INTERNAL_FANOUT + 1) // 2


def canonical_group_sizes(total: int, capacity: int, minimum: int) -> list[int]:
    if total <= 0 or capacity <= 0 or minimum <= 0 or minimum > capacity:
        raise ValueError("canonical occupancy")
    groups = (total + capacity - 1) // capacity
    if groups == 1:
        return [total]

    full_groups, remainder = divmod(total, capacity)
    if remainder == 0:
        sizes = [capacity] * full_groups
    elif remainder >= minimum:
        sizes = [capacity] * full_groups + [remainder]
    else:
        transfer = minimum - remainder
        sizes = [capacity] * (full_groups - 1)
        sizes.extend([capacity - transfer, minimum])

    if (
        len(sizes) != groups
        or sum(sizes) != total
        or any(size < minimum or size > capacity for size in sizes)
    ):
        raise AssertionError("canonical occupancy partition")
    return sizes


def build_tree(output: bytearray, entries: list[cow.Locator]) -> cow.PageRef:
    if not entries:
        raise ValueError("empty directory")
    ordered = sorted(entries, key=lambda entry: entry.object_id)
    if any(
        left.object_id >= right.object_id
        for left, right in zip(ordered, ordered[1:])
    ):
        raise ValueError("locator identifiers must be strictly ordered")

    current: list[cow.PageRef] = []
    start = 0
    for size in canonical_group_sizes(
        len(ordered), cow.LEAF_CAPACITY, LEAF_MIN_OCCUPANCY
    ):
        end = start + size
        current.append(cow.append_page(output, cow.encode_leaf(ordered[start:end])))
        start = end

    level = 1
    while len(current) > 1:
        next_level: list[cow.PageRef] = []
        start = 0
        for size in canonical_group_sizes(
            len(current), cow.INTERNAL_FANOUT, INTERNAL_MIN_OCCUPANCY
        ):
            end = start + size
            next_level.append(
                cow.append_page(output, cow.encode_internal(current[start:end], level))
            )
            start = end
        current = next_level
        level += 1
    return current[0]


def build_genesis(values: list[objects.ObjectInput]) -> bytes:
    ordered = sorted(values, key=lambda value: value.object_id)
    if not ordered or any(
        left.object_id == right.object_id
        for left, right in zip(ordered, ordered[1:])
    ):
        raise ValueError("objects must contain unique identifiers")

    output = bytearray(cow.FILE_HEADER_LEN)
    output[: len(cow.FILE_MAGIC)] = cow.FILE_MAGIC
    locators = [objects.append_object(output, value) for value in ordered]
    page_start = len(output)
    root = build_tree(output, locators)
    page_count = (len(output) - page_start) // cow.PAGE_SIZE
    cow.publish(output, 0, root, bytes(32), cow.ABSENT_OFFSET, page_count)
    result = bytes(output)
    objects.validate_complete(result)
    return result


def page_counts(data: bytes) -> list[tuple[int, int, bool]]:
    report = objects.validate_complete(data)
    root = report.structural.root
    stack = [(root, True)]
    result: list[tuple[int, int, bool]] = []
    while stack:
        reference, is_root = stack.pop()
        page = cow.checked_slice(data, reference.offset, cow.PAGE_SIZE, "page")
        _magic, kind, _level, _reserved, count, _entry_size, _minimum, _maximum, _tail = cow.PAGE_HEADER.unpack_from(page)
        result.append((kind, count, is_root))
        decoded_kind, entries = cow.decode_page(data, reference)
        if decoded_kind == 2:
            children = [entry for entry in entries if isinstance(entry, cow.PageRef)]
            stack.extend((child, False) for child in reversed(children))
    return result


def validate_canonical_occupancy(data: bytes) -> None:
    for kind, count, is_root in page_counts(data):
        if kind == 1:
            if not is_root and count < LEAF_MIN_OCCUPANCY:
                raise objects.ObjectError("leaf occupancy")
        elif kind == 2:
            minimum = 2 if is_root else INTERNAL_MIN_OCCUPANCY
            if count < minimum:
                raise objects.ObjectError("internal occupancy")
        else:
            raise objects.ObjectError("page kind")


def main() -> None:
    assert canonical_group_sizes(
        cow.LEAF_CAPACITY + 1, cow.LEAF_CAPACITY, LEAF_MIN_OCCUPANCY
    ) == [cow.LEAF_CAPACITY + 1 - LEAF_MIN_OCCUPANCY, LEAF_MIN_OCCUPANCY]
    assert canonical_group_sizes(
        400, cow.LEAF_CAPACITY, LEAF_MIN_OCCUPANCY
    ) == [cow.LEAF_CAPACITY, 122, LEAF_MIN_OCCUPANCY]
    values = [
        objects.ObjectInput(
            object_id,
            1 + object_id % 5,
            f"payload:{object_id}".encode("ascii"),
        )
        for object_id in range(1, 401)
    ]
    data = build_genesis(values)
    validate_canonical_occupancy(data)
    print(f"leaf_minimum={LEAF_MIN_OCCUPANCY}")
    print(f"internal_minimum={INTERNAL_MIN_OCCUPANCY}")
    print("object_400_leaf_counts=185,122,93")
    print("canonical_occupancy=pass")


if __name__ == "__main__":
    main()
