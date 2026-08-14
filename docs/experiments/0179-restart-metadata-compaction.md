# Experiment 0179 — Restart metadata compaction

**Status:** pending non-normative Phase 3 implementation evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0174–0178

## Purpose

Experiments 0174–0178 deliberately use append-only authenticated metadata for nonce authority, encrypted restart-stage manifests, retirement authority, and external source-set binding. That is conservative for crash recovery, but unbounded append-only growth cannot be a production contract.

Experiment 0179 adds a bounded reclamation mechanism without deleting history before replacement recovery authority exists and without deleting metadata still required by a live restart operation.

## Nonce compaction checkpoint

A compaction checkpoint is a new authenticated recovery base for the nonce journal. The current checkpoint record is 112 bytes:

- 80-byte canonical body;
- 32-byte HMAC-SHA256 tag under the existing restart-journal authentication key.

It binds:

- journal key identity;
- nonce prefix;
- durable generation;
- exact `next_unreserved` counter floor, including exhaustion.

Checkpoint creation uses create-new semantics, file flush/sync, descriptor-pinned journal-directory verification, and journal-directory sync before destructive pruning can begin.

A fully authenticated checkpoint observed after file sync but before directory sync is conservative authority: using it can only move the nonce floor forward. The compaction executor nevertheless performs no pruning until the checkpoint directory sync has succeeded.

## Compaction-aware recovery

`CompactedNonceJournal` leaves the existing pre-compaction journal semantics unchanged and adds a parallel post-checkpoint recovery path.

Recovery:

1. scans only the descriptor-pinned private journal directory under the existing bounded directory-entry cap;
2. authenticates and context-checks every checkpoint found;
3. authenticates and context-checks every surviving nonce generation record, including records older than the selected checkpoint;
4. selects the highest valid checkpoint as the recovery base;
5. requires every post-checkpoint journal generation to be contiguous and to begin at the checkpoint's exact nonce floor;
6. applies any caller-provided trusted generation/counter floor.

New nonce generations after compaction continue to use the existing ordinary generation-record format. The checkpoint is a recovery base, not a second lease format.

## Bounded cleanup inventory

Cleanup work is derived from actual bounded directory inventory. It never loops from generation `1..N`, because generation is authenticated state rather than an acceptable work bound.

The compactor authenticates/canonicalizes relevant metadata before selecting deletions:

- encrypted restart-stage manifests;
- Prepared and Terminal retirement records;
- source-set authority records;
- nonce-generation files;
- older compaction checkpoints.

## Live-authority preservation rule

A checkpoint replaces nonce **history**, not live restart evidence.

A nonce generation record is protected from pruning when its generation is still referenced by:

- a surviving authenticated encrypted restart-stage manifest; or
- a source-set authority whose crashed generation has not been terminally retired.

This is required because restart-stage verification still needs the original generation record to reconstruct and validate the encrypted spill's lease range even when a later/global checkpoint represents current nonce authority.

The compaction-aware encrypted restart classifier therefore uses:

- the checkpoint/post-checkpoint chain for current global nonce authority; and
- the preserved original generation record for exact restart-stage lease verification.

The source-bound encrypted-tree restart path has a compaction-aware continuation variant so a live restart remains usable after checkpointing.

## Retirement/source-set reclamation

Retirement metadata is reclaimed only when matching durable Terminal authority exists.

- Prepared without Terminal is preserved.
- Prepared + Terminal for the same crashed/fresh pair may be reclaimed.
- a source-set authority may be reclaimed only when its crashed generation is proven Terminal-retired;
- active/non-terminal source-set authority is preserved, together with its required nonce generation record.

A Terminal retirement that still has a live stage manifest is treated as contradictory rather than guessed away.

## Crash cuts

Executable regressions target these boundaries:

- checkpoint file synced before directory sync: old nonce prefix remains intact;
- checkpoint directory synced before prune: old records and checkpoint safely coexist;
- records pruned before final directory sync: authenticated checkpoint remains the recovery base;
- complete compaction: old eligible prefix is removed and future leases continue monotonically;
- repeated compaction: a newer checkpoint supersedes the older checkpoint after the new checkpoint becomes authoritative.

## Intended terminal-metadata evidence

A fully Terminal-retired restart pair should permit bounded reclamation of:

- obsolete nonce generation records not referenced by any other live authority;
- matching Prepared and Terminal retirement records;
- the obsolete source-set authority for that crashed generation.

An outstanding Prepared retirement should survive the first compaction, remain executable, and become reclaimable only after Terminal is durably created.

## Rollback boundary

HMAC integrity does not make the checkpoint non-rollbackable.

If an external actor deletes the latest valid checkpoint and all pruned prefix records are already gone, recovery without an external trusted floor can observe an older/initial state. The experiment therefore does not claim anti-rollback from local authenticated files alone.

A caller-provided trusted generation/counter floor is required to reject such external deletion/replay rollback. The regression suite explicitly distinguishes the no-floor rollback non-claim from trusted-floor rejection.

## What this is intended to close

If accepted, Experiment 0179 closes the unbounded append-only metadata-reclamation gap for the current research lifecycle mechanism:

- replacement nonce recovery authority exists before pruning;
- cleanup work is bounded by directory inventory;
- active restart/source/cleanup authority survives compaction;
- terminally obsolete metadata can be reclaimed;
- future nonce allocation remains monotonic from the checkpoint floor;
- crash cuts remain restart-classifiable without blind deletion retries.

## What remains open

Issue #11 remains open. This experiment does **not** establish:

- physical power-loss/filesystem qualification of the checkpoint/prune sync sequence;
- an externally non-rollbackable checkpoint/freshness anchor;
- a stronger mechanism closing the Linux same-UID final identity-check -> unlink race;
- a production compaction cadence/retention policy;
- forensic-retention policy for metadata that a deployment intentionally chooses not to reclaim;
- filesystem free-space reservation against unrelated concurrent writers;
- production key provisioning/rotation;
- confidentiality of clear sorter object IDs or spill geometry;
- production qualification outside the native Linux x86_64 crypto evidence;
- a stable or accepted EXP-0003 wire format.

Compacting away a Terminal record also means local historical replay can no longer answer "already terminal" from that deleted record alone. The checkpoint/reclamation mechanism preserves current nonce/restart safety, not an indefinite forensic history of all completed cleanup operations.

## Verification

Pending. This document must not be promoted to accepted evidence until the repository Rust/portability/integration gates are green on a pinned implementation head.

## Governance boundary

This remains private implementation/storage evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change immutable-successor public wire bytes, or make a compatibility promise.
