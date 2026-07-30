#!/usr/bin/env python3
"""Layer-targeted adversarial cases for the immutable-page microformat."""

from __future__ import annotations

from dataclasses import dataclass

import experiment_exp0002_immutable_page_cow as cow


@dataclass(frozen=True)
class Case:
    name: str
    expected: str
    bytes_value: bytes


def footer_offset(data: bytes | bytearray) -> int:
    return len(data) - cow.FOOTER_LEN


def snapshot_fields(data: bytes | bytearray) -> tuple:
    footer = cow.parse_footer(bytes(data), footer_offset(data))
    return cow.SNAPSHOT.unpack_from(data, footer.snapshot_offset)


def reauthenticate_footer(data: bytearray) -> None:
    offset = footer_offset(data)
    old = cow.parse_footer(bytes(data), offset)
    snapshot = bytes(data[old.snapshot_offset : old.snapshot_offset + old.snapshot_len])
    snapshot_digest = cow.digest(cow.SNAPSHOT_DOMAIN, snapshot)
    semantics = cow.footer_semantics(
        old.sequence,
        old.snapshot_offset,
        old.snapshot_len,
        old.previous_footer_offset,
        old.page_count_current,
        snapshot_digest,
    )
    commit_start = (
        0
        if old.previous_footer_offset == cow.ABSENT_OFFSET
        else old.previous_footer_offset + cow.FOOTER_LEN
    )
    commit_digest = cow.digest(
        cow.COMMIT_DOMAIN, bytes(data[commit_start:offset]) + semantics
    )
    cow.FOOTER.pack_into(
        data,
        offset,
        cow.FOOTER_MAGIC,
        old.sequence,
        old.snapshot_offset,
        old.snapshot_len,
        old.previous_footer_offset,
        old.page_count_current,
        snapshot_digest,
        commit_digest,
        bytes(16),
    )


def reauthenticate_root(data: bytearray) -> None:
    footer = cow.parse_footer(bytes(data), footer_offset(data))
    magic, sequence, root_offset, root_level, _root_digest, parent = cow.SNAPSHOT.unpack_from(
        data, footer.snapshot_offset
    )
    root_page = bytes(data[root_offset : root_offset + cow.PAGE_SIZE])
    root_digest = cow.digest(cow.PAGE_DOMAIN, root_page)
    cow.SNAPSHOT.pack_into(
        data,
        footer.snapshot_offset,
        magic,
        sequence,
        root_offset,
        root_level,
        root_digest,
        parent,
    )
    reauthenticate_footer(data)


def replace_child_digest(page: bytearray, child_index: int, child_digest: bytes) -> None:
    entry_offset = cow.PAGE_HEADER_LEN + child_index * cow.INTERNAL_ENTRY_LEN
    minimum, maximum, offset, length, _digest = cow.INTERNAL_ENTRY.unpack_from(
        page, entry_offset
    )
    cow.INTERNAL_ENTRY.pack_into(
        page, entry_offset, minimum, maximum, offset, length, child_digest
    )


def first_leaf_path(data: bytes | bytearray) -> tuple[int, int, int]:
    _magic, _sequence, root_offset, root_level, _root_digest, _parent = snapshot_fields(data)
    if root_level != 2:
        raise AssertionError("fixture must have a level-two root")
    root_entry = cow.INTERNAL_ENTRY.unpack_from(data, root_offset + cow.PAGE_HEADER_LEN)
    level_one_offset = root_entry[2]
    level_one_entry = cow.INTERNAL_ENTRY.unpack_from(
        data, level_one_offset + cow.PAGE_HEADER_LEN
    )
    leaf_offset = level_one_entry[2]
    return root_offset, level_one_offset, leaf_offset


def reauthenticate_first_leaf(data: bytearray) -> None:
    root_offset, level_one_offset, leaf_offset = first_leaf_path(data)
    leaf = bytes(data[leaf_offset : leaf_offset + cow.PAGE_SIZE])
    level_one = bytearray(data[level_one_offset : level_one_offset + cow.PAGE_SIZE])
    replace_child_digest(level_one, 0, cow.digest(cow.PAGE_DOMAIN, leaf))
    data[level_one_offset : level_one_offset + cow.PAGE_SIZE] = level_one

    root = bytearray(data[root_offset : root_offset + cow.PAGE_SIZE])
    replace_child_digest(level_one, 0, cow.digest(cow.PAGE_DOMAIN, leaf))
    replace_child_digest(root, 0, cow.digest(cow.PAGE_DOMAIN, bytes(level_one)))
    data[root_offset : root_offset + cow.PAGE_SIZE] = root
    reauthenticate_root(data)


def mutate_root_entry(data: bytearray, child_index: int, values: tuple[int, int, int, int, bytes]) -> None:
    root_offset = snapshot_fields(data)[2]
    cow.INTERNAL_ENTRY.pack_into(
        data,
        root_offset + cow.PAGE_HEADER_LEN + child_index * cow.INTERNAL_ENTRY_LEN,
        *values,
    )
    reauthenticate_root(data)


def expect(case: Case) -> None:
    try:
        cow.validate_strict(case.bytes_value)
    except cow.FormatError as error:
        actual = str(error)
        if actual != case.expected:
            raise AssertionError(
                f"{case.name}: expected {case.expected!r}, received {actual!r}"
            ) from error
        return
    raise AssertionError(f"{case.name}: malformed bytes validated")


def main() -> None:
    valid = cow.build_genesis([cow.locator(object_id) for object_id in range(1, cow.OBJECTS + 1)])
    cow.validate_strict(valid)
    root_offset, _level_one_offset, leaf_offset = first_leaf_path(valid)

    cases: list[Case] = []

    header = bytearray(valid)
    header[0] ^= 1
    cases.append(Case("header-magic", "header", bytes(header)))

    commit = bytearray(valid)
    commit[-32] ^= 1
    cases.append(Case("commit-digest", "commit digest", bytes(commit)))

    raw_leaf = bytearray(valid)
    raw_leaf[leaf_offset + cow.PAGE_HEADER_LEN] ^= 1
    reauthenticate_footer(raw_leaf)
    cases.append(Case("leaf-page-digest", "page digest", bytes(raw_leaf)))

    unordered = bytearray(valid)
    first = leaf_offset + cow.PAGE_HEADER_LEN
    second = first + cow.LEAF_ENTRY_LEN
    first_entry = bytes(unordered[first : first + cow.LEAF_ENTRY_LEN])
    second_entry = bytes(unordered[second : second + cow.LEAF_ENTRY_LEN])
    unordered[first : first + cow.LEAF_ENTRY_LEN] = second_entry
    unordered[second : second + cow.LEAF_ENTRY_LEN] = first_entry
    reauthenticate_first_leaf(unordered)
    cases.append(Case("leaf-order", "leaf order", bytes(unordered)))

    padding = bytearray(valid)
    padding[leaf_offset + cow.PAGE_SIZE - 1] = 1
    reauthenticate_first_leaf(padding)
    cases.append(Case("leaf-padding", "leaf padding", bytes(padding)))

    page_header = bytearray(valid)
    page_header[leaf_offset + 10] = 1  # reserved u16 inside page header
    reauthenticate_first_leaf(page_header)
    cases.append(Case("leaf-header-reserved", "page header", bytes(page_header)))

    child_digest = bytearray(valid)
    root_entry = list(
        cow.INTERNAL_ENTRY.unpack_from(
            child_digest, root_offset + cow.PAGE_HEADER_LEN
        )
    )
    root_entry[4] = bytes([7]) * 32
    mutate_root_entry(child_digest, 0, tuple(root_entry))
    cases.append(Case("child-digest", "page digest", bytes(child_digest)))

    overlap = bytearray(valid)
    first_child = cow.INTERNAL_ENTRY.unpack_from(
        overlap, root_offset + cow.PAGE_HEADER_LEN
    )
    second_child = list(
        cow.INTERNAL_ENTRY.unpack_from(
            overlap, root_offset + cow.PAGE_HEADER_LEN + cow.INTERNAL_ENTRY_LEN
        )
    )
    second_child[0] = first_child[1]
    mutate_root_entry(overlap, 1, tuple(second_child))
    cases.append(Case("child-overlap", "child order", bytes(overlap)))

    out_of_range = bytearray(valid)
    footer = cow.parse_footer(bytes(out_of_range), footer_offset(out_of_range))
    first_child = list(
        cow.INTERNAL_ENTRY.unpack_from(
            out_of_range, root_offset + cow.PAGE_HEADER_LEN
        )
    )
    first_child[2] = footer.snapshot_offset
    mutate_root_entry(out_of_range, 0, tuple(first_child))
    cases.append(Case("child-out-of-range", "page range", bytes(out_of_range)))

    snapshot_root = bytearray(valid)
    footer = cow.parse_footer(bytes(snapshot_root), footer_offset(snapshot_root))
    fields = list(cow.SNAPSHOT.unpack_from(snapshot_root, footer.snapshot_offset))
    fields[4] = bytes([9]) * 32
    cow.SNAPSHOT.pack_into(snapshot_root, footer.snapshot_offset, *fields)
    reauthenticate_footer(snapshot_root)
    cases.append(Case("snapshot-root-digest", "page digest", bytes(snapshot_root)))

    cases.append(Case("trailing-bytes", "footer", valid + b"x"))
    cases.append(Case("interrupted-footer", "footer", valid[: -cow.FOOTER_LEN // 2]))

    for case in cases:
        expect(case)

    print(f"valid_bytes={len(valid):,}")
    print(f"adversarial_cases={len(cases)}")
    for case in cases:
        print(f"{case.name}={case.expected}")
    print("outer_reauthentication=pass")
    print("layer_targeting=pass")
    print("finding=immutable content identity still requires full inner canonical validation")
    print("finding=current-commit reauthentication cannot authorize malformed historical pages")
    print("finding=successor invalid vectors must target page, parent, snapshot, and exact-end layers independently")


if __name__ == "__main__":
    main()
