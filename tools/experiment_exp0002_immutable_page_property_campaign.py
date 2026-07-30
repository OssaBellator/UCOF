#!/usr/bin/env python3
"""Multi-seed deterministic property campaign for immutable-page operations."""

from __future__ import annotations

import hashlib

import experiment_exp0002_immutable_page_sequences as sequences

SEEDS = tuple(range(32)) + (0x55434F46, 0xFFFFFFFF)
OPERATIONS_PER_SEED = 256


def report_digest(report: sequences.SequenceReport) -> str:
    hasher = hashlib.sha256()
    hasher.update(report.final_bytes)
    hasher.update(report.final_root.digest)
    for object_id in report.final_identifiers:
        hasher.update(object_id.to_bytes(8, "little"))
    for value in (
        report.insertions,
        report.deletions,
        report.maximum_root_level,
        report.maximum_new_pages,
        report.total_new_pages,
        report.total_reused_page_observations,
        report.root_height_increases,
        report.root_height_collapses,
    ):
        hasher.update(value.to_bytes(8, "little"))
    return hasher.hexdigest()


def run_seed(seed: int) -> sequences.SequenceReport:
    previous_seed = sequences.SEED
    previous_operations = sequences.OPERATIONS
    try:
        sequences.SEED = seed
        sequences.OPERATIONS = OPERATIONS_PER_SEED
        return sequences.run_sequence()
    finally:
        sequences.SEED = previous_seed
        sequences.OPERATIONS = previous_operations


def main() -> None:
    digests: list[str] = []
    total_insertions = 0
    total_deletions = 0
    total_new_pages = 0
    total_reused = 0
    maximum_new_pages = 0
    total_increases = 0
    total_collapses = 0

    for seed in SEEDS:
        first = run_seed(seed)
        second = run_seed(seed)
        assert first == second
        assert first.insertions + first.deletions == OPERATIONS_PER_SEED
        assert sequences.MIN_OBJECTS <= len(first.final_identifiers) <= sequences.MAX_OBJECTS
        assert first.maximum_root_level == 1
        assert first.maximum_new_pages <= 3
        assert first.root_height_increases >= 1
        assert first.root_height_collapses >= 1
        assert tuple(sorted(set(first.final_identifiers))) == first.final_identifiers

        digest = report_digest(first)
        digests.append(digest)
        total_insertions += first.insertions
        total_deletions += first.deletions
        total_new_pages += first.total_new_pages
        total_reused += first.total_reused_page_observations
        maximum_new_pages = max(maximum_new_pages, first.maximum_new_pages)
        total_increases += first.root_height_increases
        total_collapses += first.root_height_collapses
        print(
            f"seed={seed} final_objects={len(first.final_identifiers)} "
            f"insertions={first.insertions} deletions={first.deletions} "
            f"new_pages={first.total_new_pages} reused={first.total_reused_page_observations} "
            f"digest={digest}"
        )

    aggregate = hashlib.sha256("".join(digests).encode("ascii")).hexdigest()
    assert len(digests) == len(SEEDS)
    assert len(set(digests)) > len(SEEDS) // 2
    assert total_insertions + total_deletions == len(SEEDS) * OPERATIONS_PER_SEED
    assert maximum_new_pages <= 3

    print(f"seeds={len(SEEDS)}")
    print(f"operations_per_seed={OPERATIONS_PER_SEED}")
    print(f"total_operations={len(SEEDS) * OPERATIONS_PER_SEED}")
    print(f"total_insertions={total_insertions}")
    print(f"total_deletions={total_deletions}")
    print(f"total_new_pages={total_new_pages}")
    print(f"total_reused_page_observations={total_reused}")
    print(f"maximum_new_pages_per_operation={maximum_new_pages}")
    print(f"root_height_increases={total_increases}")
    print(f"root_height_collapses={total_collapses}")
    print(f"aggregate_sha256={aggregate}")
    print("deterministic_replay_all_seeds=pass")
    print("sorted_set_oracle_all_operations=pass")
    print("bounded_page_emission_all_operations=pass")
    print("finding=multi-seed differential campaigns catch state-machine defects beyond fixed examples")
    print("finding=deterministic replay permits pinned aggregate evidence without pinning every intermediate file")


if __name__ == "__main__":
    main()
