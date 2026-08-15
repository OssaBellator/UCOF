#!/usr/bin/env python3
"""Validate externally produced Phase 3 destructive power-loss results."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from tools import plan_phase3_powerloss_campaign as plan

SCHEMA = "ucof-phase3-powerloss-campaign-results-v1"


class PowerlossResultError(RuntimeError):
    pass


def nonempty(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def load(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise PowerlossResultError(f"cannot read power-loss result: {exc}") from exc
    if not isinstance(payload, dict):
        raise PowerlossResultError("power-loss result must be a JSON object")
    return payload


def validate(payload: dict) -> dict:
    if payload.get("schema") != SCHEMA:
        raise PowerlossResultError("unexpected power-loss result schema")
    if payload.get("plan_schema") != plan.SCHEMA:
        raise PowerlossResultError("power-loss result plan schema mismatch")
    platform = payload.get("platform")
    if not isinstance(platform, dict):
        raise PowerlossResultError("power-loss result is missing platform metadata")
    missing_platform = [name for name in plan.build_plan()["required_platform_metadata"] if not nonempty(platform.get(name))]
    if missing_platform:
        raise PowerlossResultError(
            "power-loss platform metadata missing: " + ", ".join(missing_platform)
        )

    cases = payload.get("cases")
    if not isinstance(cases, list):
        raise PowerlossResultError("power-loss cases must be a list")
    expected = [case.case_id for case in plan.CASES]
    observed_ids: list[str] = []
    failures: list[str] = []
    for entry in cases:
        if not isinstance(entry, dict):
            raise PowerlossResultError("power-loss case entry must be an object")
        case_id = entry.get("case_id")
        if not nonempty(case_id):
            raise PowerlossResultError("power-loss case entry is missing case_id")
        observed_ids.append(case_id)
        status = entry.get("status")
        if status not in {"pass", "fail"}:
            raise PowerlossResultError(
                f"{case_id} status must be pass or fail; skipped/unknown cases are not accepted"
            )
        if not nonempty(entry.get("cut_execution_reference")):
            raise PowerlossResultError(f"{case_id} is missing cut execution reference")
        if not nonempty(entry.get("reboot_observation")):
            raise PowerlossResultError(f"{case_id} is missing reboot observation")
        if not nonempty(entry.get("retry_result")):
            raise PowerlossResultError(f"{case_id} is missing retry result")
        if status == "fail":
            failures.append(case_id)

    if observed_ids != expected:
        raise PowerlossResultError("power-loss cases must match the complete canonical plan order")
    if len(observed_ids) != len(set(observed_ids)):
        raise PowerlossResultError("duplicate power-loss case id")

    campaign = payload.get("campaign")
    if not isinstance(campaign, dict):
        raise PowerlossResultError("power-loss result is missing campaign metadata")
    for field in ("operator", "started_utc", "completed_utc", "evidence_location"):
        if not nonempty(campaign.get(field)):
            raise PowerlossResultError(f"power-loss campaign is missing {field}")

    return {
        "ok": not failures,
        "case_count": len(expected),
        "failed_cases": failures,
        "ucof_git_sha": platform["ucof_git_sha"],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("result", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        summary = validate(load(args.result))
    except PowerlossResultError as exc:
        print(f"Phase 3 power-loss results: FAIL: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
