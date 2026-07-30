#!/usr/bin/env python3
"""Reuse an exact historical immutable root after an inverse update."""

from __future__ import annotations

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_splits as tree


def internal_children(data: bytes, reference: cow.PageRef) -> list[cow.PageRef]:
    kind, entries = cow.decode_page(data, reference)
    if kind != 2:
        raise ValueError("expected internal page")
    return [entry for entry in entries if isinstance(entry, cow.PageRef)]


def main() -> None:
    # Exactly one full internal root with 255 full leaves.
    object_count = cow.LEAF_CAPACITY * cow.INTERNAL_FANOUT
    entries = [cow.locator(object_id) for object_id in range(2, 2 * object_count + 1, 2)]
    original_bytes = bytearray()
    original_root = cow.build_tree(original_bytes, entries)
    original = tree.validate_tree(bytes(original_bytes), original_root)
    assert original_root.level == 1

    inserted_bytes, inserted_root = tree.insert(
        bytes(original_bytes), original_root, cow.locator(1, 1)
    )
    inserted = tree.validate_tree(inserted_bytes, inserted_root)
    assert inserted_root.level == 2

    # The general height-one delete cannot operate on a level-two root. Decode
    # the split structure and prove the inverse leaf contents match the exact
    # historical first leaf.
    top_children = internal_children(inserted_bytes, inserted_root)
    assert len(top_children) == 2
    left_children = internal_children(inserted_bytes, top_children[0])
    right_children = internal_children(inserted_bytes, top_children[1])
    assert len(left_children) + len(right_children) == cow.INTERNAL_FANOUT + 1

    first_entries = tree.leaf_entries(inserted_bytes, left_children[0])
    second_entries = tree.leaf_entries(inserted_bytes, left_children[1])
    merged_entries = [entry for entry in first_entries + second_entries if entry.object_id != 1]
    merged_entries.sort(key=lambda entry: entry.object_id)
    assert len(merged_entries) == cow.LEAF_CAPACITY

    original_children = internal_children(bytes(original_bytes), original_root)
    original_first = original_children[0]
    merged_page = cow.encode_leaf(merged_entries)
    original_page = bytes(original_bytes)[
        original_first.offset : original_first.offset + cow.PAGE_SIZE
    ]
    assert merged_page == original_page
    assert cow.digest(cow.PAGE_DOMAIN, merged_page) == original_first.digest

    # Reusing that historical leaf reconstructs the exact original child list,
    # so the exact historical root page can also be reused without emission.
    reconstructed_children = [original_first] + left_children[2:] + right_children
    assert reconstructed_children == original_children
    reconstructed_root_page = cow.encode_internal(reconstructed_children, 1)
    original_root_page = bytes(original_bytes)[
        original_root.offset : original_root.offset + cow.PAGE_SIZE
    ]
    assert reconstructed_root_page == original_root_page
    assert cow.digest(cow.PAGE_DOMAIN, reconstructed_root_page) == original_root.digest

    reverted = tree.validate_tree(inserted_bytes, original_root)
    assert reverted.identifiers == original.identifiers
    assert reverted.root == original.root
    assert reverted.reachable == original.reachable

    inserted_only_pages = inserted.reachable - original.reachable
    reverted_reused_pages = reverted.reachable & original.reachable
    assert len(inserted_only_pages) == 5
    assert len(reverted_reused_pages) == len(original.reachable)

    # Without historical content lookup, a deterministic inverse merge would
    # need one merged leaf and one replacement internal root. Content identity
    # permits both to be replaced by the earlier exact root reference.
    ordinary_inverse_pages = 2
    content_reversion_pages = 0

    print(f"objects={object_count:,}")
    print(f"original_pages={len(original.reachable):,}")
    print(f"inserted_pages={len(inserted.reachable):,}")
    print(f"inserted_only_pages={len(inserted_only_pages)}")
    print(f"reverted_reused_pages={len(reverted_reused_pages):,}")
    print(f"ordinary_inverse_pages={ordinary_inverse_pages}")
    print(f"content_reversion_pages={content_reversion_pages}")
    print("historical_leaf_byte_identity=pass")
    print("historical_root_byte_identity=pass")
    print("new_snapshot_can_reference_old_root=pass")
    print("finding=immutable content identity permits exact root resurrection after inverse updates")
    print("finding=content-indexed writers can eliminate even merge-path page writes")
    print("finding=structural root reuse does not reuse snapshot or commit identity")


if __name__ == "__main__":
    main()
