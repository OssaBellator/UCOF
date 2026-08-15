#!/usr/bin/env python3
"""Initialize a complete Phase 3 destructive power-loss result template.

The generated file is intentionally not valid final evidence until the operator
fills every remaining platform/campaign field and changes every case status from
the placeholder `unexecuted` value to explicit `pass` or `fail` with
cut/reboot/retry evidence. The strict validator rejects this initial template.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path

from tools import plan_phase3_powerloss_campaign as plan
from tools import verify_phase3_powerloss_results as verify


def template(ucof_git_sha: str = "") -> dict:
    if ucof_git_sha:
        ucof_git_sha = verify.canonical_expected_git_sha(ucof_git_sha)
    platform = {
        field: "" for field in plan.build_plan()["required_platform_metadata"]
    }
    platform["ucof_git_sha"] = ucof_git_sha
    return {
        "schema": verify.SCHEMA,
        "plan_schema": plan.SCHEMA,
        "platform": platform,
        "campaign": {
            "operator": "",
            "started_utc": "",
            "completed_utc": "",
            "evidence_location": "",
            "template_generated_utc": datetime.now(timezone.utc).isoformat(),
        },
        "cases": [
            {
                "case_id": case.case_id,
                "status": "unexecuted",
                "cut_execution_reference": "",
                "reboot_observation": "",
                "retry_result": "",
                "notes": "",
            }
            for case in plan.CASES
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--ucof-git-sha",
        help=(
            "prefill platform.ucof_git_sha with the exact 40-character lowercase "
            "candidate commit SHA"
        ),
    )
    args = parser.parse_args()
    try:
        payload = template(args.ucof_git_sha or "")
    except verify.PowerlossResultError as exc:
        parser.error(str(exc))
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    print(f"initialized={output}")
    print(f"cases={len(plan.CASES)}")
    if args.ucof_git_sha:
        print(f"ucof_git_sha={args.ucof_git_sha}")
    print("final_validator_will_reject_until all required fields/cases are completed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
