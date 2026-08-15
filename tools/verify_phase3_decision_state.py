#!/usr/bin/env python3
"""Validate the non-normative machine-readable Phase 3 D1-D7 state.

The ledger has no normative effect. This tool exists to prevent an ambiguous
half-selected governance state: all D1-D7 entries must exist, and any entry
marked selected must carry enough maintainer/corpus information to be reviewed
as a real decision rather than inferred implementation policy.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_STATE = ROOT / "docs" / "phase3-d1-d7-state.json"
EXPECTED = [f"D{i}" for i in range(1, 8)]
ALLOWED_STATUS = {"unselected", "selected"}


class DecisionStateError(RuntimeError):
    pass


def load(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise DecisionStateError(f"cannot read decision state: {exc}") from exc
    if not isinstance(payload, dict):
        raise DecisionStateError("decision state must be a JSON object")
    return payload


def nonempty_text(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def validate(payload: dict) -> dict:
    if payload.get("schema") != "ucof-phase3-d1-d7-state-v1":
        raise DecisionStateError("unexpected Phase 3 decision-state schema")
    if payload.get("normative_effect") is not False:
        raise DecisionStateError("machine-readable decision ledger must not claim normative effect")
    if payload.get("source_packet") != "docs/PHASE3_D1_D7_DECISION_PACKET.md":
        raise DecisionStateError("decision ledger source packet mismatch")

    decisions = payload.get("decisions")
    if not isinstance(decisions, dict):
        raise DecisionStateError("decision ledger is missing decisions object")
    if list(decisions) != EXPECTED:
        raise DecisionStateError("decision ledger must contain D1..D7 in canonical order")

    selected: list[str] = []
    unselected: list[str] = []
    for decision_id in EXPECTED:
        entry = decisions[decision_id]
        if not isinstance(entry, dict):
            raise DecisionStateError(f"{decision_id} must be an object")
        if not nonempty_text(entry.get("title")):
            raise DecisionStateError(f"{decision_id} is missing title")
        status = entry.get("status")
        if status not in ALLOWED_STATUS:
            raise DecisionStateError(f"{decision_id} has invalid status")

        locations = entry.get("normative_locations")
        if not isinstance(locations, list) or not all(nonempty_text(item) for item in locations):
            if locations != []:
                raise DecisionStateError(f"{decision_id} normative_locations must be text strings")

        if status == "unselected":
            unselected.append(decision_id)
            if entry.get("selection") is not None:
                raise DecisionStateError(f"{decision_id} unselected decision has selection")
            if entry.get("rationale") is not None:
                raise DecisionStateError(f"{decision_id} unselected decision has rationale")
            if locations:
                raise DecisionStateError(f"{decision_id} unselected decision has normative locations")
            if entry.get("corpus_impact") is not None:
                raise DecisionStateError(f"{decision_id} unselected decision has corpus impact")
            if entry.get("review_reference") is not None:
                raise DecisionStateError(f"{decision_id} unselected decision has review reference")
            continue

        selected.append(decision_id)
        required_text = ("selection", "rationale", "corpus_impact", "review_reference")
        missing = [name for name in required_text if not nonempty_text(entry.get(name))]
        if missing:
            raise DecisionStateError(
                f"{decision_id} selected decision is missing: {', '.join(missing)}"
            )
        if not locations:
            raise DecisionStateError(f"{decision_id} selected decision has no normative locations")

    return {
        "selected": selected,
        "unselected": unselected,
        "all_selected": len(selected) == len(EXPECTED),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", type=Path, default=DEFAULT_STATE)
    args = parser.parse_args()
    path = args.state if args.state.is_absolute() else ROOT / args.state
    try:
        summary = validate(load(path))
    except DecisionStateError as exc:
        print(f"Phase 3 D1-D7 decision state: FAIL: {exc}")
        return 1
    print("Phase 3 D1-D7 decision state: PASS")
    print(f"selected={','.join(summary['selected']) or 'none'}")
    print(f"unselected={','.join(summary['unselected']) or 'none'}")
    print(f"all_selected={str(summary['all_selected']).lower()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
