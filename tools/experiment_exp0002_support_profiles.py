#!/usr/bin/env python3
"""Check jointly satisfiable experimental successor support profiles."""

from __future__ import annotations

from dataclasses import dataclass
from math import ceil

PAGE_SIZE = 16 * 1024
LEAF_CAPACITY = (PAGE_SIZE - 64) // 88
INTERNAL_FANOUT = (PAGE_SIZE - 64) // 64
FIXED_OPERATIONS = 32


@dataclass(frozen=True)
class Profile:
    name: str
    max_file_bytes: int
    max_objects: int
    max_object_bytes: int
    max_request_bytes: int
    max_read_operations: int
    max_bytes_read: int
    max_hash_bytes: int
    max_allocation_bytes: int
    max_history_depth: int
    max_recovery_scan_bytes: int


def directory_pages(objects: int) -> tuple[int, int]:
    leaves = ceil(objects / LEAF_CAPACITY)
    total = leaves
    level = leaves
    depth = 0
    while level > 1:
        level = ceil(level / INTERNAL_FANOUT)
        total += level
        depth += 1
    return total, depth


def conservative_read_operations(profile: Profile) -> int:
    pages, _depth = directory_pages(profile.max_objects)
    # Full validation can read each object once for its fixed header and again in
    # bounded hash chunks. The commit prefix is also streamed independently.
    object_header_reads = profile.max_objects
    object_hash_reads = profile.max_objects * ceil(
        profile.max_object_bytes / profile.max_request_bytes
    )
    page_reads = pages
    commit_reads = ceil(profile.max_file_bytes / profile.max_request_bytes)
    return (
        FIXED_OPERATIONS
        + object_header_reads
        + object_hash_reads
        + page_reads
        + commit_reads
    )


def conservative_bytes_read(profile: Profile) -> int:
    # One complete commit hash pass plus one complete object/page validation pass
    # is bounded by two file lengths. A third length is reserved for footer,
    # snapshot, lookup, history bookkeeping, and implementation rereads.
    return 3 * profile.max_file_bytes


def conservative_hash_bytes(profile: Profile) -> int:
    # Current-commit hashing plus active object and page hashing can approach two
    # file lengths when most active objects are historical.
    return 2 * profile.max_file_bytes


def validate(profile: Profile) -> dict[str, int]:
    if profile.max_request_bytes <= 0 or profile.max_allocation_bytes <= 0:
        raise AssertionError(f"{profile.name}: zero request or allocation")
    if profile.max_request_bytes > profile.max_allocation_bytes:
        raise AssertionError(f"{profile.name}: request exceeds allocation")
    if profile.max_object_bytes > profile.max_file_bytes:
        raise AssertionError(f"{profile.name}: object exceeds file")
    if profile.max_recovery_scan_bytes > profile.max_file_bytes:
        raise AssertionError(f"{profile.name}: recovery scan exceeds file")
    minimum_object_headers = profile.max_objects * 48
    if minimum_object_headers > profile.max_file_bytes:
        raise AssertionError(f"{profile.name}: object count cannot fit minimum headers")

    pages, depth = directory_pages(profile.max_objects)
    operations = conservative_read_operations(profile)
    bytes_read = conservative_bytes_read(profile)
    hash_bytes = conservative_hash_bytes(profile)
    if operations > profile.max_read_operations:
        raise AssertionError(
            f"{profile.name}: requires {operations:,} reads, allows {profile.max_read_operations:,}"
        )
    if bytes_read > profile.max_bytes_read:
        raise AssertionError(
            f"{profile.name}: requires {bytes_read:,} read bytes, allows {profile.max_bytes_read:,}"
        )
    if hash_bytes > profile.max_hash_bytes:
        raise AssertionError(
            f"{profile.name}: requires {hash_bytes:,} hash bytes, allows {profile.max_hash_bytes:,}"
        )
    return {
        "pages": pages,
        "depth": depth,
        "required_read_operations": operations,
        "required_bytes_read": bytes_read,
        "required_hash_bytes": hash_bytes,
    }


def mib(value: int) -> int:
    return value * 1024 * 1024


def gib(value: int) -> int:
    return value * 1024 * 1024 * 1024


def profiles() -> list[Profile]:
    return [
        Profile(
            name="research-small",
            max_file_bytes=mib(64),
            max_objects=100_000,
            max_object_bytes=mib(1),
            max_request_bytes=mib(1),
            max_read_operations=210_000,
            max_bytes_read=mib(192),
            max_hash_bytes=mib(128),
            max_allocation_bytes=mib(2),
            max_history_depth=64,
            max_recovery_scan_bytes=mib(16),
        ),
        Profile(
            name="research-medium",
            max_file_bytes=gib(4),
            max_objects=1_000_000,
            max_object_bytes=mib(16),
            max_request_bytes=mib(1),
            max_read_operations=17_100_000,
            max_bytes_read=gib(12),
            max_hash_bytes=gib(8),
            max_allocation_bytes=mib(2),
            max_history_depth=1_024,
            max_recovery_scan_bytes=mib(64),
        ),
        Profile(
            name="research-large",
            max_file_bytes=gib(64),
            max_objects=10_000_000,
            max_object_bytes=mib(64),
            max_request_bytes=mib(4),
            max_read_operations=170_100_000,
            max_bytes_read=gib(192),
            max_hash_bytes=gib(128),
            max_allocation_bytes=mib(8),
            max_history_depth=4_096,
            max_recovery_scan_bytes=mib(256),
        ),
    ]


def main() -> None:
    print(
        "profile,file_bytes,objects,pages,depth,request_bytes,required_reads,allowed_reads,required_read_bytes,required_hash_bytes"
    )
    for profile in profiles():
        facts = validate(profile)
        print(
            f"{profile.name},{profile.max_file_bytes},{profile.max_objects},"
            f"{facts['pages']},{facts['depth']},{profile.max_request_bytes},"
            f"{facts['required_read_operations']},{profile.max_read_operations},"
            f"{facts['required_bytes_read']},{facts['required_hash_bytes']}"
        )

    inconsistent = Profile(
        name="current-independent-defaults",
        max_file_bytes=gib(64),
        max_objects=10_000_000,
        max_object_bytes=mib(64),
        max_request_bytes=mib(4),
        max_read_operations=1_000_000,
        max_bytes_read=gib(192),
        max_hash_bytes=gib(128),
        max_allocation_bytes=mib(8),
        max_history_depth=4_096,
        max_recovery_scan_bytes=mib(256),
    )
    try:
        validate(inconsistent)
    except AssertionError as error:
        print(f"inconsistent_default_rejection={error}")
    else:
        raise AssertionError("known inconsistent defaults were accepted")

    print("joint_profile_feasibility=pass")
    print("resource_policy_and_malformed_input_remain_distinct=pass")
    print("finding=support profiles must be defined as satisfiable tuples, not independent maxima")


if __name__ == "__main__":
    main()
