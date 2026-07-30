#!/usr/bin/env python3
"""Recursive immutable-page deletion, rebalance, merge, and root collapse."""

from __future__ import annotations

from dataclasses import dataclass

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_splits as tree

MIN_LEAF_ENTRIES = (cow.LEAF_CAPACITY + 1) // 2
MIN_INTERNAL_CHILDREN = (cow.INTERNAL_FANOUT + 1) // 2


@dataclass(frozen=True)
class DeleteResult:
    reference: cow.PageRef
    underfull: bool


def locators(data: bytes, reference: cow.PageRef) -> list[cow.Locator]:
    kind, entries = cow.decode_page(data, reference)
    if kind != 1:
        raise ValueError("expected leaf")
    return [entry for entry in entries if isinstance(entry, cow.Locator)]


def children(data: bytes, reference: cow.PageRef) -> list[cow.PageRef]:
    kind, entries = cow.decode_page(data, reference)
    if kind != 2:
        raise ValueError("expected internal page")
    return [entry for entry in entries if isinstance(entry, cow.PageRef)]


def combine_siblings(
    output: bytearray,
    source: bytes,
    left: cow.PageRef,
    right: cow.PageRef,
) -> list[cow.PageRef]:
    if left.level != right.level or left.maximum >= right.minimum:
        raise ValueError("incompatible siblings")
    if left.level == 0:
        combined = locators(source, left) + locators(source, right)
        if len(combined) <= cow.LEAF_CAPACITY:
            return [tree.emit_leaf(output, combined)]
        split = (len(combined) + 1) // 2
        return [
            tree.emit_leaf(output, combined[:split]),
            tree.emit_leaf(output, combined[split:]),
        ]

    combined_children = children(source, left) + children(source, right)
    if len(combined_children) <= cow.INTERNAL_FANOUT:
        return [tree.emit_internal(output, combined_children, left.level)]
    split = (len(combined_children) + 1) // 2
    return [
        tree.emit_internal(output, combined_children[:split], left.level),
        tree.emit_internal(output, combined_children[split:], left.level),
    ]


def delete_node(
    output: bytearray,
    source: bytes,
    reference: cow.PageRef,
    object_id: int,
    *,
    is_root: bool,
) -> DeleteResult:
    kind, entries = cow.decode_page(source, reference)
    if kind == 1:
        values = [entry for entry in entries if isinstance(entry, cow.Locator)]
        kept = [entry for entry in values if entry.object_id != object_id]
        if len(kept) == len(values):
            raise KeyError(object_id)
        if not kept:
            raise ValueError("prototype does not encode an empty tree")
        replacement = tree.emit_leaf(output, kept)
        return DeleteResult(
            replacement,
            underfull=not is_root and len(kept) < MIN_LEAF_ENTRIES,
        )

    page_children = [entry for entry in entries if isinstance(entry, cow.PageRef)]
    index = next(
        (
            position
            for position, child in enumerate(page_children)
            if child.minimum <= object_id <= child.maximum
        ),
        None,
    )
    if index is None:
        raise KeyError(object_id)

    child_result = delete_node(
        output,
        source,
        page_children[index],
        object_id,
        is_root=False,
    )
    updated = page_children[:]
    updated[index] = child_result.reference

    if child_result.underfull:
        if index + 1 < len(updated):
            left_index = index
            right_index = index + 1
        elif index > 0:
            left_index = index - 1
            right_index = index
        else:
            raise ValueError("underfull child has no sibling")
        replacements = combine_siblings(
            output,
            source,
            updated[left_index],
            updated[right_index],
        )
        updated = (
            updated[:left_index]
            + replacements
            + updated[right_index + 1 :]
        )

    if is_root and len(updated) == 1:
        return DeleteResult(updated[0], underfull=False)

    replacement = tree.emit_internal(output, updated, reference.level)
    return DeleteResult(
        replacement,
        underfull=not is_root and len(updated) < MIN_INTERNAL_CHILDREN,
    )


def delete(data: bytes, root: cow.PageRef, object_id: int) -> tuple[bytes, cow.PageRef]:
    output = bytearray(data)
    result = delete_node(output, data, root, object_id, is_root=True)
    return bytes(output), result.reference


def main() -> None:
    object_count = cow.LEAF_CAPACITY * cow.INTERNAL_FANOUT
    entries = [cow.locator(object_id) for object_id in range(2, 2 * object_count + 1, 2)]
    original_bytes = bytearray()
    original_root = cow.build_tree(original_bytes, entries)
    original = tree.validate_tree(bytes(original_bytes), original_root)

    inserted_bytes, inserted_root = tree.insert(
        bytes(original_bytes), original_root, cow.locator(1, 1)
    )
    inserted = tree.validate_tree(inserted_bytes, inserted_root)
    assert inserted_root.level == 2

    deleted_bytes, deleted_root = delete(inserted_bytes, inserted_root, 1)
    repeated_bytes, repeated_root = delete(inserted_bytes, inserted_root, 1)
    assert deleted_bytes == repeated_bytes
    assert deleted_root == repeated_root

    deleted = tree.validate_tree(deleted_bytes, deleted_root)
    assert deleted_root.level == 1
    assert deleted.identifiers == original.identifiers

    new_pages = deleted.reachable - inserted.reachable
    reused_pages = deleted.reachable & inserted.reachable
    retired_pages = inserted.reachable - deleted.reachable
    assert len(new_pages) == 2  # merged leaf and merged level-one root
    assert len(reused_pages) == cow.INTERNAL_FANOUT - 1
    assert len(retired_pages) == 5

    # A non-underflow deletion from a level-two tree copies one page per level.
    richer_bytes, richer_root = tree.insert(
        inserted_bytes, inserted_root, cow.locator(3, 2)
    )
    richer = tree.validate_tree(richer_bytes, richer_root)
    ordinary_bytes, ordinary_root = delete(richer_bytes, richer_root, 3)
    ordinary = tree.validate_tree(ordinary_bytes, ordinary_root)
    assert ordinary_root.level == 2
    assert ordinary.identifiers == inserted.identifiers
    ordinary_new = ordinary.reachable - richer.reachable
    assert len(ordinary_new) == 3

    print(f"objects={object_count:,}")
    print(f"minimum_leaf_entries={MIN_LEAF_ENTRIES}")
    print(f"minimum_internal_children={MIN_INTERNAL_CHILDREN}")
    print(f"recursive_delete_new_pages={len(new_pages)}")
    print(f"recursive_delete_reused_pages={len(reused_pages):,}")
    print(f"recursive_delete_retired_pages={len(retired_pages)}")
    print(f"collapsed_root_level={deleted_root.level}")
    print(f"ordinary_level_two_delete_new_pages={len(ordinary_new)}")
    print("deterministic_recursive_delete=pass")
    print("finding=immutable deletion can propagate underflow and collapse a level-two root")
    print("finding=the split inverse emits two pages without a historical content cache")
    print("finding=ordinary deep deletion copies one page per level")


if __name__ == "__main__":
    main()
