#!/usr/bin/env python3
"""Verify pinned non-normative successor support-profile boundary recipes."""

from __future__ import annotations

from dataclasses import replace
from hashlib import sha256
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VECTOR = ROOT / "tests/vectors/exp-0002-immutable-support-profiles/profile-boundaries.json"
MODEL = ROOT / "tools/experiment_exp0002_support_profiles.py"


def load_model():
    specification = importlib.util.spec_from_file_location("support_profiles", MODEL)
    if specification is None or specification.loader is None:
        raise AssertionError("support profile model could not be loaded")
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def mutate(profile, mutation: str, facts: dict[str, int]):
    if mutation == "none":
        return profile
    if mutation == "max_read_operations=required-1":
        return replace(profile, max_read_operations=facts["required_read_operations"] - 1)
    if mutation == "max_bytes_read=required-1":
        return replace(profile, max_bytes_read=facts["required_bytes_read"] - 1)
    if mutation == "max_hash_bytes=required-1":
        return replace(profile, max_hash_bytes=facts["required_hash_bytes"] - 1)
    if mutation == "max_request_bytes=max_allocation_bytes+1":
        return replace(profile, max_request_bytes=profile.max_allocation_bytes + 1)
    if mutation == "max_recovery_scan_bytes=max_file_bytes+1":
        return replace(profile, max_recovery_scan_bytes=profile.max_file_bytes + 1)
    raise AssertionError(f"unknown mutation: {mutation}")


def profile_dict(profile) -> dict[str, int]:
    return {
        "max_file_bytes": profile.max_file_bytes,
        "max_objects": profile.max_objects,
        "max_object_bytes": profile.max_object_bytes,
        "max_request_bytes": profile.max_request_bytes,
        "max_read_operations": profile.max_read_operations,
        "max_bytes_read": profile.max_bytes_read,
        "max_hash_bytes": profile.max_hash_bytes,
        "max_allocation_bytes": profile.max_allocation_bytes,
        "max_history_depth": profile.max_history_depth,
        "max_recovery_scan_bytes": profile.max_recovery_scan_bytes,
    }


def main() -> None:
    document = json.loads(VECTOR.read_text(encoding="utf-8"))
    assert document["schema"] == "ucof-exp0002-support-profile-boundaries-v1"
    assert document["status"] == "non-normative research vectors"

    model = load_model()
    expanded = []
    accepted = 0
    rejected = 0

    for profile_name, values in document["profiles"].items():
        profile = model.Profile(name=profile_name, **values)
        baseline_facts = model.validate(profile)
        for template in document["templates"]:
            candidate = mutate(profile, template["mutation"], baseline_facts)
            case = {
                "name": f"{profile_name}-{template['suffix']}",
                "profile": profile_dict(candidate),
                "expected": template["expected"],
            }
            if "error_contains" in template:
                case["error_contains"] = template["error_contains"]
            expanded.append(case)

            try:
                model.validate(candidate)
            except AssertionError as error:
                if template["expected"] != "reject":
                    raise AssertionError(f"{case['name']}: unexpected rejection: {error}") from error
                expected = template["error_contains"]
                if expected not in str(error):
                    raise AssertionError(
                        f"{case['name']}: expected error containing {expected!r}, got {error!r}"
                    ) from error
                rejected += 1
            else:
                if template["expected"] != "accept":
                    raise AssertionError(f"{case['name']}: expected rejection")
                accepted += 1

    encoded = json.dumps(expanded, sort_keys=True, separators=(",", ":")).encode("utf-8")
    digest = sha256(encoded).hexdigest()
    assert len(expanded) == document["expanded_case_count"]
    assert digest == document["expanded_cases_sha256"]
    assert accepted == 3
    assert rejected == 15

    print(f"support_profile_boundary_cases={len(expanded)}")
    print(f"support_profile_boundary_sha256={digest}")
    print(f"exact_limit_acceptances={accepted}")
    print(f"one_step_or_structural_rejections={rejected}")
    print("resource_policy_is_not_malformed_input=pass")


if __name__ == "__main__":
    main()
