#!/usr/bin/env python3
"""Deterministic immutable-successor fork, recovery, and history recipes."""

from __future__ import annotations

from hashlib import sha256

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_object_source as source_model
import experiment_exp0002_immutable_page_objects as objects
import experiment_exp0002_immutable_page_recovery as recovery


def base_file() -> bytes:
    return objects.build_genesis(
        [
            objects.ObjectInput(1, 1, b"alpha"),
            objects.ObjectInput(2, 2, b"bravo"),
            objects.ObjectInput(3, 3, b"charlie"),
            objects.ObjectInput(4, 1, b"delta"),
        ]
    )


def append_child_a(base: bytes) -> bytes:
    return objects.append_replacement(
        base,
        objects.ObjectInput(1, 9, b"alpha-v2"),
    )


def append_sibling_child(base: bytes, child_a: bytes) -> bytes:
    base_report = objects.validate_complete(base)
    output = bytearray(child_a)
    replacement = objects.append_object(
        output,
        objects.ObjectInput(2, 10, b"bravo-fork"),
    )
    locators = list(base_report.objects)
    index = next(
        index
        for index, locator in enumerate(locators)
        if locator.object_id == replacement.object_id
    )
    locators[index] = replacement
    page_start = len(output)
    root = cow.build_tree(output, locators)
    page_count = (len(output) - page_start) // cow.PAGE_SIZE
    cow.publish(
        output,
        1,
        root,
        base_report.structural.snapshot_digest,
        base_report.structural.footer_offset,
        page_count,
    )
    return bytes(output)


def rewrite_final_footer_sequence(data: bytes, sequence: int) -> bytes:
    output = bytearray(data)
    footer_offset = len(output) - cow.FOOTER_LEN
    footer = cow.parse_footer(data, footer_offset)
    snapshot_values = list(cow.SNAPSHOT.unpack_from(output, footer.snapshot_offset))
    snapshot_values[1] = sequence
    cow.SNAPSHOT.pack_into(output, footer.snapshot_offset, *snapshot_values)
    cow.FOOTER.pack_into(
        output,
        footer_offset,
        cow.FOOTER_MAGIC,
        sequence,
        footer.snapshot_offset,
        footer.snapshot_len,
        footer.previous_footer_offset,
        footer.page_count_current,
        footer.snapshot_digest,
        footer.commit_digest,
        bytes(16),
    )
    objects.reauthenticate_footer(output)
    return bytes(output)


def rewrite_final_parent_digest(data: bytes, digest: bytes) -> bytes:
    output = bytearray(data)
    footer = cow.parse_footer(data, len(data) - cow.FOOTER_LEN)
    snapshot_values = list(cow.SNAPSHOT.unpack_from(output, footer.snapshot_offset))
    snapshot_values[5] = digest
    cow.SNAPSHOT.pack_into(output, footer.snapshot_offset, *snapshot_values)
    objects.reauthenticate_footer(output)
    return bytes(output)


def strict_error(data: bytes) -> str:
    try:
        source_model.strict_validate(
            source_model.CountingSource(data, recovery.RecoveryLimits().source)
        )
    except cow.FormatError as error:
        return str(error)
    raise AssertionError("malformed fork recipe validated")


def main() -> None:
    base = base_file()
    child_a = append_child_a(base)
    forked = append_sibling_child(base, child_a)
    forked_again = append_sibling_child(base, child_a)
    assert forked == forked_again

    active = objects.validate_complete(forked)
    assert active.structural.sequence == 1
    assert active.object_payloads[1] == b"alpha"
    assert active.object_payloads[2] == b"bravo-fork"

    child_a_report = objects.validate_complete(child_a)
    base_report = objects.validate_complete(base)
    assert child_a_report.structural.sequence == 1
    assert child_a_report.object_payloads[1] == b"alpha-v2"
    assert (
        active.structural.snapshot_digest
        != child_a_report.structural.snapshot_digest
    )

    report = recovery.scan_valid_prefixes(forked, recovery.RecoveryLimits())
    sequences = tuple(result.sequence for result in report.results)
    assert sequences == (1, 1, 0)
    sequence_one = [
        result for result in report.results if result.sequence == 1
    ]
    assert len(sequence_one) == 2
    assert (
        sequence_one[0].snapshot_digest
        != sequence_one[1].snapshot_digest
    )

    active_history = recovery.validate_history_at(
        forked,
        len(forked),
        recovery.RecoveryLimits(),
    )
    child_a_history = recovery.validate_history_at(
        forked,
        len(child_a),
        recovery.RecoveryLimits(),
    )
    assert active_history.sequences == (1, 0)
    assert child_a_history.sequences == (1, 0)

    interrupted = forked[: -cow.FOOTER_LEN // 2]
    interrupted_error = strict_error(interrupted)
    interrupted_recovery = recovery.scan_valid_prefixes(
        interrupted,
        recovery.RecoveryLimits(),
    )
    assert tuple(
        result.sequence for result in interrupted_recovery.results
    ) == (1, 0)
    assert interrupted_recovery.results[0].prefix_len == len(child_a)

    bad_parent = rewrite_final_parent_digest(
        forked,
        bytes([7]) + bytes(31),
    )
    parent_error = strict_error(bad_parent)
    assert parent_error == "parent linkage"

    sequence_gap = rewrite_final_footer_sequence(forked, 2)
    sequence_error = strict_error(sequence_gap)
    assert sequence_error == "parent linkage"

    print(f"base_bytes={len(base)}")
    print(f"child_a_bytes={len(child_a)}")
    print(f"forked_bytes={len(forked)}")
    print(f"base_sha256={sha256(base).hexdigest()}")
    print(f"child_a_sha256={sha256(child_a).hexdigest()}")
    print(f"forked_sha256={sha256(forked).hexdigest()}")
    print(f"recovered_sequences={sequences}")
    print(
        "sequence_one_snapshot_digests="
        + ",".join(result.snapshot_digest.hex() for result in sequence_one)
    )
    print(f"active_history={active_history.sequences}")
    print(f"sibling_history={child_a_history.sequences}")
    print(f"interrupted_strict_error={interrupted_error}")
    print(
        "interrupted_recovered_sequences="
        f"{tuple(result.sequence for result in interrupted_recovery.results)}"
    )
    print(f"bad_parent_error={parent_error}")
    print(f"sequence_gap_error={sequence_error}")
    print("deterministic_fork_bytes=pass")
    print("recovery_enumerates_both_fork_terminals=pass")
    print("recovery_does_not_select_between_equal_sequence_forks=pass")
    print("each_fork_history_revalidates_to_same_genesis=pass")


if __name__ == "__main__":
    main()
