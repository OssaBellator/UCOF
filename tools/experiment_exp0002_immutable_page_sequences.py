#!/usr/bin/env python3
"""Deterministic differential operation sequences over immutable pages."""

from __future__ import annotations

import random
from dataclasses import dataclass

import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_splits as tree

SEED = 0x55434F46
OPERATIONS = 512
IDENTIFIER_SPACE = 2_000
MIN_OBJECTS = 100
MAX_OBJECTS = 300


@dataclass(frozen=True)
class SequenceReport:
    final_bytes: bytes
    final_root: cow.PageRef
    final_identifiers: tuple[int, ...]
    insertions: int
    deletions: int
    maximum_root_level: int
    maximum_new_pages: int
    total_new_pages: int
    total_reused_page_observations: int
    root_height_increases: int
    root_height_collapses: int


def choose_absent(rng: random.Random, model: set[int]) -> int:
    for _ in range(IDENTIFIER_SPACE * 2):
        candidate = rng.randrange(1, IDENTIFIER_SPACE + 1)
        if candidate not in model:
            return candidate
    raise AssertionError("identifier space exhausted")


def run_sequence() -> SequenceReport:
    initial = [cow.locator(object_id) for object_id in range(2, 322, 2)]
    data = bytearray()
    root = cow.build_tree(data, initial)
    model = {entry.object_id for entry in initial}
    rng = random.Random(SEED)

    insertions = 0
    deletions = 0
    maximum_root_level = root.level
    maximum_new_pages = 0
    total_new_pages = 0
    total_reused = 0
    increases = 0
    collapses = 0

    report = tree.validate_tree(bytes(data), root)
    assert report.identifiers == tuple(sorted(model))

    for step in range(OPERATIONS):
        previous_report = report
        previous_level = root.level
        should_insert = len(model) <= MIN_OBJECTS or (
            len(model) < MAX_OBJECTS and rng.randrange(100) < 57
        )
        if should_insert:
            object_id = choose_absent(rng, model)
            data_bytes, root = tree.insert(bytes(data), root, cow.locator(object_id, step + 1))
            model.add(object_id)
            insertions += 1
        else:
            ordered = sorted(model)
            object_id = ordered[rng.randrange(len(ordered))]
            data_bytes, root = tree.delete_from_height_one(bytes(data), root, object_id)
            model.remove(object_id)
            deletions += 1

        data = bytearray(data_bytes)
        report = tree.validate_tree(bytes(data), root)
        expected = tuple(sorted(model))
        assert report.identifiers == expected
        assert root.minimum == expected[0]
        assert root.maximum == expected[-1]
        assert root.level <= 1  # constrained fixture remains below internal split depth

        new_pages = report.reachable - previous_report.reachable
        reused_pages = report.reachable & previous_report.reachable
        assert len(new_pages) <= 3
        maximum_new_pages = max(maximum_new_pages, len(new_pages))
        total_new_pages += len(new_pages)
        total_reused += len(reused_pages)
        maximum_root_level = max(maximum_root_level, root.level)
        if previous_level == 0 and root.level == 1:
            increases += 1
        if previous_level == 1 and root.level == 0:
            collapses += 1

    return SequenceReport(
        final_bytes=bytes(data),
        final_root=root,
        final_identifiers=tuple(sorted(model)),
        insertions=insertions,
        deletions=deletions,
        maximum_root_level=maximum_root_level,
        maximum_new_pages=maximum_new_pages,
        total_new_pages=total_new_pages,
        total_reused_page_observations=total_reused,
        root_height_increases=increases,
        root_height_collapses=collapses,
    )


def main() -> None:
    first = run_sequence()
    second = run_sequence()
    assert first == second
    assert first.insertions + first.deletions == OPERATIONS
    assert MIN_OBJECTS <= len(first.final_identifiers) <= MAX_OBJECTS
    assert first.maximum_root_level == 1
    assert first.maximum_new_pages <= 3
    assert first.root_height_increases >= 1
    assert first.root_height_collapses >= 1

    print(f"seed={SEED}")
    print(f"operations={OPERATIONS}")
    print(f"insertions={first.insertions}")
    print(f"deletions={first.deletions}")
    print(f"final_objects={len(first.final_identifiers)}")
    print(f"final_root_level={first.final_root.level}")
    print(f"maximum_root_level={first.maximum_root_level}")
    print(f"maximum_new_pages_per_operation={first.maximum_new_pages}")
    print(f"total_new_pages={first.total_new_pages}")
    print(f"total_reused_page_observations={first.total_reused_page_observations}")
    print(f"root_height_increases={first.root_height_increases}")
    print(f"root_height_collapses={first.root_height_collapses}")
    print("deterministic_replay=pass")
    print("differential_sorted_set_agreement=pass")
    print("bounded_page_emission_per_operation=pass")
    print("finding=gap insertions require a canonical child-routing rule")
    print("finding=immutable append-only pages can match a sorted-set oracle across mixed operations")
    print("finding=operation-sequence fuzzing is required before successor byte selection")


if __name__ == "__main__":
    main()
