#!/usr/bin/env python3
"""Emit the destructive Phase 3 power-loss qualification campaign.

This tool does not trigger crashes or power cuts. It defines the exact external
campaign cases that must be exercised on a target filesystem/storage stack if
physical durability is claimed.
"""

from __future__ import annotations

from dataclasses import dataclass, asdict
import json

SCHEMA = "ucof-phase3-powerloss-campaign-plan-v1"


@dataclass(frozen=True)
class Case:
    case_id: str
    subsystem: str
    cut: str
    precondition: str
    durable_operations_completed: tuple[str, ...]
    deliberately_not_completed: tuple[str, ...]
    reboot_observation: str
    required_retry: str
    safety_invariant: str


CASES: tuple[Case, ...] = (
    Case(
        "checkpoint-file-fsync-before-dir-fsync",
        "0179-metadata-compaction",
        "after checkpoint file fsync, before journal directory fsync",
        "authenticated pre-compaction nonce authority and prune candidates exist",
        ("checkpoint bytes written", "checkpoint file fsync"),
        ("journal directory fsync", "any destructive prune"),
        "checkpoint may be absent or present; if present it must authenticate exactly; old authority must remain sufficient",
        "if checkpoint is present and exact, re-verify/re-fsync pinned journal directory before any prune; otherwise retry checkpoint creation without reusing nonce authority",
        "no prune is authorized solely by a file-fsynced checkpoint",
    ),
    Case(
        "checkpoint-dir-fsync-before-prune",
        "0179-metadata-compaction",
        "after checkpoint directory fsync, before first prune",
        "new current checkpoint is file- and directory-durable",
        ("checkpoint file fsync", "journal directory fsync"),
        ("nonce/source/retirement/checkpoint pruning",),
        "new checkpoint must be recoverable as current nonce authority; all old metadata may still be present",
        "resume dependency-safe pruning from authenticated inventory",
        "replacement nonce authority exists before any historical metadata is removed",
    ),
    Case(
        "compaction-after-nonce-prune-before-source-prune",
        "0179-metadata-compaction",
        "after eligible ordinary nonce pruning, before terminal source-set pruning",
        "current checkpoint is durable and live-stage nonce generations were preserved",
        ("current checkpoint durability", "eligible ordinary nonce unlink operations"),
        ("terminal source-set prune", "retirement prune", "final directory fsync"),
        "current checkpoint plus preserved live-stage nonce records must classify global/restart authority",
        "resume source/retirement/checkpoint prune order",
        "removing obsolete ordinary nonce history cannot make a live restart unverifiable",
    ),
    Case(
        "compaction-after-source-prune-before-prepared-prune",
        "0179-metadata-compaction",
        "after terminal source-set unlink, before Prepared retirement unlink",
        "matching Terminal retirement authority remains durable",
        ("current checkpoint durability", "eligible nonce prune", "terminal source-set unlink"),
        ("Prepared unlink", "Terminal unlink", "old checkpoint unlink", "final directory fsync"),
        "Terminal authority must still explain the completed cleanup lineage even though source-set authority is absent",
        "resume by pruning Prepared before Terminal",
        "a crash prefix never removes the only completion authority before its dependents",
    ),
    Case(
        "compaction-after-prepared-prune-before-terminal-prune",
        "0179-metadata-compaction",
        "after Prepared unlink, before Terminal unlink",
        "matching Terminal retirement authority remains durable",
        ("current checkpoint durability", "source-set prune", "Prepared unlink"),
        ("Terminal unlink", "old checkpoint unlink", "final directory fsync"),
        "Terminal authority must remain visible/authenticated",
        "resume Terminal then old-checkpoint pruning",
        "Terminal is the last retirement record removed for a completed pair",
    ),
    Case(
        "compaction-after-all-prune-before-final-dir-fsync",
        "0179-metadata-compaction",
        "after all selected unlinks, before final journal directory fsync",
        "current checkpoint was durable before pruning",
        ("current checkpoint durability", "selected metadata unlinks"),
        ("final journal directory fsync",),
        "storage image may show any prefix of selected unlinks, but current checkpoint must still classify nonce authority",
        "re-inventory/re-authenticate remaining metadata, finish dependency-safe prune, fsync journal directory",
        "partial prune persistence cannot roll nonce authority backward or authorize nonce reuse",
    ),
    Case(
        "publication-stage-file-fsync-before-private-dir-fsync",
        "durable-publication",
        "after staged output file fsync, before private staging directory fsync",
        "canonical private output bytes have been written",
        ("staged output file fsync",),
        ("private directory fsync", "publication hard link", "publication directory fsync"),
        "staged name may be absent or present; final destination must not be newly durable from this cut",
        "classify exact staged identity if present; otherwise rebuild from durable restart authority",
        "file fsync alone does not claim durable staged-name publication authority",
    ),
    Case(
        "publication-private-dir-fsync-before-link",
        "durable-publication",
        "after private staging directory fsync, before final no-overwrite hard link",
        "staged output name/bytes are durable in private directory",
        ("staged file fsync", "private directory fsync"),
        ("final hard link", "publication directory fsync"),
        "durable staged output must remain classifiable; final destination remains absent unless it pre-existed",
        "retry no-overwrite publication from the exact staged inode",
        "durable private staging does not imply public publication",
    ),
    Case(
        "publication-link-before-public-dir-fsync",
        "durable-publication",
        "after final no-overwrite hard link, before publication directory fsync",
        "exact staged inode has been linked at the destination name",
        ("staged durability", "no-overwrite hard link syscall"),
        ("publication directory fsync",),
        "destination may be absent or present after reboot; if present it must identify the expected staged inode/content",
        "classify destination/staged identities and complete directory fsync or report indeterminate outcome without overwriting",
        "pre-directory-sync link visibility is not treated as unquestionably durable publication",
    ),
    Case(
        "publication-public-dir-fsync-before-retirement",
        "durable-publication",
        "after publication directory fsync, before Prepared retirement authority",
        "final destination is PublishedAndDurable",
        ("staged durability", "hard link", "publication directory fsync"),
        ("Prepared retirement persistence", "private cleanup"),
        "destination must survive reboot with exact content/identity; private restart state may remain",
        "prepare cleanup authority, then retire private state",
        "destructive retirement never precedes durable public publication",
    ),
    Case(
        "retirement-prepared-before-first-unlink",
        "restart-retirement",
        "after Prepared retirement record and its directory durability, before first target unlink",
        "public output is durable and Prepared binds both cleanup target identities",
        ("Prepared file fsync", "Prepared journal directory fsync"),
        ("stage unlink", "manifest unlink", "cleanup directory fsync", "Terminal persistence"),
        "Prepared and both cleanup targets should be recoverable unless storage violates prior durability",
        "reclassify both targets before performing any unlink",
        "no cleanup begins without durable Prepared authority",
    ),
    Case(
        "retirement-stage-unlink-before-manifest-unlink",
        "restart-retirement",
        "after private stage unlink, before manifest unlink/directory sync",
        "Prepared is durable and both targets were classified before first unlink",
        ("Prepared durability", "stage unlink syscall"),
        ("manifest unlink", "private/journal directory fsync", "Terminal persistence"),
        "stage may be absent or present; manifest may be present; Prepared must remain",
        "reclassify exact/absent targets, finish cleanup, sync directories, persist Terminal",
        "an already-absent first target is a retryable cleanup state, not authority loss",
    ),
    Case(
        "retirement-both-unlinks-before-directory-fsync",
        "restart-retirement",
        "after both cleanup unlink syscalls, before cleanup directory fsyncs",
        "Prepared is durable",
        ("Prepared durability", "both unlink syscalls"),
        ("private directory fsync", "journal directory fsync", "Terminal persistence"),
        "each target may independently reappear or remain absent after reboot; Prepared must remain",
        "reclassify both, repeat idempotent cleanup as needed, fsync both directories, then Terminal",
        "Terminal is not written until cleanup-name removal is directory-durable",
    ),
    Case(
        "retirement-directory-fsync-before-terminal",
        "restart-retirement",
        "after both cleanup directories are fsynced, before Terminal retirement persistence",
        "Prepared is durable and cleanup target removals are directory-durable",
        ("Prepared durability", "target cleanup", "private directory fsync", "journal directory fsync"),
        ("Terminal persistence",),
        "both cleanup targets must remain absent; Prepared remains sufficient completion authority",
        "persist matching Terminal record",
        "a crash before Terminal cannot resurrect retired private targets if prior directory durability holds",
    ),
)


def build_plan() -> dict:
    ids = [case.case_id for case in CASES]
    if len(ids) != len(set(ids)):
        raise RuntimeError("duplicate power-loss campaign case id")
    return {
        "schema": SCHEMA,
        "destructive_external_execution_required": True,
        "cases": [asdict(case) for case in CASES],
        "required_platform_metadata": [
            "kernel_version",
            "filesystem_type",
            "filesystem_version_if_available",
            "mount_options",
            "block_device_or_volume_type",
            "storage_controller_or_virtualization_layer",
            "write_cache_policy_if_known",
            "host_or_cloud_provider",
            "test_image_or_snapshot_identifier",
            "ucof_git_sha",
        ],
        "non_claims": [
            "process-crash-only evidence is sufficient",
            "results transfer to another filesystem/mount/storage stack",
            "network/distributed filesystems inherit local-filesystem results",
        ],
    }


def main() -> int:
    print(json.dumps(build_plan(), indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
