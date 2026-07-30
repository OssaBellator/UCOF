#!/usr/bin/env python3
"""Recursive immutable-page internal split and level-two root prototype."""

from __future__ import annotations

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_splits as tree

OBJECTS = cow.LEAF_CAPACITY * cow.INTERNAL_FANOUT


def main() -> None:
    entries = [cow.locator(object_id) for object_id in range(2, 2 * OBJECTS + 1, 2)]
    data = bytearray()
    root = cow.build_tree(data, entries)
    original = tree.validate_tree(bytes(data), root)

    assert root.level == 1
    assert len(original.reachable) == cow.INTERNAL_FANOUT + 1
    assert len(original.identifiers) == OBJECTS

    inserted_bytes, inserted_root = tree.insert(bytes(data), root, cow.locator(1, 1))
    repeated_bytes, repeated_root = tree.insert(bytes(data), root, cow.locator(1, 1))
    assert inserted_bytes == repeated_bytes
    assert inserted_root == repeated_root

    inserted = tree.validate_tree(inserted_bytes, inserted_root)
    assert inserted_root.level == 2
    assert inserted.identifiers[0] == 1
    assert len(inserted.identifiers) == OBJECTS + 1

    new_pages = inserted.reachable - original.reachable
    reused_pages = inserted.reachable & original.reachable
    retired_pages = original.reachable - inserted.reachable

    # Full leaf -> two leaves; full internal root -> two level-1 pages; new level-2 root.
    assert len(new_pages) == 5
    assert len(reused_pages) == cow.INTERNAL_FANOUT - 1
    assert len(retired_pages) == 2

    # A subsequent non-splitting insertion into a reused distant leaf copies
    # one leaf and one page per internal level.
    second_identifier = 2 * OBJECTS + 1
    second_bytes, second_root = tree.insert(
        inserted_bytes, inserted_root, cow.locator(second_identifier, 2)
    )
    second = tree.validate_tree(second_bytes, second_root)
    assert second_root.level == 2
    assert second.identifiers[-1] == second_identifier
    second_new = second.reachable - inserted.reachable
    second_reused = second.reachable & inserted.reachable
    assert len(second_new) == 3
    assert len(second_reused) == len(inserted.reachable) - 3

    full_rebuild_bytes = len(original.reachable) * cow.PAGE_SIZE
    recursive_split_bytes = len(new_pages) * cow.PAGE_SIZE
    second_path_bytes = len(second_new) * cow.PAGE_SIZE

    print(f"objects_before={OBJECTS:,}")
    print(f"leaf_capacity={cow.LEAF_CAPACITY}")
    print(f"internal_fanout={cow.INTERNAL_FANOUT}")
    print(f"original_pages={len(original.reachable):,}")
    print(f"recursive_split_new_pages={len(new_pages)}")
    print(f"recursive_split_reused_pages={len(reused_pages):,}")
    print(f"recursive_split_retired_pages={len(retired_pages)}")
    print(f"recursive_split_root_level={inserted_root.level}")
    print(f"second_insert_new_pages={len(second_new)}")
    print(f"second_insert_reused_pages={len(second_reused):,}")
    print(f"full_rebuild_page_bytes={full_rebuild_bytes:,}")
    print(f"recursive_split_page_bytes={recursive_split_bytes:,}")
    print(f"second_path_page_bytes={second_path_bytes:,}")
    print("deterministic_recursive_split=pass")
    print("finding=immutable child references permit recursive split propagation")
    print("finding=overflowing a full internal root emits five pages and reuses 254 leaves")
    print("finding=ordinary level-two insertion returns to one copied page per level")


if __name__ == "__main__":
    main()
