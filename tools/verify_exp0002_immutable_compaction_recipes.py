#!/usr/bin/env python3
"""Independently verify immutable-successor semantic compaction graph recipes."""

from __future__ import annotations

from hashlib import sha256
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = (
    ROOT
    / "tests"
    / "vectors"
    / "exp-0002-immutable-compaction"
    / "graph-recipes.json"
)


def error(label: str) -> dict[str, str]:
    return {"error": label}


def evaluate(contract: dict[str, Any], case: dict[str, Any]) -> dict[str, Any]:
    objects = {entry["object_id"]: entry for entry in contract["objects"]}
    limits = case["limits"]
    supplied_roots = case["roots"]
    if not supplied_roots or len(supplied_roots) > limits["max_roots"]:
        return error("invalid-selection")

    roots = sorted(set(supplied_roots))
    if len(roots) > limits["max_roots"]:
        return error("limit:root-count")
    for root in roots:
        if root not in objects:
            return error(f"missing-root:{root}")

    retained: set[int] = set()
    stack = [(root, 0) for root in reversed(roots)]
    edges_visited = 0
    maximum_depth = 0
    unknown_trigger: int | None = None
    conservative_full_retention = False

    while stack:
        object_id, depth = stack.pop()
        if depth > limits["max_depth"]:
            return error("limit:dependency-depth")
        maximum_depth = max(maximum_depth, depth)
        if object_id in retained:
            continue
        if len(retained) >= limits["max_nodes"]:
            return error("limit:node-count")

        entry = objects.get(object_id)
        if entry is None:
            return error(f"missing-root:{object_id}")
        retained.add(object_id)
        resolution = entry["resolution"]

        if "known" in resolution:
            dependencies = sorted(set(resolution["known"]))
            edges_visited += len(dependencies)
            if edges_visited > limits["max_edges"]:
                return error("limit:edge-count")
            next_depth = depth + 1
            for dependency_id in reversed(dependencies):
                if dependency_id not in objects:
                    return error(f"missing:{object_id}->{dependency_id}")
                if dependency_id not in retained:
                    stack.append((dependency_id, next_depth))
        elif resolution.get("unknown") is True:
            if case["unknown_policy"] == "reject":
                return error(f"unknown:{object_id}")
            if case["unknown_policy"] != "retain-all":
                return error("invalid-unknown-policy")
            unknown_trigger = object_id
            conservative_full_retention = True
            retained = set(objects)
            break
        elif "error" in resolution:
            return error(f"resolver:{object_id}:{resolution['error']}")
        else:
            return error("invalid-resolution")

    if len(retained) > limits["max_nodes"]:
        return error("limit:node-count")

    result: dict[str, Any] = {
        "selected_roots": roots,
        "retained": sorted(retained),
        "discarded": sorted(set(objects) - retained),
        "edges_visited": edges_visited,
        "maximum_depth": maximum_depth,
    }
    if unknown_trigger is not None:
        result["unknown_trigger"] = unknown_trigger
    if conservative_full_retention:
        result["conservative_full_retention"] = True
    return result


def verify_expected(name: str, actual: dict[str, Any], expected: dict[str, Any]) -> None:
    for key, value in expected.items():
        if actual.get(key) != value:
            raise AssertionError(
                f"{name}: {key} expected {value!r}, received {actual.get(key)!r}; "
                f"actual={actual!r}"
            )
    if "error" in expected and set(actual) != {"error"}:
        raise AssertionError(f"{name}: error result carried extra claims: {actual!r}")


def main() -> None:
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if contract["status"] != "non-normative immutable successor semantic compaction recipes":
        raise AssertionError("semantic compaction recipe status")

    object_ids = [entry["object_id"] for entry in contract["objects"]]
    if not object_ids or len(object_ids) != len(set(object_ids)) or any(value <= 0 for value in object_ids):
        raise AssertionError("semantic compaction object identifiers")

    names = [case["name"] for case in contract["cases"]]
    if not names or len(names) != len(set(names)):
        raise AssertionError("semantic compaction case names")

    results: list[dict[str, Any]] = []
    for case in contract["cases"]:
        first = evaluate(contract, case)
        second = evaluate(contract, case)
        if first != second:
            raise AssertionError(f"{case['name']}: non-deterministic evaluation")
        verify_expected(case["name"], first, case["expected"])
        results.append({"name": case["name"], "result": first})
        print(f"{case['name']}: {json.dumps(first, sort_keys=True, separators=(',', ':'))}")

    encoded = json.dumps(results, sort_keys=True, separators=(",", ":")).encode("utf-8")
    aggregate = sha256(encoded).hexdigest()
    if aggregate != contract["aggregate_sha256"]:
        raise AssertionError(
            f"aggregate SHA-256 expected {contract['aggregate_sha256']}, received {aggregate}"
        )
    print(f"cases={len(results)}")
    print(f"aggregate_sha256={aggregate}")
    print("independent_semantic_compaction=pass")


if __name__ == "__main__":
    main()
