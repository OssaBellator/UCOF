#!/usr/bin/env python3
"""Independent stdlib validation of Phase 3 algorithm fixtures.

This tool intentionally imports no Rust implementation and defines no UCOF
wire bytes. It checks algorithmic invariants recorded in JSON fixtures.
"""

from __future__ import annotations

import argparse
import json
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path
from typing import Any


class ModelError(Exception):
    """Categorized model failure."""

    def __init__(self, category: str) -> None:
        super().__init__(category)
        self.category = category


def div_ceil(value: int, divisor: int) -> int:
    if divisor <= 0:
        raise ModelError("invalid_capacity")
    return (value + divisor - 1) // divisor


def directory_shape(entries: int, leaf_capacity: int, fanout: int) -> tuple[int, int]:
    if entries < 0 or leaf_capacity <= 0 or fanout < 2:
        raise ModelError("invalid_directory_parameters")
    level_pages = max(1, div_ceil(entries, leaf_capacity))
    pages = level_pages
    depth = 1
    while level_pages > 1:
        level_pages = div_ceil(level_pages, fanout)
        pages += level_pages
        depth += 1
    return pages, depth


@dataclass(frozen=True)
class Candidate:
    identity: str
    sequence: int
    parent: str | None
    exact_end: bool
    status: str
    checkpoint: str

    @classmethod
    def from_json(cls, value: dict[str, Any]) -> "Candidate":
        return cls(
            identity=str(value["id"]),
            sequence=int(value["sequence"]),
            parent=None if value["parent"] is None else str(value["parent"]),
            exact_end=bool(value["exact_end"]),
            status=str(value["status"]),
            checkpoint=str(value["checkpoint"]),
        )


def validate_chain(
    start: Candidate,
    by_identity: dict[str, Candidate],
    *,
    max_depth: int = 256,
) -> tuple[str, ...]:
    if start.status != "verified":
        raise ModelError("not_verified")
    if start.checkpoint != "complete":
        raise ModelError("progress_checkpoint")

    current = start
    seen: set[str] = set()
    reverse: list[str] = []
    while True:
        if len(reverse) >= max_depth:
            raise ModelError("parent_depth_exceeded")
        if current.identity in seen:
            raise ModelError("parent_cycle")
        seen.add(current.identity)
        reverse.append(current.identity)
        if current.parent is None:
            return tuple(reversed(reverse))
        try:
            parent = by_identity[current.parent]
        except KeyError as error:
            raise ModelError("missing_parent") from error
        if parent.status != "verified" or parent.checkpoint != "complete":
            raise ModelError("parent_not_verified")
        if current.sequence <= parent.sequence:
            raise ModelError("non_increasing_sequence")
        if current.sequence != parent.sequence + 1:
            raise ModelError("sequence_gap")
        current = parent


def select_root(candidates: Iterable[Candidate], mode: str) -> str:
    values = list(candidates)
    by_identity: dict[str, Candidate] = {}
    for candidate in values:
        if candidate.identity in by_identity:
            raise ModelError("duplicate_identity")
        by_identity[candidate.identity] = candidate

    valid: dict[str, tuple[str, ...]] = {}
    for candidate in values:
        try:
            valid[candidate.identity] = validate_chain(candidate, by_identity)
        except ModelError:
            pass

    if mode == "strict":
        exact = [
            candidate
            for candidate in values
            if candidate.exact_end and candidate.identity in valid
        ]
        if not exact:
            raise ModelError("no_exact_end_root")
        if len(exact) != 1:
            raise ModelError("multiple_exact_end_roots")
        return exact[0].identity

    if mode != "recovery":
        raise ModelError("unknown_mode")
    if not valid:
        raise ModelError("no_valid_root")

    parent_ids = {
        by_identity[identity].parent
        for identity in valid
        if by_identity[identity].parent in valid
    }
    terminals = [
        by_identity[identity]
        for identity in valid
        if identity not in parent_ids
    ]
    maximum = max(candidate.sequence for candidate in terminals)
    highest = [candidate for candidate in terminals if candidate.sequence == maximum]
    if len(highest) != 1:
        raise ModelError("ambiguous_fork")
    return highest[0].identity


def compaction_plan(
    graph: dict[int, tuple[int, ...]],
    roots: Iterable[int],
    *,
    max_nodes: int = 1_000_000,
    max_edges: int = 4_000_000,
    max_depth: int = 1024,
) -> tuple[list[int], list[int]]:
    stack = [(root, 0) for root in reversed(tuple(roots))]
    reachable: set[int] = set()
    edges = 0
    while stack:
        object_id, depth = stack.pop()
        if depth > max_depth:
            raise ModelError("depth_limit_exceeded")
        if object_id in reachable:
            continue
        if object_id not in graph:
            raise ModelError("missing_object")
        if len(reachable) >= max_nodes:
            raise ModelError("node_limit_exceeded")
        reachable.add(object_id)
        dependencies = graph[object_id]
        edges += len(dependencies)
        if edges > max_edges:
            raise ModelError("edge_limit_exceeded")
        for dependency in reversed(dependencies):
            if dependency not in graph:
                raise ModelError("missing_object")
            if dependency not in reachable:
                stack.append((dependency, depth + 1))
    all_objects = set(graph)
    return sorted(reachable), sorted(all_objects - reachable)


def validate_fixture(path: Path) -> None:
    fixture = json.loads(path.read_text(encoding="utf-8"))

    for case in fixture["directory_shapes"]:
        actual = directory_shape(
            int(case["entries"]),
            int(case["leaf_capacity"]),
            int(case["fanout"]),
        )
        expected = (int(case["expected_pages"]), int(case["expected_depth"]))
        if actual != expected:
            raise AssertionError(f"directory shape mismatch: {case}: {actual}")

    for case in fixture["root_cases"]:
        candidates = [Candidate.from_json(value) for value in case["candidates"]]
        try:
            selected = select_root(candidates, str(case["mode"]))
        except ModelError as error:
            expected = case.get("expected_error")
            if error.category != expected:
                raise AssertionError(
                    f"root error mismatch for {case['name']}: "
                    f"expected {expected}, got {error.category}"
                ) from error
        else:
            expected = case.get("expected_selected")
            if selected != expected:
                raise AssertionError(
                    f"root selection mismatch for {case['name']}: "
                    f"expected {expected}, got {selected}"
                )

    for case in fixture["compaction_cases"]:
        graph = {
            int(object_id): tuple(int(value) for value in dependencies)
            for object_id, dependencies in case["graph"].items()
        }
        reachable, orphaned = compaction_plan(
            graph,
            (int(root) for root in case["roots"]),
        )
        if reachable != case["expected_reachable"]:
            raise AssertionError(
                f"reachable mismatch for {case['name']}: {reachable}"
            )
        if orphaned != case["expected_orphaned"]:
            raise AssertionError(
                f"orphan mismatch for {case['name']}: {orphaned}"
            )

    print(
        "validated Phase 3 model fixture: "
        f"{len(fixture['directory_shapes'])} directory shapes, "
        f"{len(fixture['root_cases'])} root cases, "
        f"{len(fixture['compaction_cases'])} compaction cases"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--fixture",
        type=Path,
        default=Path("tests/vectors/phase3-models/cases.json"),
    )
    args = parser.parse_args()
    validate_fixture(args.fixture)


if __name__ == "__main__":
    main()
