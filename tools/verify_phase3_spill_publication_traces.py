#!/usr/bin/env python3
"""Independently verify Phase 3 spill publication transition traces."""

from __future__ import annotations

from hashlib import sha256
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "tests" / "vectors" / "phase-3-spill" / "publication-traces.json"


def evaluate(case: dict[str, Any]) -> dict[str, Any]:
    stage = "PrivateStaging"
    outcome = "NotPublished"
    staged_bytes = 0
    staged_files = 0
    cleanup_actions = 0
    error: str | None = None
    limits = case["limits"]

    for event in case["events"]:
        if event.get("owner", "correct") != "correct":
            error = "ownership-mismatch"
            break

        operation = event["op"]
        if operation == "stage":
            if stage != "PrivateStaging":
                error = "invalid-transition"
                break
            next_files = staged_files + 1
            next_bytes = staged_bytes + event["bytes"]
            if next_files > limits["max_staged_files"]:
                error = "limit:file-count"
                break
            if next_bytes > limits["max_staged_bytes"]:
                error = "limit:byte-count"
                break
            staged_files = next_files
            staged_bytes = next_bytes
        elif operation == "validate":
            if stage != "PrivateStaging" or staged_files == 0:
                error = "invalid-transition"
                break
            stage = "OutputValidated"
        elif operation == "file-sync":
            if stage != "OutputValidated":
                error = "invalid-transition"
                break
            if not event["success"]:
                error = "not-published:staged-file-sync"
                break
            stage = "StagedFileSynchronized"
        elif operation == "link":
            if stage != "StagedFileSynchronized":
                error = "invalid-transition"
                break
            result = event["result"]
            if result == "destination-exists":
                error = "destination-exists"
                break
            if result == "not-created":
                error = "not-published:destination-link"
                break
            if result == "indeterminate":
                outcome = "PublicationIndeterminate"
                error = "indeterminate:destination-link"
                break
            if result != "created":
                error = "invalid-link-result"
                break
            stage = "DestinationLinked"
        elif operation == "dir-sync":
            if stage != "DestinationLinked":
                error = "invalid-transition"
                break
            if not event["success"]:
                outcome = "PublicationIndeterminate"
                error = "indeterminate:destination-directory-sync"
                break
            stage = "DestinationDirectorySynchronized"
            outcome = "PublishedAndDurable"
        elif operation in {"retire", "cleanup"}:
            if operation == "retire" and stage != "DestinationDirectorySynchronized":
                error = "invalid-transition"
                break
            next_cleanup = cleanup_actions + 1
            if next_cleanup > limits["max_cleanup_actions"]:
                error = "limit:cleanup-work"
                break
            cleanup_actions = next_cleanup
            if not event["success"]:
                suffix = (
                    "private-name-retirement"
                    if operation == "retire"
                    else "owned-artifact"
                )
                error = f"cleanup-failed:{suffix}"
                break
            if operation == "retire":
                stage = "PrivateNameRetired"
        else:
            error = "invalid-operation"
            break

    return {
        "stage": stage,
        "outcome": outcome,
        "staged_bytes": staged_bytes,
        "staged_files": staged_files,
        "cleanup_actions": cleanup_actions,
        "error": error,
    }


def main() -> None:
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if contract["status"] != "non-normative Phase 3 spill publication transition traces":
        raise AssertionError("spill transition trace status")

    names = [case["name"] for case in contract["cases"]]
    if not names or len(names) != len(set(names)):
        raise AssertionError("spill transition trace names")

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
        if first["error"] and first["outcome"] == "PublishedAndDurable":
            if not first["error"].startswith("cleanup-failed:") and first["error"] != "limit:cleanup-work":
                raise AssertionError(
                    f"{case['name']}: non-cleanup error preserved durable success"
                )
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
    print("independent_spill_publication_traces=pass")


if __name__ == "__main__":
    main()
