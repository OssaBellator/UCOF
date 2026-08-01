#!/usr/bin/env python3
"""Verify independent fresh-process spill restart classification traces."""

from __future__ import annotations

from hashlib import sha256
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "tests" / "vectors" / "phase-3-spill" / "restart-traces.json"


def manual(label: str) -> str:
    return f"manual:{label}"


def classify_unpublished(facts: dict) -> str:
    if facts["dest"]:
        if facts["dest_validation"] == "invalid":
            return manual("invalid-destination")
        return "indeterminate"
    if not facts["stage"]:
        return "nothing"
    if facts["owner"] == "foreign":
        return "preserve-foreign"
    if facts["owner"] == "unverifiable":
        return manual("unverifiable-staged-ownership")
    if facts["stage_validation"] == "valid":
        return "retry"
    if facts["stage_validation"] == "invalid":
        return "remove-invalid"
    return manual("unvalidated-owned-stage")


def classify(facts: dict) -> str:
    journal = facts["journal"]
    if journal is None:
        return classify_unpublished(facts)
    if not journal["authenticated"]:
        return manual("unauthenticated-restart-journal")
    if not journal["ownership_matches"]:
        return manual("restart-journal-ownership")

    phase = journal["phase"]
    if phase == "staged":
        if not facts["stage"]:
            return manual("missing-synced-stage")
        return classify_unpublished(facts)
    if phase == "linked":
        if facts["dest"]:
            if facts["dest_validation"] == "invalid":
                return manual("invalid-linked-destination")
            return "indeterminate"
        if not facts["stage"]:
            return manual("linked-destination-and-stage-missing")
        return classify_unpublished(facts)
    if phase == "synced":
        if not facts["dest"] or facts["dest_validation"] != "valid":
            return manual("durable-destination-contradiction")
        if not facts["stage"]:
            return "durable"
        if facts["owner"] == "owned" and facts["stage_validation"] == "valid":
            return "durable-cleanup"
        if facts["owner"] == "foreign":
            return manual("foreign-stage-after-durability")
        return manual("untrusted-stage-after-durability")
    if phase == "retired":
        if not facts["dest"] or facts["dest_validation"] != "valid":
            return manual("retired-publication-contradiction")
        if facts["stage"]:
            return manual("private-name-exists-after-retirement")
        return "durable"
    raise AssertionError(f"unknown phase {phase!r}")


def main() -> None:
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if contract["status"] != "non-normative spill restart classification traces":
        raise AssertionError("restart trace status")
    cases = contract["cases"]
    names = [case["name"] for case in cases]
    if len(cases) != 16 or len(names) != len(set(names)):
        raise AssertionError("restart trace set")

    aggregate = sha256()
    for case in cases:
        actual = classify(case["facts"])
        if actual != case["expected"]:
            raise AssertionError(
                f"{case['name']}: expected {case['expected']!r}, received {actual!r}"
            )
        aggregate.update(case["name"].encode("utf-8"))
        aggregate.update(actual.encode("utf-8"))
        print(f"{case['name']}: {actual}")

    actual_aggregate = aggregate.hexdigest()
    if actual_aggregate != contract["aggregate_sha256"]:
        raise AssertionError(
            f"aggregate expected {contract['aggregate_sha256']}, received {actual_aggregate}"
        )
    print(f"restart_traces={len(cases)}")
    print(f"aggregate_sha256={actual_aggregate}")
    print("destination_name_without_sync_is_indeterminate=pass")
    print("foreign_state_is_preserved=pass")
    print("durable_cleanup_requires_authenticated_journal=pass")
    print("contradictions_require_manual_intervention=pass")


if __name__ == "__main__":
    main()
