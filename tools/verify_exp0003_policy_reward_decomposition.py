#!/usr/bin/env python3
"""Verify and decompose the EXP-0003 deletion-policy trace reward histogram."""

from __future__ import annotations

import csv
import io
import subprocess
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "tests/vectors/exp-0003-policy-reward-decomposition/manifest.csv"
PAGE_SIZE = 16_384
CYCLES_PER_TRACE = 48
TRACE_COUNT = 5
OPERATIONS_PER_CYCLE = 2
OPERATIONS_PER_POLICY = CYCLES_PER_TRACE * TRACE_COUNT * OPERATIONS_PER_CYCLE


def reproduce() -> str:
    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--locked",
            "-q",
            "-p",
            "ucof-experiments",
            "--example",
            "exp0003_delete_policy_trace_matrix",
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    rows = [
        line.removeprefix("histogram,")
        for line in completed.stdout.splitlines()
        if line.startswith("histogram,")
    ]
    return "trace,policy,operation,pages_written,count\n" + "\n".join(rows) + "\n"


def parse(manifest: str) -> list[dict[str, str]]:
    return list(csv.DictReader(io.StringIO(manifest)))


def main() -> None:
    reproduced = reproduce()
    expected = MANIFEST.read_text(encoding="utf-8")
    if reproduced != expected:
        raise AssertionError("EXP-0003 policy reward histogram drifted")

    rows = parse(reproduced)
    counts: dict[tuple[str, str, int], int] = defaultdict(int)
    per_trace_expensive: dict[tuple[str, str], int] = defaultdict(int)
    total_operations: dict[str, int] = defaultdict(int)
    total_pages: dict[str, int] = defaultdict(int)

    for row in rows:
        trace = row["trace"]
        policy = row["policy"]
        operation = row["operation"]
        pages = int(row["pages_written"])
        count = int(row["count"])
        if pages not in (2, 3):
            raise AssertionError(f"unexpected page-write reward class: {pages}")
        if operation not in ("delete", "insert"):
            raise AssertionError(f"unexpected operation class: {operation}")

        counts[(policy, operation, pages)] += count
        per_trace_expensive[(trace, policy)] += count if pages == 3 else 0
        total_operations[policy] += count
        total_pages[policy] += pages * count

    for policy in ("left-first", "fuller-sibling"):
        assert total_operations[policy] == OPERATIONS_PER_POLICY

    assert total_pages["left-first"] == 1_004
    assert total_pages["fuller-sibling"] == 966
    assert counts[("left-first", "delete", 3)] == 42
    assert counts[("left-first", "insert", 3)] == 2
    assert counts[("fuller-sibling", "delete", 3)] == 6
    assert counts[("fuller-sibling", "insert", 3)] == 0

    expected_expensive_by_trace = {
        ("whole-set-lcg", "left-first"): 20,
        ("whole-set-lcg", "fuller-sibling"): 3,
        ("left-leaf-hot", "left-first"): 0,
        ("left-leaf-hot", "fuller-sibling"): 0,
        ("middle-leaf-hot", "left-first"): 1,
        ("middle-leaf-hot", "fuller-sibling"): 1,
        ("right-leaf-hot", "left-first"): 0,
        ("right-leaf-hot", "fuller-sibling"): 0,
        ("left-middle-boundary-hot", "left-first"): 23,
        ("left-middle-boundary-hot", "fuller-sibling"): 2,
    }
    assert dict(per_trace_expensive) == expected_expensive_by_trace

    # With a common reward map r(2-page)=2 and r(3-page)=3, and the same number
    # of operations under both policies, the finite-trace reward difference is
    # exactly the change in visitation count of the 3-page class.
    left_expensive = counts[("left-first", "delete", 3)] + counts[
        ("left-first", "insert", 3)
    ]
    fuller_expensive = counts[("fuller-sibling", "delete", 3)] + counts[
        ("fuller-sibling", "insert", 3)
    ]
    page_saving = total_pages["left-first"] - total_pages["fuller-sibling"]
    visitation_saving = left_expensive - fuller_expensive
    direct_reward_map_saving = page_saving - visitation_saving

    assert left_expensive == 44
    assert fuller_expensive == 6
    assert page_saving == 38
    assert visitation_saving == page_saving
    assert direct_reward_map_saving == 0

    byte_saving = PAGE_SIZE * page_saving
    assert byte_saving == 622_592

    left_mean = total_pages["left-first"] / OPERATIONS_PER_POLICY
    fuller_mean = total_pages["fuller-sibling"] / OPERATIONS_PER_POLICY

    print(f"verified_policy_reward_manifest={MANIFEST.relative_to(ROOT)}")
    print(f"operations_per_policy={OPERATIONS_PER_POLICY}")
    print(f"left_first_pages={total_pages['left-first']}")
    print(f"fuller_sibling_pages={total_pages['fuller-sibling']}")
    print(f"left_first_three_page_transitions={left_expensive}")
    print(f"fuller_sibling_three_page_transitions={fuller_expensive}")
    print(f"page_saving={page_saving}")
    print(f"visitation_term_pages={visitation_saving}")
    print(f"direct_reward_map_term_pages={direct_reward_map_saving}")
    print(f"byte_saving_from_page_delta={byte_saving}")
    print(f"left_first_mean_pages_per_operation={left_mean:.9f}")
    print(f"fuller_sibling_mean_pages_per_operation={fuller_mean:.9f}")


if __name__ == "__main__":
    main()
