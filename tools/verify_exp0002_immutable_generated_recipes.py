#!/usr/bin/env python3
"""Regenerate and verify compact immutable-successor vector recipes."""

from __future__ import annotations

from hashlib import sha256
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

import experiment_exp0002_immutable_canonical_occupancy as canonical
import experiment_exp0002_immutable_page_cow as cow
import experiment_exp0002_immutable_page_objects as objects

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / "tests" / "vectors" / "exp-0002-immutable"
CONTRACT = DIRECTORY / "generated-recipes.json"


def load_base(contract: dict) -> bytes:
    data = bytes.fromhex(
        (DIRECTORY / contract["base_vector"]).read_text(encoding="ascii")
    )
    actual = sha256(data).hexdigest()
    if actual != contract["base_sha256"]:
        raise AssertionError(
            f"base SHA-256 mismatch: expected {contract['base_sha256']}, received {actual}"
        )
    objects.validate_complete(data)
    canonical.validate_canonical_occupancy(data)
    return data


def generate(recipe: dict, base: bytes) -> bytes:
    operation = recipe["operation"]
    if operation == "replace_object":
        return objects.append_replacement(
            base,
            objects.ObjectInput(
                recipe["object_id"],
                recipe["kind"],
                recipe["payload_utf8"].encode("utf-8"),
            ),
        )
    if operation == "generated_genesis":
        first = recipe["first_object_id"]
        last = recipe["last_object_id"]
        values = [
            objects.ObjectInput(
                object_id,
                1 + object_id % 5,
                f"payload:{object_id}".encode("ascii"),
            )
            for object_id in range(first, last + 1)
        ]
        return canonical.build_genesis(values)
    raise AssertionError(f"unknown generated-vector operation: {operation}")


def verify_recipe(recipe: dict, data: bytes) -> None:
    report = objects.validate_complete(data)
    canonical.validate_canonical_occupancy(data)
    actual_sha = sha256(data).hexdigest()
    facts = {
        "decoded_bytes": len(data),
        "sha256": actual_sha,
        "sequence": report.structural.sequence,
        "root_level": report.structural.root.level,
        "page_count": len(report.structural.reachable_pages),
        "object_count": len(report.objects),
    }
    for key, actual in facts.items():
        expected = recipe[key]
        if actual != expected:
            raise AssertionError(
                f"{recipe['name']}: {key} expected {expected!r}, received {actual!r}"
            )
    if data[-cow.FOOTER_LEN : -cow.FOOTER_LEN + 8] != cow.FOOTER_MAGIC:
        raise AssertionError(f"{recipe['name']}: footer is not exact-end")
    print(
        f"{recipe['name']}: bytes={len(data)} sha256={actual_sha} "
        f"sequence={report.structural.sequence} root_level={report.structural.root.level} "
        f"pages={len(report.structural.reachable_pages)} objects={len(report.objects)}"
    )


def main() -> None:
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if contract["status"] != "non-normative successor generated vector recipes":
        raise AssertionError("generated-vector status")
    recipes = contract["vectors"]
    if len(recipes) != 2:
        raise AssertionError("generated-vector recipe count")
    names = [recipe["name"] for recipe in recipes]
    if len(names) != len(set(names)):
        raise AssertionError("duplicate generated-vector name")

    base = load_base(contract)
    aggregate = sha256()
    for recipe in recipes:
        first = generate(recipe, base)
        second = generate(recipe, base)
        if first != second:
            raise AssertionError(f"{recipe['name']}: generation was not deterministic")
        verify_recipe(recipe, first)
        aggregate.update(recipe["name"].encode("utf-8"))
        aggregate.update(sha256(first).digest())

    print(f"generated_vectors={len(recipes)}")
    print(f"aggregate_sha256={aggregate.hexdigest()}")
    print("deterministic_generation=pass")
    print("strict_python_validation=pass")
    print("canonical_occupancy=pass")


if __name__ == "__main__":
    main()
