#!/usr/bin/env python3
"""Compare simple history-independent partition models for EXP-0003 leaves.

This experiment is deliberately a model, not a proposal to replace UCOF's current
persistent B+tree policy. It compares four leaf-grouping regimes:

1. packed rank: canonical and capacity-bounded, but rank shifts cascade;
2. Bernoulli hash anchors: canonical and expected-local, but unbounded tails;
3. window minimizers: canonical, local, and maximum-gap bounded, but can create
   tiny groups and has roughly half-capacity mean spacing for w == capacity under
   the classic random-order minimizer model;
4. current-style persistent split/repair for a round-trip history-sensitivity
   control.

The hash-based schemes use SHA-256 only as a deterministic pseudorandom ordering
for the experiment. This is not a claim that public hash priorities are safe from
chosen-identifier grinding; the script includes a grinding control explicitly.
"""

from __future__ import annotations

import argparse
import bisect
import hashlib
import json
import math
import random
import statistics
from collections import deque
from dataclasses import asdict, dataclass

PAGE_SIZE = 16_384
PAGE_HEADER_LEN = 80
LEAF_ENTRY_LEN = 64
CAPACITY = (PAGE_SIZE - PAGE_HEADER_LEN) // LEAF_ENTRY_LEN
MINIMUM = math.ceil(CAPACITY / 2)
HASH_SPACE = 1 << 256
ANCHOR_THRESHOLD = HASH_SPACE // CAPACITY
WINDOW = CAPACITY

FULL_SEEDS = (3, 17, 29, 43, 71)
QUICK_SEEDS = (3, 17, 29)
FULL_OBJECTS = 25_000
QUICK_OBJECTS = 5_000
FULL_EDITS_PER_KIND = 100
QUICK_EDITS_PER_KIND = 12
GRIND_CANDIDATES = 256


@dataclass(frozen=True)
class ShapeResult:
    scheme: str
    seed: int
    objects: int
    groups: int
    mean_group_size: float
    median_group_size: float
    min_group_size: int
    p95_group_size: int
    max_group_size: int
    fraction_below_half_full: float
    fraction_over_capacity: float


@dataclass(frozen=True)
class EditResult:
    scheme: str
    seed: int
    edit_kind: str
    trial: int
    old_groups: int
    new_groups: int
    reused_groups: int
    reused_old_group_fraction: float
    reused_old_key_fraction: float
    new_or_changed_groups: int
    inserted_selected_boundary: bool | None


@dataclass(frozen=True)
class Aggregate:
    scheme: str
    mean_group_size: float
    mean_max_group_size: float
    mean_fraction_below_half_full: float
    mean_fraction_over_capacity: float
    normal_insert_changed_groups: float
    normal_insert_reused_key_fraction: float
    delete_changed_groups: float
    delete_reused_key_fraction: float
    grind_insert_changed_groups: float
    grind_insert_reused_key_fraction: float
    normal_insert_boundary_rate: float | None
    grind_insert_boundary_rate: float | None


@dataclass(frozen=True)
class RoundTripResult:
    seed: int
    base_groups: int
    final_groups: int
    reused_groups: int
    reused_group_fraction: float
    reused_key_fraction: float
    final_logical_set_equal: bool
    final_partition_equal: bool


def score(identifier: int) -> int:
    raw = identifier.to_bytes(16, "big", signed=False)
    return int.from_bytes(hashlib.sha256(raw).digest(), "big", signed=False)


def generate_ids(seed: int, objects: int) -> list[int]:
    rng = random.Random(seed)
    values: set[int] = set()
    while len(values) < objects:
        values.add(rng.getrandbits(128))
    return sorted(values)


def packed_rank(ids: list[int]) -> list[tuple[int, ...]]:
    return [tuple(ids[start : start + CAPACITY]) for start in range(0, len(ids), CAPACITY)]


def hash_anchor(ids: list[int], scores: dict[int, int]) -> list[tuple[int, ...]]:
    boundaries = [0]
    for index, identifier in enumerate(ids):
        if index and scores[identifier] < ANCHOR_THRESHOLD:
            boundaries.append(index)
    boundaries.append(len(ids))
    return [
        tuple(ids[left:right])
        for left, right in zip(boundaries, boundaries[1:])
        if left < right
    ]


def minimizer(ids: list[int], scores: dict[int, int]) -> list[tuple[int, ...]]:
    if len(ids) <= WINDOW:
        return [tuple(ids)]

    minima: deque[tuple[int, int]] = deque()
    selected: set[int] = set()

    for index, identifier in enumerate(ids):
        current = scores[identifier]
        # Strict > preserves the leftmost item on equal scores.
        while minima and minima[-1][1] > current:
            minima.pop()
        minima.append((index, current))
        while minima and minima[0][0] <= index - WINDOW:
            minima.popleft()
        if index >= WINDOW - 1:
            selected.add(minima[0][0])

    boundaries = [0] + sorted(index for index in selected if index) + [len(ids)]
    return [
        tuple(ids[left:right])
        for left, right in zip(boundaries, boundaries[1:])
        if left < right
    ]


def partition(scheme: str, ids: list[int], scores: dict[int, int]) -> list[tuple[int, ...]]:
    if scheme == "packed-rank":
        return packed_rank(ids)
    if scheme == "hash-anchor":
        return hash_anchor(ids, scores)
    if scheme == "window-minimizer":
        return minimizer(ids, scores)
    raise ValueError(f"unknown scheme: {scheme}")


def selected_boundaries(scheme: str, groups: list[tuple[int, ...]]) -> set[int]:
    if scheme == "packed-rank":
        return {group[0] for group in groups[1:]}
    return {group[0] for group in groups[1:]}


def shape_result(scheme: str, seed: int, ids: list[int], groups: list[tuple[int, ...]]) -> ShapeResult:
    lengths = [len(group) for group in groups]
    ordered = sorted(lengths)
    p95 = ordered[math.floor(0.95 * (len(ordered) - 1))]
    return ShapeResult(
        scheme=scheme,
        seed=seed,
        objects=len(ids),
        groups=len(groups),
        mean_group_size=statistics.fmean(lengths),
        median_group_size=statistics.median(lengths),
        min_group_size=min(lengths),
        p95_group_size=p95,
        max_group_size=max(lengths),
        fraction_below_half_full=sum(length < MINIMUM for length in lengths) / len(lengths),
        fraction_over_capacity=sum(length > CAPACITY for length in lengths) / len(lengths),
    )


def edit_result(
    *,
    scheme: str,
    seed: int,
    edit_kind: str,
    trial: int,
    old_ids: list[int],
    old_groups: list[tuple[int, ...]],
    new_ids: list[int],
    new_groups: list[tuple[int, ...]],
    inserted_id: int | None,
) -> EditResult:
    old_set = set(old_groups)
    new_set = set(new_groups)
    reused = old_set & new_set
    reused_keys = sum(len(group) for group in reused)
    boundary_selected = None
    if inserted_id is not None:
        boundary_selected = inserted_id in selected_boundaries(scheme, new_groups)

    return EditResult(
        scheme=scheme,
        seed=seed,
        edit_kind=edit_kind,
        trial=trial,
        old_groups=len(old_groups),
        new_groups=len(new_groups),
        reused_groups=len(reused),
        reused_old_group_fraction=len(reused) / len(old_groups),
        reused_old_key_fraction=reused_keys / len(old_ids),
        new_or_changed_groups=len(new_groups) - len(reused),
        inserted_selected_boundary=boundary_selected,
    )


def midpoint_candidate(ids: list[int], index: int) -> int:
    left = ids[index - 1] if index else 0
    right = ids[index] if index < len(ids) else (1 << 128) - 1
    if right - left <= 1:
        raise ValueError("chosen identifier gap has no insertion point")
    return (left + right) // 2


def ground_candidate(ids: list[int], index: int, candidates: int) -> int:
    left = ids[index - 1] if index else 0
    right = ids[index] if index < len(ids) else (1 << 128) - 1
    width = right - left - 1
    if width < candidates:
        raise ValueError("chosen identifier gap is too small for grinding control")

    # Deterministically spread candidates across the open interval. This avoids
    # introducing a second RNG and makes the adversarial control reproducible.
    best_identifier: int | None = None
    best_score: int | None = None
    for ordinal in range(1, candidates + 1):
        identifier = left + (width * ordinal) // (candidates + 1)
        if identifier <= left or identifier >= right:
            continue
        current = score(identifier)
        if best_score is None or current < best_score:
            best_identifier = identifier
            best_score = current
    if best_identifier is None:
        raise AssertionError("grinding control failed to produce a candidate")
    return best_identifier


def persistent_insert(groups: list[tuple[int, ...]], identifier: int) -> list[tuple[int, ...]]:
    mutable = [list(group) for group in groups]
    target = next(
        (index for index, group in enumerate(mutable) if identifier <= group[-1]),
        len(mutable) - 1,
    )
    bisect.insort(mutable[target], identifier)
    if len(mutable[target]) > CAPACITY:
        overflow = mutable[target]
        left_size = math.ceil(len(overflow) / 2)
        mutable[target : target + 1] = [overflow[:left_size], overflow[left_size:]]
    return [tuple(group) for group in mutable]


def persistent_delete(groups: list[tuple[int, ...]], identifier: int) -> list[tuple[int, ...]]:
    mutable = [list(group) for group in groups]
    target = next(index for index, group in enumerate(mutable) if identifier in group)
    mutable[target].remove(identifier)

    if len(mutable[target]) >= MINIMUM or len(mutable) == 1:
        return [tuple(group) for group in mutable]

    if target > 0 and len(mutable[target - 1]) > MINIMUM:
        borrowed = mutable[target - 1].pop()
        mutable[target].insert(0, borrowed)
    elif target + 1 < len(mutable) and len(mutable[target + 1]) > MINIMUM:
        borrowed = mutable[target + 1].pop(0)
        mutable[target].append(borrowed)
    elif target > 0:
        mutable[target - 1].extend(mutable[target])
        assert len(mutable[target - 1]) <= CAPACITY
        mutable.pop(target)
    elif target + 1 < len(mutable):
        mutable[target].extend(mutable[target + 1])
        assert len(mutable[target]) <= CAPACITY
        mutable.pop(target + 1)

    return [tuple(group) for group in mutable]


def round_trip(seed: int, ids: list[int]) -> RoundTripResult:
    base = packed_rank(ids)
    # Pick the middle of a full leaf so the persistent split history is exercised
    # rather than the short final leaf.
    full_groups = [group for group in base if len(group) == CAPACITY]
    target_group = full_groups[len(full_groups) // 2]
    right = target_group[len(target_group) // 2]
    insert_index = bisect.bisect_left(ids, right)
    identifier = midpoint_candidate(ids, insert_index)

    after_insert = persistent_insert(base, identifier)
    after_delete = persistent_delete(after_insert, identifier)

    base_set = set(base)
    final_set = set(after_delete)
    reused = base_set & final_set
    final_ids = sorted(identifier for group in after_delete for identifier in group)

    return RoundTripResult(
        seed=seed,
        base_groups=len(base),
        final_groups=len(after_delete),
        reused_groups=len(reused),
        reused_group_fraction=len(reused) / len(base),
        reused_key_fraction=sum(len(group) for group in reused) / len(ids),
        final_logical_set_equal=final_ids == ids,
        final_partition_equal=after_delete == base,
    )


def run_seed(seed: int, objects: int, edits_per_kind: int) -> tuple[list[ShapeResult], list[EditResult], RoundTripResult]:
    ids = generate_ids(seed, objects)
    scores = {identifier: score(identifier) for identifier in ids}
    schemes = ("packed-rank", "hash-anchor", "window-minimizer")
    base_groups = {scheme: partition(scheme, ids, scores) for scheme in schemes}
    shapes = [shape_result(scheme, seed, ids, base_groups[scheme]) for scheme in schemes]

    rng = random.Random(seed ^ 0x55434F46)
    edits: list[EditResult] = []

    for trial in range(edits_per_kind):
        index = rng.randrange(1, len(ids))
        inserted = midpoint_candidate(ids, index)
        new_ids = ids[:index] + [inserted] + ids[index:]
        new_scores = dict(scores)
        new_scores[inserted] = score(inserted)
        for scheme in schemes:
            new_groups = partition(scheme, new_ids, new_scores)
            edits.append(
                edit_result(
                    scheme=scheme,
                    seed=seed,
                    edit_kind="insert",
                    trial=trial,
                    old_ids=ids,
                    old_groups=base_groups[scheme],
                    new_ids=new_ids,
                    new_groups=new_groups,
                    inserted_id=inserted,
                )
            )

        delete_index = rng.randrange(len(ids))
        deleted_ids = ids[:delete_index] + ids[delete_index + 1 :]
        deleted_scores = {identifier: scores[identifier] for identifier in deleted_ids}
        for scheme in schemes:
            new_groups = partition(scheme, deleted_ids, deleted_scores)
            edits.append(
                edit_result(
                    scheme=scheme,
                    seed=seed,
                    edit_kind="delete",
                    trial=trial,
                    old_ids=ids,
                    old_groups=base_groups[scheme],
                    new_ids=deleted_ids,
                    new_groups=new_groups,
                    inserted_id=None,
                )
            )

        grind_index = rng.randrange(1, len(ids))
        ground = ground_candidate(ids, grind_index, GRIND_CANDIDATES)
        ground_ids = ids[:grind_index] + [ground] + ids[grind_index:]
        ground_scores = dict(scores)
        ground_scores[ground] = score(ground)
        for scheme in schemes:
            new_groups = partition(scheme, ground_ids, ground_scores)
            edits.append(
                edit_result(
                    scheme=scheme,
                    seed=seed,
                    edit_kind="grind-insert",
                    trial=trial,
                    old_ids=ids,
                    old_groups=base_groups[scheme],
                    new_ids=ground_ids,
                    new_groups=new_groups,
                    inserted_id=ground,
                )
            )

    return shapes, edits, round_trip(seed, ids)


def mean_field(items: list[object], field: str) -> float:
    return statistics.fmean(float(getattr(item, field)) for item in items)


def optional_bool_rate(items: list[EditResult]) -> float | None:
    selected = [item.inserted_selected_boundary for item in items if item.inserted_selected_boundary is not None]
    if not selected:
        return None
    return sum(bool(value) for value in selected) / len(selected)


def aggregate(shapes: list[ShapeResult], edits: list[EditResult]) -> list[Aggregate]:
    result: list[Aggregate] = []
    for scheme in ("packed-rank", "hash-anchor", "window-minimizer"):
        scheme_shapes = [item for item in shapes if item.scheme == scheme]
        insert = [item for item in edits if item.scheme == scheme and item.edit_kind == "insert"]
        delete = [item for item in edits if item.scheme == scheme and item.edit_kind == "delete"]
        grind = [item for item in edits if item.scheme == scheme and item.edit_kind == "grind-insert"]
        result.append(
            Aggregate(
                scheme=scheme,
                mean_group_size=mean_field(scheme_shapes, "mean_group_size"),
                mean_max_group_size=mean_field(scheme_shapes, "max_group_size"),
                mean_fraction_below_half_full=mean_field(scheme_shapes, "fraction_below_half_full"),
                mean_fraction_over_capacity=mean_field(scheme_shapes, "fraction_over_capacity"),
                normal_insert_changed_groups=mean_field(insert, "new_or_changed_groups"),
                normal_insert_reused_key_fraction=mean_field(insert, "reused_old_key_fraction"),
                delete_changed_groups=mean_field(delete, "new_or_changed_groups"),
                delete_reused_key_fraction=mean_field(delete, "reused_old_key_fraction"),
                grind_insert_changed_groups=mean_field(grind, "new_or_changed_groups"),
                grind_insert_reused_key_fraction=mean_field(grind, "reused_old_key_fraction"),
                normal_insert_boundary_rate=optional_bool_rate(insert),
                grind_insert_boundary_rate=optional_bool_rate(grind),
            )
        )
    return result


def self_check(aggregates: list[Aggregate], round_trips: list[RoundTripResult]) -> None:
    assert CAPACITY == 254
    assert MINIMUM == 127
    by_name = {item.scheme: item for item in aggregates}

    packed = by_name["packed-rank"]
    anchor = by_name["hash-anchor"]
    minim = by_name["window-minimizer"]

    # Packed rank is dense but a random rank edit cascades through many later groups.
    assert packed.mean_group_size > 0.9 * CAPACITY
    assert packed.normal_insert_changed_groups > anchor.normal_insert_changed_groups * 5
    assert packed.normal_insert_changed_groups > minim.normal_insert_changed_groups * 5

    # A Bernoulli anchor process has no hard maximum group size. Fixed seeds should
    # exhibit that tail even in the quick ensemble.
    assert anchor.mean_fraction_over_capacity > 0.1

    # Winnowing/minimizers guarantee at least one selected position per WINDOW, so
    # no group may exceed WINDOW in this construction.
    assert minim.mean_fraction_over_capacity == 0.0
    assert minim.mean_max_group_size <= CAPACITY

    # Chosen-ID grinding must materially raise the chance that a new hash-anchor
    # boundary is selected compared with an ordinary midpoint insertion.
    assert anchor.grind_insert_boundary_rate is not None
    assert anchor.normal_insert_boundary_rate is not None
    assert anchor.grind_insert_boundary_rate > anchor.normal_insert_boundary_rate + 0.2

    # Current-style persistent split/repair is deliberately history-sensitive:
    # inserting into a full leaf and then deleting the same key restores the logical
    # set but generally preserves the split leaf history.
    assert all(item.final_logical_set_equal for item in round_trips)
    assert all(not item.final_partition_equal for item in round_trips)
    assert all(item.reused_key_fraction > 0.9 for item in round_trips)


def print_csv(aggregates: list[Aggregate], round_trips: list[RoundTripResult]) -> None:
    print(
        "scheme,mean_group_size,mean_max_group_size,fraction_below_half_full,"
        "fraction_over_capacity,insert_changed_groups,insert_reused_key_fraction,"
        "delete_changed_groups,delete_reused_key_fraction,grind_changed_groups,"
        "grind_reused_key_fraction,normal_boundary_rate,grind_boundary_rate"
    )
    for item in aggregates:
        normal_rate = "" if item.normal_insert_boundary_rate is None else f"{item.normal_insert_boundary_rate:.9f}"
        grind_rate = "" if item.grind_insert_boundary_rate is None else f"{item.grind_insert_boundary_rate:.9f}"
        print(
            f"{item.scheme},{item.mean_group_size:.9f},{item.mean_max_group_size:.9f},"
            f"{item.mean_fraction_below_half_full:.9f},{item.mean_fraction_over_capacity:.9f},"
            f"{item.normal_insert_changed_groups:.9f},{item.normal_insert_reused_key_fraction:.9f},"
            f"{item.delete_changed_groups:.9f},{item.delete_reused_key_fraction:.9f},"
            f"{item.grind_insert_changed_groups:.9f},{item.grind_insert_reused_key_fraction:.9f},"
            f"{normal_rate},{grind_rate}"
        )

    print("# persistent_round_trip")
    print("seed,base_groups,final_groups,reused_groups,reused_group_fraction,reused_key_fraction,logical_equal,partition_equal")
    for item in round_trips:
        print(
            f"{item.seed},{item.base_groups},{item.final_groups},{item.reused_groups},"
            f"{item.reused_group_fraction:.9f},{item.reused_key_fraction:.9f},"
            f"{int(item.final_logical_set_equal)},{int(item.final_partition_equal)}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--quick", action="store_true", help="use the shorter deterministic CI ensemble")
    parser.add_argument("--json", action="store_true", help="emit JSON")
    args = parser.parse_args()

    objects = QUICK_OBJECTS if args.quick else FULL_OBJECTS
    edits_per_kind = QUICK_EDITS_PER_KIND if args.quick else FULL_EDITS_PER_KIND
    seeds = QUICK_SEEDS if args.quick else FULL_SEEDS

    shapes: list[ShapeResult] = []
    edits: list[EditResult] = []
    round_trips: list[RoundTripResult] = []
    for seed in seeds:
        seed_shapes, seed_edits, seed_round_trip = run_seed(seed, objects, edits_per_kind)
        shapes.extend(seed_shapes)
        edits.extend(seed_edits)
        round_trips.append(seed_round_trip)

    aggregates = aggregate(shapes, edits)
    self_check(aggregates, round_trips)

    if args.json:
        print(
            json.dumps(
                {
                    "configuration": {
                        "page_size": PAGE_SIZE,
                        "page_header_len": PAGE_HEADER_LEN,
                        "leaf_entry_len": LEAF_ENTRY_LEN,
                        "capacity": CAPACITY,
                        "minimum": MINIMUM,
                        "window": WINDOW,
                        "anchor_probability": 1 / CAPACITY,
                        "grind_candidates": GRIND_CANDIDATES,
                        "objects": objects,
                        "edits_per_kind_per_seed": edits_per_kind,
                        "seeds": list(seeds),
                        "quick": args.quick,
                    },
                    "aggregates": [asdict(item) for item in aggregates],
                    "round_trips": [asdict(item) for item in round_trips],
                    "shapes": [asdict(item) for item in shapes],
                    "edits": [asdict(item) for item in edits],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print_csv(aggregates, round_trips)


if __name__ == "__main__":
    main()
