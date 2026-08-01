#!/usr/bin/env python3
"""Generate and verify independent immutable-successor mixed transition recipes."""

from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

import experiment_exp0002_immutable_canonical_occupancy as canonical
import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_objects as objects

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = (
    ROOT
    / "tests"
    / "vectors"
    / "exp-0002-immutable"
    / "mixed-transition-recipes.json"
)


@dataclass(frozen=True)
class TransitionResult:
    data: bytes
    pages_written: int
    pages_reused: int


def base_values(recipe: dict) -> list[objects.ObjectInput]:
    count = recipe["base_object_count"]
    stride = recipe["base_identifier_stride"]
    return [
        objects.ObjectInput(
            index * stride,
            1 + (index * stride) % 19,
            f"payload:{index * stride}".encode("ascii"),
        )
        for index in range(1, count + 1)
    ]


def operation_id(operation: dict) -> int:
    object_id = operation["object_id"]
    if not isinstance(object_id, int) or object_id <= 0:
        raise AssertionError("operation identifier")
    return object_id


def operation_input(operation: dict) -> objects.ObjectInput:
    return objects.ObjectInput(
        operation_id(operation),
        operation["kind"],
        operation["payload_utf8"].encode("utf-8"),
    )


def inventory_pages(data: bytes, root: cow.PageRef) -> dict[int, dict[bytes, cow.PageRef]]:
    inventory: dict[int, dict[bytes, cow.PageRef]] = {}
    stack = [root]
    while stack:
        reference = stack.pop()
        page = cow.checked_slice(data, reference.offset, cow.PAGE_SIZE, "inventory page")
        by_body = inventory.setdefault(reference.level, {})
        if page in by_body:
            raise AssertionError("duplicate authenticated page body")
        by_body[page] = reference
        kind, entries = cow.decode_page(data, reference)
        if kind == 2:
            stack.extend(
                reversed([entry for entry in entries if isinstance(entry, cow.PageRef)])
            )
    return inventory


def append_or_reuse(
    output: bytearray,
    page: bytes,
    level: int,
    inventory: dict[int, dict[bytes, cow.PageRef]],
) -> tuple[cow.PageRef, bool]:
    existing = inventory.get(level, {}).get(page)
    if existing is not None:
        return existing, False
    return cow.append_page(output, page), True


def build_exact_reuse_tree(
    output: bytearray,
    locators: list[cow.Locator],
    inventory: dict[int, dict[bytes, cow.PageRef]],
) -> tuple[cow.PageRef, int]:
    if not locators:
        raise AssertionError("empty final directory")

    current: list[cow.PageRef] = []
    pages_written = 0
    start = 0
    for size in canonical.canonical_group_sizes(
        len(locators), cow.LEAF_CAPACITY, canonical.LEAF_MIN_OCCUPANCY
    ):
        end = start + size
        page = cow.encode_leaf(locators[start:end])
        reference, written = append_or_reuse(output, page, 0, inventory)
        current.append(reference)
        pages_written += int(written)
        start = end

    level = 1
    while len(current) > 1:
        next_level: list[cow.PageRef] = []
        start = 0
        for size in canonical.canonical_group_sizes(
            len(current), cow.INTERNAL_FANOUT, canonical.INTERNAL_MIN_OCCUPANCY
        ):
            end = start + size
            page = cow.encode_internal(current[start:end], level)
            reference, written = append_or_reuse(output, page, level, inventory)
            next_level.append(reference)
            pages_written += int(written)
            start = end
        current = next_level
        level += 1
    return current[0], pages_written


def apply_mixed_transition(base: bytes, operations: list[dict]) -> TransitionResult:
    verified = objects.validate_complete(base)
    canonical.validate_canonical_occupancy(base)
    ordered = sorted(operations, key=operation_id)
    identifiers = [operation_id(operation) for operation in ordered]
    if not ordered or len(identifiers) != len(set(identifiers)):
        raise AssertionError("operation identifiers must be unique")

    active = {locator.object_id: locator for locator in verified.objects}
    original_ids = set(active)
    for operation in ordered:
        object_id = operation_id(operation)
        if operation["operation"] == "delete":
            if object_id not in original_ids:
                raise AssertionError("delete target is absent")
        elif operation["operation"] == "put":
            if operation["kind"] <= 0:
                raise AssertionError("put kind")
        else:
            raise AssertionError("unknown mixed operation")

    output = bytearray(base)
    for operation in ordered:
        object_id = operation_id(operation)
        if operation["operation"] == "delete":
            del active[object_id]
        else:
            active[object_id] = objects.append_object(output, operation_input(operation))
    if not active:
        raise AssertionError("empty final directory")

    final_locators = [active[object_id] for object_id in sorted(active)]
    inventory = inventory_pages(base, verified.structural.root)
    root, pages_written = build_exact_reuse_tree(output, final_locators, inventory)
    cow.publish(
        output,
        verified.structural.sequence + 1,
        root,
        verified.structural.snapshot_digest,
        verified.structural.footer_offset,
        pages_written,
    )
    result = bytes(output)
    report = objects.validate_complete(result)
    canonical.validate_canonical_occupancy(result)
    pages_reused = len(report.structural.reachable_pages) - pages_written
    if pages_reused < 0:
        raise AssertionError("page accounting")
    return TransitionResult(result, pages_written, pages_reused)


def verify_recipe(recipe: dict) -> tuple[str, bytes]:
    base = canonical.build_genesis(base_values(recipe))
    forward = apply_mixed_transition(base, recipe["operations"])
    reverse = apply_mixed_transition(base, list(reversed(recipe["operations"])))
    if forward != reverse:
        raise AssertionError(f"{recipe['name']}: caller order changed output")

    report = objects.validate_complete(forward.data)
    canonical.validate_canonical_occupancy(forward.data)
    actual_sha = sha256(forward.data).hexdigest()
    facts = {
        "decoded_bytes": len(forward.data),
        "sha256": actual_sha,
        "sequence": report.structural.sequence,
        "root_level": report.structural.root.level,
        "page_count": len(report.structural.reachable_pages),
        "object_count": len(report.objects),
        "pages_written": forward.pages_written,
        "pages_reused": forward.pages_reused,
    }
    for key, actual in facts.items():
        expected = recipe[key]
        if expected in (None, ""):
            raise AssertionError(
                f"{recipe['name']}: pin {key} to {actual!r}"
            )
        if actual != expected:
            raise AssertionError(
                f"{recipe['name']}: {key} expected {expected!r}, received {actual!r}"
            )

    payloads = report.object_payloads
    for expectation in recipe["payload_expectations"]:
        object_id = expectation["object_id"]
        if expectation["status"] == "missing":
            if object_id in payloads:
                raise AssertionError(f"{recipe['name']}: object {object_id} remained active")
        elif payloads.get(object_id) != expectation["payload_utf8"].encode("utf-8"):
            raise AssertionError(f"{recipe['name']}: object {object_id} payload")

    print(
        f"{recipe['name']}: bytes={len(forward.data)} sha256={actual_sha} "
        f"sequence={report.structural.sequence} root_level={report.structural.root.level} "
        f"pages={len(report.structural.reachable_pages)} objects={len(report.objects)} "
        f"written={forward.pages_written} reused={forward.pages_reused}"
    )
    return recipe["name"], bytes.fromhex(actual_sha)


def main() -> None:
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if contract["status"] != "non-normative independent mixed transition recipes":
        raise AssertionError("mixed transition recipe status")
    recipes = contract["vectors"]
    names = [recipe["name"] for recipe in recipes]
    if len(recipes) != 3 or len(names) != len(set(names)):
        raise AssertionError("mixed transition recipe set")

    aggregate = sha256()
    for name, output_digest in map(verify_recipe, recipes):
        aggregate.update(name.encode("utf-8"))
        aggregate.update(output_digest)
    actual_aggregate = aggregate.hexdigest()
    expected_aggregate = contract["aggregate_sha256"]
    if expected_aggregate in (None, ""):
        raise AssertionError(f"pin aggregate_sha256 to {actual_aggregate!r}")
    if actual_aggregate != expected_aggregate:
        raise AssertionError(
            f"aggregate SHA-256 expected {expected_aggregate}, received {actual_aggregate}"
        )

    print(f"mixed_transition_vectors={len(recipes)}")
    print(f"aggregate_sha256={actual_aggregate}")
    print("caller_order_determinism=pass")
    print("strict_python_validation=pass")
    print("canonical_occupancy=pass")
    print("exact_page_body_reuse=pass")


if __name__ == "__main__":
    main()
