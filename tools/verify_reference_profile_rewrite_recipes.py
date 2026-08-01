#!/usr/bin/env python3
"""Generate and verify independent reference-profile rewrite byte recipes."""

from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
import json
from pathlib import Path
import struct
import sys
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

import experiment_exp0002_immutable_page_objects as objects

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = (
    ROOT
    / "tests"
    / "vectors"
    / "exp-0002-immutable"
    / "reference-profile-rewrite-recipes.json"
)


@dataclass(frozen=True)
class PlannedRewrite:
    retained: tuple[int, ...]
    discarded: tuple[int, ...]
    edges_visited: int
    maximum_depth: int


@dataclass(frozen=True)
class VerifiedRecipe:
    name: str
    output_digest: bytes
    facts: dict[str, int | str | list[int]]
    pin_errors: tuple[str, ...]


def encode_reference_list(identifiers: list[int], maximum: int) -> bytes:
    if len(identifiers) > maximum:
        raise AssertionError("reference count")
    previous: int | None = None
    for identifier in identifiers:
        if identifier <= 0 or (previous is not None and previous >= identifier):
            raise AssertionError("reference order")
        previous = identifier
    return (
        struct.pack("<I", len(identifiers))
        + bytes(4)
        + b"".join(struct.pack("<Q", identifier) for identifier in identifiers)
    )


def decode_reference_list(payload: bytes, maximum: int) -> list[int]:
    if len(payload) < 8 or payload[4:8] != bytes(4):
        raise AssertionError("reference header")
    count = struct.unpack("<I", payload[:4])[0]
    if count > maximum or len(payload) != 8 + count * 8:
        raise AssertionError("reference length")
    identifiers: list[int] = []
    previous: int | None = None
    for index in range(count):
        start = 8 + index * 8
        identifier = struct.unpack("<Q", payload[start : start + 8])[0]
        if identifier <= 0 or (previous is not None and previous >= identifier):
            raise AssertionError("reference order")
        previous = identifier
        identifiers.append(identifier)
    return identifiers


def recipe_inputs(contract: dict[str, Any], recipe: dict[str, Any]) -> list[objects.ObjectInput]:
    reference_kind = contract["reference_kind"]
    leaf_kind = contract["leaf_kind"]
    maximum = contract["max_dependencies_per_object"]
    values: list[objects.ObjectInput] = []
    previous = 0
    for item in recipe["objects"]:
        object_id = item["object_id"]
        if not isinstance(object_id, int) or object_id <= previous:
            raise AssertionError("recipe object order")
        previous = object_id
        profile = item["profile"]
        if profile == "reference":
            payload = encode_reference_list(item["dependencies"], maximum)
            kind = reference_kind
        elif profile == "leaf":
            if "dependencies" in item:
                raise AssertionError("leaf dependencies")
            payload = b""
            kind = leaf_kind
        else:
            raise AssertionError("recipe profile")
        values.append(objects.ObjectInput(object_id, kind, payload))
    return values


def plan_rewrite(
    values: list[objects.ObjectInput],
    roots: list[int],
    reference_kind: int,
    leaf_kind: int,
    maximum: int,
) -> PlannedRewrite:
    by_id = {value.object_id: value for value in values}
    if len(by_id) != len(values) or not roots:
        raise AssertionError("recipe graph")
    stack = [(root, 0) for root in reversed(roots)]
    visited: set[int] = set()
    edges_visited = 0
    maximum_depth = 0
    while stack:
        object_id, depth = stack.pop()
        if object_id in visited:
            continue
        value = by_id.get(object_id)
        if value is None:
            raise AssertionError("missing dependency")
        visited.add(object_id)
        maximum_depth = max(maximum_depth, depth)
        if value.kind == reference_kind:
            dependencies = decode_reference_list(value.payload, maximum)
        elif value.kind == leaf_kind:
            if value.payload:
                raise AssertionError("leaf payload")
            dependencies = []
        else:
            raise AssertionError("unknown profile kind")
        edges_visited += len(dependencies)
        stack.extend((dependency, depth + 1) for dependency in reversed(dependencies))
    retained = tuple(sorted(visited))
    discarded = tuple(sorted(set(by_id) - visited))
    return PlannedRewrite(retained, discarded, edges_visited, maximum_depth)


def verify_recipe(contract: dict[str, Any], recipe: dict[str, Any]) -> VerifiedRecipe:
    values = recipe_inputs(contract, recipe)
    source = objects.build_genesis(values)
    source_report = objects.validate_complete(source)
    if len(source_report.objects) != len(values):
        raise AssertionError("source object count")

    roots = recipe["roots"]
    plan = plan_rewrite(
        values,
        roots,
        contract["reference_kind"],
        contract["leaf_kind"],
        contract["max_dependencies_per_object"],
    )
    reverse_plan = plan_rewrite(
        values,
        list(reversed(roots)),
        contract["reference_kind"],
        contract["leaf_kind"],
        contract["max_dependencies_per_object"],
    )
    if plan != reverse_plan:
        raise AssertionError(f"{recipe['name']}: root order changed closure")

    retained_values = [value for value in values if value.object_id in plan.retained]
    output = objects.build_genesis(retained_values)
    output_report = objects.validate_complete(output)
    actual_sha = sha256(output).hexdigest()
    facts: dict[str, int | str | list[int]] = {
        "retained_object_ids": list(plan.retained),
        "discarded_object_ids": list(plan.discarded),
        "edges_visited": plan.edges_visited,
        "maximum_depth": plan.maximum_depth,
        "decoded_bytes": len(output),
        "sha256": actual_sha,
        "root_level": output_report.structural.root.level,
        "page_count": len(output_report.structural.reachable_pages),
        "object_count": len(output_report.objects),
    }
    pin_errors: list[str] = []
    for key, actual in facts.items():
        expected = recipe[key]
        if expected in (None, ""):
            pin_errors.append(f"{recipe['name']}: pin {key} to {actual!r}")
        elif actual != expected:
            pin_errors.append(
                f"{recipe['name']}: {key} expected {expected!r}, received {actual!r}"
            )

    expected_payloads = {value.object_id: value.payload for value in retained_values}
    if output_report.object_payloads != expected_payloads:
        raise AssertionError(f"{recipe['name']}: output payload semantics")
    if [locator.object_id for locator in output_report.objects] != list(plan.retained):
        raise AssertionError(f"{recipe['name']}: output object order")

    print(
        f"{recipe['name']}: bytes={facts['decoded_bytes']} sha256={actual_sha} "
        f"root_level={facts['root_level']} pages={facts['page_count']} "
        f"objects={facts['object_count']} retained={list(plan.retained)} "
        f"discarded={list(plan.discarded)} edges={plan.edges_visited} "
        f"depth={plan.maximum_depth}"
    )
    return VerifiedRecipe(
        recipe["name"],
        bytes.fromhex(actual_sha),
        facts,
        tuple(pin_errors),
    )


def main() -> None:
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if contract["status"] != "non-normative reference-profile rewrite recipes":
        raise AssertionError("reference-profile rewrite status")
    recipes = contract["vectors"]
    names = [recipe["name"] for recipe in recipes]
    if len(recipes) != 3 or len(names) != len(set(names)):
        raise AssertionError("reference-profile rewrite recipe set")

    aggregate = sha256()
    pin_errors: list[str] = []
    for result in (verify_recipe(contract, recipe) for recipe in recipes):
        aggregate.update(result.name.encode("utf-8"))
        aggregate.update(result.output_digest)
        pin_errors.extend(result.pin_errors)
    actual_aggregate = aggregate.hexdigest()
    expected_aggregate = contract["aggregate_sha256"]
    if expected_aggregate in (None, ""):
        pin_errors.append(f"pin aggregate_sha256 to {actual_aggregate!r}")
    elif actual_aggregate != expected_aggregate:
        pin_errors.append(
            f"aggregate SHA-256 expected {expected_aggregate}, received {actual_aggregate}"
        )

    print(f"reference_profile_rewrite_vectors={len(recipes)}")
    print(f"aggregate_sha256={actual_aggregate}")
    print("root_order_determinism=pass")
    print("strict_python_validation=pass")
    print("deterministic_python_writer=pass")
    print("payload_semantics=pass")

    if pin_errors:
        print("required_pins:")
        for error in pin_errors:
            print(f"- {error}")
        raise AssertionError(f"{len(pin_errors)} reference-profile rewrite pins require update")


if __name__ == "__main__":
    main()
