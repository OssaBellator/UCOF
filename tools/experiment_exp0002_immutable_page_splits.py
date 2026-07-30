#!/usr/bin/env python3
"""Immutable-page insertion, split, deletion, merge, and root-height prototype."""

from __future__ import annotations

from bisect import bisect_left
from dataclasses import dataclass

import experiment_exp0002_immutable_page_cow as cow

LEAF_CAPACITY = cow.LEAF_CAPACITY
MIN_LEAF_ENTRIES = (LEAF_CAPACITY + 1) // 2


@dataclass(frozen=True)
class TreeReport:
    root: cow.PageRef
    reachable: frozenset[int]
    identifiers: tuple[int, ...]


def make_locator(object_id: int, generation: int = 0) -> cow.Locator:
    return cow.locator(object_id, generation)


def validate_tree(data: bytes, root: cow.PageRef) -> TreeReport:
    stack = [root]
    seen: set[int] = set()
    identifiers: list[int] = []
    while stack:
        reference = stack.pop()
        if reference.offset in seen:
            raise cow.FormatError("page cycle")
        seen.add(reference.offset)
        kind, entries = cow.decode_page(data, reference)
        if kind == 1:
            identifiers.extend(entry.object_id for entry in entries if isinstance(entry, cow.Locator))
        else:
            children = [entry for entry in entries if isinstance(entry, cow.PageRef)]
            stack.extend(reversed(children))
    if any(left >= right for left, right in zip(identifiers, identifiers[1:])):
        raise cow.FormatError("tree identifier order")
    if not identifiers or (identifiers[0], identifiers[-1]) != (root.minimum, root.maximum):
        raise cow.FormatError("tree root range")
    return TreeReport(root, frozenset(seen), tuple(identifiers))


def emit_leaf(output: bytearray, entries: list[cow.Locator]) -> cow.PageRef:
    return cow.append_page(output, cow.encode_leaf(entries))


def emit_internal(output: bytearray, children: list[cow.PageRef], level: int) -> cow.PageRef:
    return cow.append_page(output, cow.encode_internal(children, level))


def insert_node(
    output: bytearray,
    source: bytes,
    reference: cow.PageRef,
    value: cow.Locator,
) -> list[cow.PageRef]:
    kind, entries = cow.decode_page(source, reference)
    if kind == 1:
        locators = [entry for entry in entries if isinstance(entry, cow.Locator)]
        identifiers = [entry.object_id for entry in locators]
        position = bisect_left(identifiers, value.object_id)
        if position < len(locators) and locators[position].object_id == value.object_id:
            raise ValueError("duplicate insertion")
        locators.insert(position, value)
        if len(locators) <= LEAF_CAPACITY:
            return [emit_leaf(output, locators)]
        split = (len(locators) + 1) // 2
        left = emit_leaf(output, locators[:split])
        right = emit_leaf(output, locators[split:])
        return [left, right]

    children = [entry for entry in entries if isinstance(entry, cow.PageRef)]
    child_index = next(
        (index for index, child in enumerate(children) if child.minimum <= value.object_id <= child.maximum),
        None,
    )
    if child_index is None:
        if value.object_id < children[0].minimum:
            child_index = 0
        elif value.object_id > children[-1].maximum:
            child_index = len(children) - 1
        else:
            raise ValueError("insertion falls into an unowned child gap")
    replacements = insert_node(output, source, children[child_index], value)
    updated = children[:child_index] + replacements + children[child_index + 1 :]
    if len(updated) <= cow.INTERNAL_FANOUT:
        return [emit_internal(output, updated, reference.level)]
    split = (len(updated) + 1) // 2
    left = emit_internal(output, updated[:split], reference.level)
    right = emit_internal(output, updated[split:], reference.level)
    return [left, right]


def insert(data: bytes, root: cow.PageRef, value: cow.Locator) -> tuple[bytes, cow.PageRef]:
    output = bytearray(data)
    replacements = insert_node(output, data, root, value)
    if len(replacements) == 1:
        return bytes(output), replacements[0]
    return bytes(output), emit_internal(output, replacements, root.level + 1)


def leaf_entries(data: bytes, reference: cow.PageRef) -> list[cow.Locator]:
    kind, entries = cow.decode_page(data, reference)
    if kind != 1:
        raise ValueError("expected leaf")
    return [entry for entry in entries if isinstance(entry, cow.Locator)]


def delete_from_height_one(data: bytes, root: cow.PageRef, object_id: int) -> tuple[bytes, cow.PageRef]:
    if root.level == 0:
        entries = leaf_entries(data, root)
        kept = [entry for entry in entries if entry.object_id != object_id]
        if len(kept) == len(entries):
            raise KeyError(object_id)
        if not kept:
            raise ValueError("prototype does not emit an empty tree")
        output = bytearray(data)
        return bytes(output), emit_leaf(output, kept)
    if root.level != 1:
        raise ValueError("prototype deletion supports height-one roots only")

    kind, decoded = cow.decode_page(data, root)
    if kind != 2:
        raise ValueError("root kind")
    children = [entry for entry in decoded if isinstance(entry, cow.PageRef)]
    index = next(
        (position for position, child in enumerate(children) if child.minimum <= object_id <= child.maximum),
        None,
    )
    if index is None:
        raise KeyError(object_id)

    target = leaf_entries(data, children[index])
    updated_target = [entry for entry in target if entry.object_id != object_id]
    if len(updated_target) == len(target):
        raise KeyError(object_id)
    if not updated_target:
        raise ValueError("prototype does not delete the final leaf entry")

    output = bytearray(data)
    if len(updated_target) >= MIN_LEAF_ENTRIES:
        replacement = emit_leaf(output, updated_target)
        updated_children = children[:index] + [replacement] + children[index + 1 :]
    else:
        sibling_index = index + 1 if index + 1 < len(children) else index - 1
        sibling = leaf_entries(data, children[sibling_index])
        combined = updated_target + sibling if index < sibling_index else sibling + updated_target
        combined.sort(key=lambda entry: entry.object_id)
        left_index = min(index, sibling_index)
        right_index = max(index, sibling_index)
        if len(combined) <= LEAF_CAPACITY:
            merged = emit_leaf(output, combined)
            updated_children = children[:left_index] + [merged] + children[right_index + 1 :]
        else:
            split = (len(combined) + 1) // 2
            left = emit_leaf(output, combined[:split])
            right = emit_leaf(output, combined[split:])
            updated_children = children[:left_index] + [left, right] + children[right_index + 1 :]

    if len(updated_children) == 1:
        return bytes(output), updated_children[0]
    return bytes(output), emit_internal(output, updated_children, 1)


def main() -> None:
    even_entries = [make_locator(object_id) for object_id in range(2, 402, 2)]
    genesis = bytearray()
    root = cow.build_tree(genesis, even_entries)
    original = validate_tree(bytes(genesis), root)
    assert root.level == 1
    assert len(original.reachable) == 3

    inserted_bytes, inserted_root = insert(bytes(genesis), root, make_locator(101, 1))
    inserted_again_bytes, inserted_again_root = insert(bytes(genesis), root, make_locator(101, 1))
    assert inserted_bytes == inserted_again_bytes
    assert inserted_root == inserted_again_root
    inserted = validate_tree(inserted_bytes, inserted_root)
    assert 101 in inserted.identifiers
    assert len(inserted.reachable - original.reachable) == 3
    assert len(inserted.reachable & original.reachable) == 1
    assert len(original.reachable - inserted.reachable) == 2

    deleted_bytes, deleted_root = delete_from_height_one(inserted_bytes, inserted_root, 101)
    deleted_again_bytes, deleted_again_root = delete_from_height_one(inserted_bytes, inserted_root, 101)
    assert deleted_bytes == deleted_again_bytes
    assert deleted_root == deleted_again_root
    deleted = validate_tree(deleted_bytes, deleted_root)
    assert deleted.identifiers == original.identifiers
    assert len(deleted.reachable - inserted.reachable) == 2
    assert len(deleted.reachable & original.reachable) == 1

    full_leaf_entries = [make_locator(object_id) for object_id in range(2, 2 * LEAF_CAPACITY + 1, 2)]
    full_leaf_bytes = bytearray()
    full_leaf_root = cow.build_tree(full_leaf_bytes, full_leaf_entries)
    assert full_leaf_root.level == 0
    raised_bytes, raised_root = insert(bytes(full_leaf_bytes), full_leaf_root, make_locator(101, 1))
    raised = validate_tree(raised_bytes, raised_root)
    assert raised_root.level == 1
    assert len(raised.reachable) == 3
    collapsed_bytes, collapsed_root = delete_from_height_one(raised_bytes, raised_root, 101)
    collapsed = validate_tree(collapsed_bytes, collapsed_root)
    assert collapsed_root.level == 0
    assert collapsed.identifiers == tuple(entry.object_id for entry in full_leaf_entries)

    try:
        insert(bytes(genesis), root, make_locator(100, 1))
    except ValueError:
        pass
    else:
        raise AssertionError("duplicate insertion was not rejected")

    print(f"leaf_capacity={LEAF_CAPACITY}")
    print(f"minimum_leaf_entries={MIN_LEAF_ENTRIES}")
    print("split_insert_new_pages=3")
    print("split_insert_reused_pages=1")
    print("merge_delete_new_pages=2")
    print("root_height_increase=pass")
    print("root_height_collapse=pass")
    print("deterministic_insert_bytes=pass")
    print("deterministic_delete_bytes=pass")
    print("duplicate_insert_rejection=pass")
    print("finding=immutable pages support deterministic append-only split and merge paths")
    print("finding=unaffected sibling pages remain exactly reusable through split and merge")
    print("finding=root height changes do not require rewriting unrelated pages")


if __name__ == "__main__":
    main()
