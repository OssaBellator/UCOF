#!/usr/bin/env python3
"""Independently verify immutable-successor conditional retry traces."""

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
    / "exp-0002-immutable-transport"
    / "retry-traces.json"
)

TERMINAL_OUTCOMES = {"terminal", "version-change", "protocol", "cancelled", "deadline"}


def evaluate(case: dict[str, Any]) -> dict[str, Any]:
    maximum = case["max_attempts"]
    if maximum <= 0:
        return {
            "result": "invalid-policy",
            "transport_attempts": 0,
            "logical_requests": 0,
            "accepted_bytes": 0,
        }

    attempts = 0
    logical_requests = 0
    accepted_bytes = 0
    controls = case.get("control_events", [])

    for event in controls:
        if event.get("before_metadata") is True:
            return {
                "result": event["state"],
                "transport_attempts": 0,
                "logical_requests": 0,
                "accepted_bytes": 0,
            }

    def run_attempts(outcomes: list[str]) -> str:
        nonlocal attempts
        for outcome in outcomes:
            if attempts >= maximum:
                return "attempt-limit"
            attempts += 1
            if outcome == "retryable":
                if attempts >= maximum:
                    return "attempt-limit"
                continue
            if outcome == "success" or outcome in TERMINAL_OUTCOMES:
                return outcome
            return "invalid-outcome"
        return "trace-exhausted"

    result = run_attempts(case["metadata"])
    if result != "success":
        return {
            "result": result,
            "transport_attempts": attempts,
            "logical_requests": logical_requests,
            "accepted_bytes": accepted_bytes,
        }

    for range_index, range_trace in enumerate(case["ranges"]):
        logical_requests += 1
        matching_controls = [
            event
            for event in controls
            if event.get("before_range") == range_index
        ]
        if matching_controls:
            return {
                "result": matching_controls[0]["state"],
                "transport_attempts": attempts,
                "logical_requests": logical_requests,
                "accepted_bytes": accepted_bytes,
            }
        result = run_attempts(range_trace["outcomes"])
        if result != "success":
            return {
                "result": result,
                "transport_attempts": attempts,
                "logical_requests": logical_requests,
                "accepted_bytes": accepted_bytes,
            }
        accepted_bytes += range_trace["length"]

    return {
        "result": "success",
        "transport_attempts": attempts,
        "logical_requests": logical_requests,
        "accepted_bytes": accepted_bytes,
    }


def main() -> None:
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if contract["status"] != "non-normative immutable successor conditional retry traces":
        raise AssertionError("conditional retry trace status")

    names = [case["name"] for case in contract["cases"]]
    if not names or len(names) != len(set(names)):
        raise AssertionError("conditional retry trace names")

    results: list[dict[str, Any]] = []
    for case in contract["cases"]:
        first = evaluate(case)
        second = evaluate(case)
        if first != second:
            raise AssertionError(f"{case['name']}: non-deterministic evaluation")
        if first != case["expected"]:
            raise AssertionError(
                f"{case['name']}: expected {case['expected']!r}, received {first!r}"
            )
        if first["accepted_bytes"] > 0 and first["result"] != "success":
            raise AssertionError(f"{case['name']}: failed trace accepted bytes")
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
    print("independent_conditional_retry_traces=pass")


if __name__ == "__main__":
    main()
