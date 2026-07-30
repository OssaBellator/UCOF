#!/usr/bin/env python3
"""Profile-supplied dependency and snapshot-retention model for compaction."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum


class RetentionError(ValueError):
    pass


class UnknownSemantics(RetentionError):
    pass


class MissingDependency(RetentionError):
    pass


class WorkLimit(RetentionError):
    pass


class UnknownPolicy(Enum):
    ABORT = "abort"
    RETAIN_ALL_UNKNOWN = "retain-all-unknown"


@dataclass(frozen=True)
class Snapshot:
    identity: str
    sequence: int
    roots: tuple[int, ...]


@dataclass(frozen=True)
class Limits:
    max_snapshots: int = 16
    max_nodes: int = 100
    max_edges: int = 200
    max_depth: int = 32


@dataclass(frozen=True)
class Plan:
    retained_snapshots: tuple[str, ...]
    retained_objects: tuple[int, ...]
    discarded_objects: tuple[int, ...]
    unknown_objects_retained: tuple[int, ...]
    edges_visited: int
    maximum_depth: int


SNAPSHOTS = (
    Snapshot("s0", 0, (1,)),
    Snapshot("s1", 1, (4,)),
    Snapshot("s2", 2, (7,)),
)

# None means the active profile cannot interpret this object's dependencies.
DEPENDENCIES: dict[int, tuple[int, ...] | None] = {
    1: (2,),
    2: (3,),
    3: (),
    4: (2, 5),
    5: (6,),
    6: (),
    7: (8,),
    8: (3,),
    9: None,
    10: (11,),
    11: (10,),  # cycle is valid and retained once
}
ALL_OBJECTS = tuple(sorted(DEPENDENCIES))


def select_snapshots(
    snapshots: tuple[Snapshot, ...],
    retain_last: int,
    pinned: set[str],
    limits: Limits,
) -> tuple[Snapshot, ...]:
    if retain_last <= 0:
        raise RetentionError("retain_last must be positive")
    by_sequence = tuple(sorted(snapshots, key=lambda snapshot: snapshot.sequence))
    if any(left.sequence >= right.sequence for left, right in zip(by_sequence, by_sequence[1:])):
        raise RetentionError("snapshot sequences must be strictly increasing")
    known = {snapshot.identity for snapshot in by_sequence}
    if not pinned <= known:
        raise RetentionError("pinned snapshot is absent")
    selected_ids = {snapshot.identity for snapshot in by_sequence[-retain_last:]} | pinned
    selected = tuple(snapshot for snapshot in by_sequence if snapshot.identity in selected_ids)
    if len(selected) > limits.max_snapshots:
        raise WorkLimit("snapshot limit")
    return selected


def plan_retention(
    snapshots: tuple[Snapshot, ...],
    dependencies: dict[int, tuple[int, ...] | None],
    all_objects: tuple[int, ...],
    *,
    retain_last: int,
    pinned: set[str],
    unknown_policy: UnknownPolicy,
    limits: Limits = Limits(),
) -> Plan:
    selected = select_snapshots(snapshots, retain_last, pinned, limits)
    roots = sorted({root for snapshot in selected for root in snapshot.roots})
    retained: set[int] = set()
    unknown_retained: set[int] = set()
    edges_visited = 0
    maximum_depth = 0
    stack = [(root, 0) for root in reversed(roots)]

    while stack:
        object_id, depth = stack.pop()
        if depth > limits.max_depth:
            raise WorkLimit("dependency depth")
        if object_id in retained:
            continue
        if object_id not in dependencies:
            raise MissingDependency(f"object {object_id} is absent")
        if len(retained) >= limits.max_nodes:
            raise WorkLimit("node limit")
        retained.add(object_id)
        maximum_depth = max(maximum_depth, depth)
        children = dependencies[object_id]
        if children is None:
            if unknown_policy is UnknownPolicy.ABORT:
                raise UnknownSemantics(f"object {object_id} has unknown dependency semantics")
            unknown_retained.add(object_id)
            continue
        for child in reversed(tuple(sorted(children))):
            edges_visited += 1
            if edges_visited > limits.max_edges:
                raise WorkLimit("edge limit")
            stack.append((child, depth + 1))

    if unknown_policy is UnknownPolicy.RETAIN_ALL_UNKNOWN:
        for object_id, children in dependencies.items():
            if children is None:
                retained.add(object_id)
                unknown_retained.add(object_id)

    all_set = set(all_objects)
    if not retained <= all_set:
        raise MissingDependency("dependency graph references an unlisted object")
    discarded = all_set - retained
    return Plan(
        retained_snapshots=tuple(snapshot.identity for snapshot in selected),
        retained_objects=tuple(sorted(retained)),
        discarded_objects=tuple(sorted(discarded)),
        unknown_objects_retained=tuple(sorted(unknown_retained)),
        edges_visited=edges_visited,
        maximum_depth=maximum_depth,
    )


def expect_error(error_type: type[Exception], function, *args, **kwargs) -> None:
    try:
        function(*args, **kwargs)
    except error_type:
        return
    raise AssertionError(f"expected {error_type.__name__}")


def main() -> None:
    active = plan_retention(
        SNAPSHOTS,
        DEPENDENCIES,
        ALL_OBJECTS,
        retain_last=1,
        pinned=set(),
        unknown_policy=UnknownPolicy.ABORT,
    )
    assert active.retained_snapshots == ("s2",)
    assert active.retained_objects == (3, 7, 8)

    last_two = plan_retention(
        SNAPSHOTS,
        DEPENDENCIES,
        ALL_OBJECTS,
        retain_last=2,
        pinned=set(),
        unknown_policy=UnknownPolicy.ABORT,
    )
    assert last_two.retained_snapshots == ("s1", "s2")
    assert last_two.retained_objects == (2, 3, 4, 5, 6, 7, 8)

    pinned = plan_retention(
        SNAPSHOTS,
        DEPENDENCIES,
        ALL_OBJECTS,
        retain_last=1,
        pinned={"s0"},
        unknown_policy=UnknownPolicy.ABORT,
    )
    assert pinned.retained_snapshots == ("s0", "s2")
    assert pinned.retained_objects == (1, 2, 3, 7, 8)

    unknown_snapshot = SNAPSHOTS + (Snapshot("s3", 3, (9,)),)
    expect_error(
        UnknownSemantics,
        plan_retention,
        unknown_snapshot,
        DEPENDENCIES,
        ALL_OBJECTS,
        retain_last=1,
        pinned=set(),
        unknown_policy=UnknownPolicy.ABORT,
    )
    retain_unknown = plan_retention(
        unknown_snapshot,
        DEPENDENCIES,
        ALL_OBJECTS,
        retain_last=1,
        pinned=set(),
        unknown_policy=UnknownPolicy.RETAIN_ALL_UNKNOWN,
    )
    assert retain_unknown.retained_objects == (9,)
    assert retain_unknown.unknown_objects_retained == (9,)

    cycle_snapshot = SNAPSHOTS + (Snapshot("s3", 3, (10,)),)
    cycle = plan_retention(
        cycle_snapshot,
        DEPENDENCIES,
        ALL_OBJECTS,
        retain_last=1,
        pinned=set(),
        unknown_policy=UnknownPolicy.ABORT,
    )
    assert cycle.retained_objects == (10, 11)
    assert cycle.edges_visited == 2

    missing = dict(DEPENDENCIES)
    missing[8] = (999,)
    expect_error(
        MissingDependency,
        plan_retention,
        SNAPSHOTS,
        missing,
        ALL_OBJECTS,
        retain_last=1,
        pinned=set(),
        unknown_policy=UnknownPolicy.ABORT,
    )

    expect_error(
        WorkLimit,
        plan_retention,
        SNAPSHOTS,
        DEPENDENCIES,
        ALL_OBJECTS,
        retain_last=2,
        pinned=set(),
        unknown_policy=UnknownPolicy.ABORT,
        limits=Limits(max_snapshots=1),
    )
    expect_error(
        WorkLimit,
        plan_retention,
        SNAPSHOTS,
        DEPENDENCIES,
        ALL_OBJECTS,
        retain_last=2,
        pinned=set(),
        unknown_policy=UnknownPolicy.ABORT,
        limits=Limits(max_nodes=3),
    )

    print(f"active_only_objects={active.retained_objects}")
    print(f"last_two_objects={last_two.retained_objects}")
    print(f"pinned_history_objects={pinned.retained_objects}")
    print(f"cycle_objects={cycle.retained_objects}")
    print("unknown_semantics_abort=pass")
    print("retain_all_unknown_policy=pass")
    print("missing_dependency_rejection=pass")
    print("snapshot_node_edge_depth_limits=pass")
    print("finding=semantic compaction requires profile-supplied dependencies and explicit snapshot retention")
    print("finding=unknown dependency semantics must abort or use an explicit conservative retention policy")
    print("finding=history retention is identity-based and sequence-based, not wall-clock freshness")
    print("finding=repair-all remains distinct because it retains every active object without semantic pruning")


if __name__ == "__main__":
    main()
