#!/usr/bin/env python3
"""Analyze interactions among Candidate 1 implementation-local defaults."""

from __future__ import annotations

from math import ceil

KIB = 1024
MIB = 1024 * KIB
GIB = 1024 * MIB

PAGE_BYTES = 16 * KIB
PAGE_HEADER_BYTES = 64
LEAF_ENTRY_BYTES = 88
INTERNAL_ENTRY_BYTES = 64
INTERNAL_FANOUT = (PAGE_BYTES - PAGE_HEADER_BYTES) // INTERNAL_ENTRY_BYTES
OBJECT_HEADER_BYTES = 48
SNAPSHOT_HEADER_BYTES = 160

MAX_FILE_BYTES = 16 * GIB
MAX_COMMIT_BYTES = 16 * GIB
MAX_SNAPSHOT_BYTES = 16 * MIB
MAX_PAGES = 1_000_000
MAX_PAGE_DEPTH = 32
MAX_OBJECTS = 10_000_000
MAX_PAYLOAD_BYTES = 16 * GIB
MAX_HASHED_BYTES = 32 * GIB
MAX_ROOTS = 1_000_000
MAX_CAPABILITIES = 65_536

MAX_SOURCE_BYTES_READ = 32 * GIB
MAX_READ_OPERATIONS = 1_000_000
MAX_READ_REQUEST_BYTES = 64 * KIB
MAX_PAGE_READS = 32

MAX_SCAN_BYTES = 16 * MIB
MAX_SCAN_READ_OPERATIONS = 4_096
MAX_MAGIC_MATCHES = 4_096
MAX_CANDIDATE_VALIDATIONS = 1_024
MAX_RESULTS = 64
MAX_TOTAL_CANDIDATE_BYTES_READ = 64 * GIB

VALID_VECTOR_MAX_BYTES = 85_528
VALID_VECTOR_MAX_OBJECTS = 400


def directory_shape(objects: int) -> tuple[int, tuple[int, ...], int]:
    leaf_capacity = (PAGE_BYTES - PAGE_HEADER_BYTES) // LEAF_ENTRY_BYTES
    level = ceil(objects / leaf_capacity)
    levels = [level]
    pages = level
    while level > 1:
        level = ceil(level / INTERNAL_FANOUT)
        levels.append(level)
        pages += level
    return pages, tuple(levels), pages * PAGE_BYTES


def main() -> None:
    pages, levels, directory_bytes = directory_shape(MAX_OBJECTS)
    minimum_object_record_bytes = MAX_OBJECTS * OBJECT_HEADER_BYTES
    minimum_file_bytes_for_max_objects = minimum_object_record_bytes + directory_bytes
    optimistic_object_reads = MAX_OBJECTS
    object_read_ratio = optimistic_object_reads / MAX_READ_OPERATIONS

    maximum_scan_reads_needed = ceil(MAX_SCAN_BYTES / MAX_READ_REQUEST_BYTES)
    worst_case_full_file_candidates = MAX_TOTAL_CANDIDATE_BYTES_READ // MAX_FILE_BYTES

    roots_bytes = MAX_ROOTS * 8
    capability_bytes = MAX_CAPABILITIES * 8 * 2
    maximum_declared_snapshot_arrays = SNAPSHOT_HEADER_BYTES + roots_bytes + capability_bytes

    assert INTERNAL_FANOUT == 255
    assert levels == (54_055, 212, 1)
    assert pages == 54_268
    assert directory_bytes == 889_126_912
    assert minimum_file_bytes_for_max_objects == 1_369_126_912
    assert optimistic_object_reads > MAX_READ_OPERATIONS
    assert maximum_scan_reads_needed == 256
    assert maximum_scan_reads_needed < MAX_SCAN_READ_OPERATIONS
    assert worst_case_full_file_candidates == 4
    assert MAX_CANDIDATE_VALIDATIONS == 1_024
    assert maximum_declared_snapshot_arrays < MAX_SNAPSHOT_BYTES
    assert VALID_VECTOR_MAX_BYTES < MAX_FILE_BYTES
    assert VALID_VECTOR_MAX_OBJECTS < MAX_OBJECTS
    assert MAX_HASHED_BYTES == 2 * MAX_FILE_BYTES
    assert MAX_SOURCE_BYTES_READ == 2 * MAX_FILE_BYTES
    assert MAX_PAGE_DEPTH > len(levels)
    assert MAX_PAGE_READS >= MAX_PAGE_DEPTH

    print("| Relationship | Value | Consequence |")
    print("|---|---:|---|")
    print(
        f"| Max-object directory pages | {pages:,} | "
        f"{directory_bytes:,} directory bytes at 10,000,000 objects |"
    )
    print(
        f"| Minimum bytes for max objects | {minimum_file_bytes_for_max_objects:,} | "
        "Headers and directory fit under max file before payloads |"
    )
    print(
        f"| Optimistic reads for max objects | {optimistic_object_reads:,} | "
        f"{object_read_ratio:.1f}x the default read-operation budget |"
    )
    print(
        f"| Reads needed for full recovery scan | {maximum_scan_reads_needed:,} | "
        f"Below the {MAX_SCAN_READ_OPERATIONS:,} scan-read cap |"
    )
    print(
        f"| Full-size candidates affordable | {worst_case_full_file_candidates:,} | "
        f"Far below the {MAX_CANDIDATE_VALIDATIONS:,} validation-count cap |"
    )
    print(
        f"| Max root/capability arrays | {maximum_declared_snapshot_arrays:,} | "
        f"Below the {MAX_SNAPSHOT_BYTES:,}-byte snapshot cap |"
    )
    print(
        f"| Largest valid vector | {VALID_VECTOR_MAX_BYTES:,} | "
        "Corpus exercises only a tiny fraction of policy defaults |"
    )

    print()
    print(f"max_object_tree_levels={levels}")
    print(f"max_object_tree_depth={len(levels)}")
    print(f"configured_page_depth={MAX_PAGE_DEPTH}")
    print(f"configured_source_page_reads={MAX_PAGE_READS}")
    print(f"max_vector_file_fraction={VALID_VECTOR_MAX_BYTES / MAX_FILE_BYTES:.9f}")
    print(f"max_vector_object_fraction={VALID_VECTOR_MAX_OBJECTS / MAX_OBJECTS:.9f}")
    print("finding=current defaults are independent safety ceilings, not one coherent conformance support class")
    print("finding=object count and source read-operation defaults conflict by at least 10x")
    print("finding=recovery validation count is bounded more tightly by cumulative candidate bytes for full-size files")
    print("finding=normative minima must be selected as a jointly satisfiable profile and tested at its boundaries")
    print("finding=resource-limit refusal must remain distinct from malformed-file rejection")


if __name__ == "__main__":
    main()
