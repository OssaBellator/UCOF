#!/usr/bin/env python3
"""Plan additional private filesystem inodes for the Phase 3 writer lifecycle.

This is a non-normative deployment-adjacent companion to the Rust byte quota.
It counts *additional* inodes that may be needed above the files already present
when an operation starts, so its ``required_additional_inodes`` value can feed
``tools/check_phase3_storage_headroom.py --required-inodes``.

The model deliberately counts private file creation, not directory entries and
not open file descriptors. A hard-link publication creates another directory
entry for the same staged-output inode, so it does not add an inode here.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import json
from pathlib import Path


class InodePlanError(RuntimeError):
    pass


@dataclass(frozen=True)
class NormalInodePlan:
    max_initial_runs: int
    spill_run_peak: int
    sort_window: int
    restart_copy_window: int
    restart_manifest_window: int
    source_authority_window: int
    restart_transcode_window: int
    output_tree_window: int
    required_additional_inodes: int


@dataclass(frozen=True)
class CrashResumeInodePlan:
    fresh_lease_window: int
    restart_transcode_window: int
    output_tree_window: int
    retirement_prepared_window: int
    retirement_terminal_window: int
    terminal_compaction_window: int
    required_additional_inodes: int


@dataclass(frozen=True)
class UnifiedInodePlan:
    normal: NormalInodePlan
    crash_resume: CrashResumeInodePlan
    required_additional_inodes: int


def spill_run_peak(max_initial_runs: int) -> int:
    if max_initial_runs <= 0:
        raise InodePlanError("max initial runs must be positive")
    # Initial-run accumulation can hold max_initial_runs files. If more than
    # one run exists, the first merge output is created before its input files
    # are unlinked, creating one extra live spill-run inode transiently.
    return max_initial_runs + (1 if max_initial_runs > 1 else 0)


def normal_inode_plan(max_initial_runs: int) -> NormalInodePlan:
    run_peak = spill_run_peak(max_initial_runs)

    # Newly-created persistent/working files, above pre-existing inventory:
    #   fresh nonce generation                                  1
    #   sorted encrypted descriptor destination                 1
    #   live spill-run files                              run_peak
    sort_window = 1 + 1 + run_peak

    # Fresh nonce + sorted encrypted spill + durable restart copy.
    restart_copy_window = 3
    # Add authenticated restart manifest.
    restart_manifest_window = 4
    # Add source-set authority.
    source_authority_window = 5
    # Add retained encrypted descriptors while original spill/durable copy,
    # manifest and source authority are still present.
    restart_transcode_window = 6

    # After preflight/transcode the byte planner proves only two adjacent tree
    # working stages are simultaneously required (retained+locator,
    # locator+leaf-ref, or adjacent page-ref levels). Add the staged canonical
    # output plus fresh nonce, durable restart stage, manifest and source-set.
    output_tree_window = 1 + 1 + 1 + 1 + 1 + 2

    required = max(
        sort_window,
        restart_copy_window,
        restart_manifest_window,
        source_authority_window,
        restart_transcode_window,
        output_tree_window,
    )
    return NormalInodePlan(
        max_initial_runs=max_initial_runs,
        spill_run_peak=run_peak,
        sort_window=sort_window,
        restart_copy_window=restart_copy_window,
        restart_manifest_window=restart_manifest_window,
        source_authority_window=source_authority_window,
        restart_transcode_window=restart_transcode_window,
        output_tree_window=output_tree_window,
        required_additional_inodes=required,
    )


def crash_resume_inode_plan() -> CrashResumeInodePlan:
    # The crashed stage/manifest/source-set/original nonce record already
    # occupy inodes at restart entry and therefore are not additional-free-
    # inode demand. This plan counts new inodes above that inventory.
    fresh_lease_window = 1
    restart_transcode_window = 2  # fresh nonce + retained descriptor stage
    output_tree_window = 4  # fresh nonce + output + two adjacent tree stages
    retirement_prepared_window = 3  # fresh nonce + output + Prepared
    retirement_terminal_window = 4  # conservative: + Terminal before credits

    # A final/current checkpoint may be created before obsolete fresh nonce and
    # completed retirement records are pruned. Ignore inode credits from old
    # stage/manifest/source removal so the admission bound remains conservative.
    terminal_compaction_window = 5  # fresh nonce + output + Prepared + Terminal + checkpoint
    required = max(
        fresh_lease_window,
        restart_transcode_window,
        output_tree_window,
        retirement_prepared_window,
        retirement_terminal_window,
        terminal_compaction_window,
    )
    return CrashResumeInodePlan(
        fresh_lease_window=fresh_lease_window,
        restart_transcode_window=restart_transcode_window,
        output_tree_window=output_tree_window,
        retirement_prepared_window=retirement_prepared_window,
        retirement_terminal_window=retirement_terminal_window,
        terminal_compaction_window=terminal_compaction_window,
        required_additional_inodes=required,
    )


def unified_inode_plan(max_initial_runs: int) -> UnifiedInodePlan:
    normal = normal_inode_plan(max_initial_runs)
    crash = crash_resume_inode_plan()
    return UnifiedInodePlan(
        normal=normal,
        crash_resume=crash,
        required_additional_inodes=max(
            normal.required_additional_inodes,
            crash.required_additional_inodes,
        ),
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--max-initial-runs", type=int, required=True)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        plan = unified_inode_plan(args.max_initial_runs)
    except InodePlanError as exc:
        parser = argparse.ArgumentParser(prog="plan_phase3_private_inodes.py")
        parser.error(str(exc))
    payload = {
        "schema": "ucof-phase3-private-inode-plan-v1",
        "basis": "additional free inodes above inventory present at operation entry",
        "normal": asdict(plan.normal),
        "crash_resume": asdict(plan.crash_resume),
        "required_additional_inodes": plan.required_additional_inodes,
        "hard_link_publication_additional_inodes": 0,
        "non_claims": {
            "filesystem_inodes_reserved": False,
            "directory_entries_equivalent_to_inodes": False,
            "open_file_descriptors_equivalent_to_inodes": False,
            "concurrent_unrelated_inode_consumption_prevented": False,
        },
    }
    encoded = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    print(encoded, end="")
    if args.output:
        output = args.output if args.output.is_absolute() else Path.cwd() / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(encoded)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
