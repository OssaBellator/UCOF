#!/usr/bin/env python3
"""Prove whether Candidate 1 page sequence binding permits historical page reuse."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass

from exp0002_codec import (
    COMMIT_DOMAIN,
    FOOTER_LEN,
    INTERNAL_ENTRY_LEN,
    PAGE_DOMAIN,
    PAGE_HEADER_LEN,
    PAGE_SIZE,
    SNAPSHOT_DOMAIN,
    FileHeader,
    Footer,
    InternalEntry,
    ObjectInput,
    PageLocator,
    _digest,
    _parse_page,
    build_append,
    build_genesis,
    validate_strict,
)

SEQUENCE_OFFSET = 32
SEQUENCE_END = 40


@dataclass(frozen=True)
class PageRecord:
    offset: int
    kind: int
    level: int
    minimum: int
    maximum: int
    sequence: int
    digest: bytes
    data: bytes


def collect_pages(data: bytes) -> list[PageRecord]:
    verified = validate_strict(data)
    stack = [
        PageLocator(
            0,
            (1 << 64) - 1,
            verified.snapshot.directory_root_offset,
            verified.snapshot.directory_root_level,
            verified.snapshot.directory_root_digest,
        )
    ]
    pages: list[PageRecord] = []
    while stack:
        locator = stack.pop()
        page = data[locator.offset : locator.offset + PAGE_SIZE]
        parsed = _parse_page(page)
        pages.append(
            PageRecord(
                locator.offset,
                parsed.kind,
                parsed.level,
                parsed.minimum,
                parsed.maximum,
                parsed.sequence,
                locator.digest,
                page,
            )
        )
        if parsed.kind == 2:
            for child in reversed(parsed.entries):
                assert isinstance(child, InternalEntry)
                stack.append(
                    PageLocator(
                        child.min_key,
                        child.max_key,
                        child.page_offset,
                        child.level,
                        child.page_digest,
                    )
                )
    return pages


def without_sequence(page: bytes) -> bytes:
    return page[:SEQUENCE_OFFSET] + bytes(8) + page[SEQUENCE_END:]


def forge_authenticated_historical_leaf_reuse(
    genesis: bytes,
    append: bytes,
    minimum: int,
    maximum: int,
) -> bytes:
    genesis_pages = collect_pages(genesis)
    source = next(
        page
        for page in genesis_pages
        if page.kind == 1 and page.minimum == minimum and page.maximum == maximum
    )

    verified = validate_strict(append)
    output = bytearray(append)
    root_offset = verified.snapshot.directory_root_offset
    root_page = bytearray(output[root_offset : root_offset + PAGE_SIZE])
    parsed_root = _parse_page(bytes(root_page))
    assert parsed_root.kind == 2

    child_index = next(
        index
        for index, child in enumerate(parsed_root.entries)
        if isinstance(child, InternalEntry)
        and child.min_key == minimum
        and child.max_key == maximum
    )
    child = parsed_root.entries[child_index]
    assert isinstance(child, InternalEntry)

    # Replace the new commit's duplicate leaf with the exact historical page.
    output[child.page_offset : child.page_offset + PAGE_SIZE] = source.data

    # Authenticate that historical page in the current root page.
    digest_start = PAGE_HEADER_LEN + child_index * INTERNAL_ENTRY_LEN + 32
    root_page[digest_start : digest_start + 32] = source.digest
    output[root_offset : root_offset + PAGE_SIZE] = root_page

    # Reauthenticate the changed root, snapshot, footer semantics, and commit.
    snapshot_offset = verified.footer.snapshot_offset
    snapshot_len = verified.footer.snapshot_len
    snapshot = bytearray(output[snapshot_offset : snapshot_offset + snapshot_len])
    snapshot[72:104] = _digest(PAGE_DOMAIN, bytes(root_page))
    output[snapshot_offset : snapshot_offset + snapshot_len] = snapshot
    snapshot_digest = _digest(SNAPSHOT_DOMAIN, bytes(snapshot))

    footer_offset = verified.footer_offset
    footer = Footer.parse(bytes(output[footer_offset:]))
    unsigned = Footer(
        footer.commit_start,
        footer.commit_len,
        footer.snapshot_offset,
        footer.snapshot_len,
        footer.sequence,
        footer.previous_footer_offset,
        footer.record_count,
        snapshot_digest,
    )
    commit_digest = hashlib.sha256(
        COMMIT_DOMAIN
        + bytes(output[footer.commit_start:footer_offset])
        + unsigned.semantics()
    ).digest()
    output[footer_offset:] = Footer(
        unsigned.commit_start,
        unsigned.commit_len,
        unsigned.snapshot_offset,
        unsigned.snapshot_len,
        unsigned.sequence,
        unsigned.previous_footer_offset,
        unsigned.record_count,
        unsigned.snapshot_digest,
        commit_digest,
    ).encode()
    return bytes(output)


def main() -> None:
    header = FileHeader(b"reuse-page-test!", b"reuse-nonce-0002")
    genesis = build_genesis(
        header,
        [
            ObjectInput(index, 1, bytes([index % 251]), index == 1)
            for index in range(1, 401)
        ],
    )
    append = build_append(
        genesis,
        [ObjectInput(401, 1, b"new", False)],
        [1, 401],
    )

    genesis_pages = collect_pages(genesis)
    append_pages = collect_pages(append)
    exact_matches = 0
    sequence_only_matches = 0
    matched_ranges: list[tuple[int, int]] = []
    for old in genesis_pages:
        for new in append_pages:
            if (old.kind, old.level, old.minimum, old.maximum) != (
                new.kind,
                new.level,
                new.minimum,
                new.maximum,
            ):
                continue
            if old.data == new.data:
                exact_matches += 1
            elif without_sequence(old.data) == without_sequence(new.data):
                sequence_only_matches += 1
                matched_ranges.append((old.minimum, old.maximum))

    assert exact_matches == 0
    assert sequence_only_matches >= 2
    assert (1, 185) in matched_ranges
    assert (186, 370) in matched_ranges

    forged = forge_authenticated_historical_leaf_reuse(genesis, append, 1, 185)
    try:
        validate_strict(forged)
    except ValueError as error:
        rejection = str(error)
    else:
        raise AssertionError("historical page reuse unexpectedly passed Candidate 1")
    assert rejection == "page reference mismatch", rejection

    print(f"genesis_pages={len(genesis_pages)}")
    print(f"append_pages={len(append_pages)}")
    print(f"exact_reusable_pages={exact_matches}")
    print(f"sequence_only_equivalent_pages={sequence_only_matches}")
    print(f"sequence_only_ranges={matched_ranges}")
    print(f"authenticated_reuse_rejection={rejection}")
    print(
        "finding=Candidate 1 snapshot-sequence binding prevents byte-for-byte "
        "historical page reuse"
    )


if __name__ == "__main__":
    main()
