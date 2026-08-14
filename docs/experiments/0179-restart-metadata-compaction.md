# Experiment 0179 — Restart metadata compaction

**Status:** pending non-normative Phase 3 implementation evidence  
**Date:** 2026-08-15  
**Tracking:** issue #11  
**Depends on:** Experiments 0174–0178

## Purpose

Experiments 0174–0178 deliberately use append-only authenticated metadata for nonce authority, encrypted restart-stage manifests, retirement authority, and external source-set binding. That is conservative for crash recovery, but unbounded append-only growth cannot be a production contract.

Experiment 0179 adds a bounded reclamation mechanism without deleting history before replacement recovery authority exists and without deleting metadata still required by a live restart or unfinished retirement.

This is implementation/storage evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change public immutable-successor bytes, or create a compatibility promise.

## Important verification correction

The first #141 drafts contained 0179 source files that were **not included by** `bounded_end_to_end_candidate.rs`. Consequently, earlier green repository workflow results for that branch did not compile or execute Experiment 0179 and are not acceptance evidence for this experiment.

The branch now explicitly wires every 0179 implementation and regression file into the native Linux x86_64 experiment test module. `tools/verify_phase3_local.py` contains a static wiring guard so this class of false-green cannot recur silently.

GitHub Actions is no longer the acceptance mechanism for this work. A future accepted 0179 record must instead pin a clean local verification report generated from the exact accepted Git SHA.

## Nonce compaction checkpoint

A compaction checkpoint is a new authenticated recovery base for the nonce journal. The current record is 112 bytes:

- 80-byte canonical body;
- 32-byte HMAC-SHA256 tag under the existing restart-journal authentication key.

It binds journal key identity, nonce prefix, durable generation, and exact `next_unreserved` counter authority including exhaustion.

Checkpoint creation uses create-new semantics, file flush/sync, descriptor-pinned journal-directory verification, and journal-directory sync before destructive pruning can begin.

If a retry observes an exact checkpoint left by a crash after checkpoint file sync but before directory sync, pathname visibility is not treated as proof that the prior directory sync completed. The retry re-verifies the pinned directory and performs directory `sync_all` before that checkpoint can authorize pruning.

## Compaction-aware recovery

`CompactedNonceJournal` leaves the accepted pre-compaction journal format unchanged and adds a parallel checkpoint-aware recovery path.

Recovery:

1. scans only the descriptor-pinned private journal directory under the existing directory-entry bound;
2. authenticates and context-checks every checkpoint and surviving ordinary nonce record;
3. rejects an authenticated checkpoint chain whose nonce floor moves backward;
4. selects the highest valid checkpoint as the global recovery base;
5. checks any surviving historical record at/below the checkpoint does not exceed the checkpoint nonce floor and requires exact agreement when the same generation exists in both forms;
6. requires every post-checkpoint ordinary generation to be contiguous and to begin at the exact previous nonce floor;
7. applies any caller-provided trusted generation/counter floor.

New nonce reservations after compaction continue to use the accepted ordinary generation-record format. The checkpoint replaces history; it is not a new lease format.

## Authenticated metadata graph

Compaction first authenticates and canonicalizes the restart metadata graph before creating a checkpoint or selecting destructive work.

The graph includes:

- encrypted restart-stage manifests;
- Prepared and Terminal retirement records;
- source-set authority records;
- ordinary nonce-generation records;
- compaction checkpoints.

It fails closed on authenticated contradictions, including:

- more than one fresh retirement generation for one crashed generation;
- Prepared and Terminal records for one pair whose protected payload/publication facts disagree;
- Terminal retirement coexisting with a still-live stage manifest;
- source-set authority that belongs to neither a live manifest nor an active/terminal cleanup lineage;
- source-set operation/stage identity/object count disagreement with its live manifest;
- lifecycle metadata that claims generations ahead of current global nonce authority.

Cleanup work is derived from actual bounded directory inventory. It never loops from generation `1..N`; authenticated generation numbers are state, not acceptable work bounds.

## Live-authority preservation

A checkpoint replaces obsolete nonce history, not live restart evidence.

The original nonce generation record is preserved when referenced by a live encrypted stage manifest or by non-terminal source authority. An outstanding Prepared retirement conservatively protects both its crashed and fresh nonce generations until matching Terminal authority exists.

This matters because encrypted-stage authentication still needs the original generation record to reconstruct and validate the old spill lease even when a later checkpoint or burned generation represents current global nonce authority.

The compacted restart path therefore deliberately separates:

- **historical stage verification authority:** the preserved crashed-generation record; and
- **new allocation authority:** the current checkpoint/post-checkpoint global nonce state.

A crash-retry regression targets:

`stage generation 1 -> checkpoint -> commit/burn generation 2 -> retry generation-1 stage -> allocate generation 3`

The old stage is never re-encrypted under reused counters and the burned generation-2 range is not reissued.

## Retirement and reclamation

Prepared retirement authority is preserved until matching Terminal authority exists. Once a pair is terminally retired, the matching Prepared/Terminal records, obsolete source-set record, and nonce generations no longer protected by another live lineage become reclaimable.

Before each selected unlink, the compactor reopens the canonical file through the pinned directory, re-authenticates it, and compares the current record with the authenticated inventory snapshot. Only then does it unlink that pathname.

This narrows stale/replacement mistakes but does **not** eliminate the documented Linux same-UID final verification-to-unlink race; that remains an explicit production-qualification gap.

## Private-storage quota continuity

Checkpointing does not make persistent metadata free.

`CompactedPersistentInventory` authenticates and charges surviving ordinary nonce records, compaction checkpoints, retirement records, and source-set records. The compaction admission plan charges existing authenticated persistent metadata plus one 112-byte checkpoint when the exact current checkpoint does not already exist.

A one-byte-short compaction metadata cap must fail before checkpoint creation or pruning. The exact computed cap may proceed.

Post-compaction source-bound restart adds exact checkpointed persistent bytes to the accepted encrypted-tree crash-resume lifecycle plan. A one-byte-short restart cap therefore still fails before a fresh nonce generation or private output side effect.

## Crash and adversarial evidence

The wired Rust regression set covers:

- checkpoint file sync before directory sync;
- retry from that cut through a fresh pinned-directory sync before pruning;
- checkpoint directory sync before pruning;
- pruning before final directory sync;
- repeated checkpoint replacement and future nonce monotonicity;
- authenticated checkpoint counter rollback;
- orphan and mismatched source-set authority;
- competing retirement generations;
- mismatched Prepared/Terminal payload authority;
- Terminal/live-manifest contradiction;
- live source-bound restart after checkpointing;
- retry after a burned intermediate generation;
- trusted-floor rejection versus the explicit no-floor rollback non-claim;
- exact-cap and one-byte-short private-storage admission;
- a repeated 32-generation Rust commit/compact campaign.

## Independent model evidence

`tools/verify_restart_metadata_compaction_model.py` is a standard-library-only state model that does not call or parse the Rust implementation. It independently models nonce generations, checkpoints, crash cuts, graph validity, preservation/reclamation, trusted floors, quota admission, and retry over burned generations.

On 2026-08-15 the model was executed locally in the development environment with:

```text
python3 tools/verify_restart_metadata_compaction_model.py --campaigns 256 --steps 192
```

Result:

```text
restart metadata compaction independent model: PASS
fixed_cases=8
matrix_cases=64
campaigns=256
campaign_steps=192
campaign_transitions=49152
```

The local verification runner itself was also syntax-checked and exercised in `--model-only` mode against a synthetic correctly-wired checkout. That runner self-test passed.

This evidence is useful but is **not** a substitute for compiling and executing the newly wired Rust implementation.

## Local acceptance gate

The repository replacement for the former workflow gate is:

```text
python3 tools/verify_phase3_local.py --acceptance
```

Use `--offline` when the required Cargo dependencies/toolchains are already available locally and network access should be forbidden.

A complete `--acceptance` run performs the Phase 3 wiring guard and independent 0179 model, Cargo metadata/fmt/Clippy/tests, workspace tests/docs, HTTP/S3 adapter tests, policy/vector checks, Rust 1.85 checks, i686 and powerpc64 checks, and a 256-run smoke pass over every locally installed fuzz target. The script never installs missing tools; acceptance fails if the required MSRV/targets/nightly/cargo-fuzz environment is unavailable.

Every run writes `target/phase3-local-verification.json`, including exact Git SHA/branch, tool versions, dirty-worktree state, commands, elapsed times, skips, and final result.

Experiment 0179 must remain **pending** until a report from the exact candidate head has:

- `mode: "acceptance"`;
- `ok: true`;
- no skipped checks;
- successful wired Rust tests/Clippy;
- successful MSRV/portability/fuzz checks.

A model-only report, a report with `--skip-fuzz`, or any historical GitHub Actions result is insufficient for acceptance.

## Rollback boundary

HMAC integrity does not make a local checkpoint non-rollbackable.

If an external actor deletes the latest valid checkpoint after its obsolete prefix was pruned, recovery without an external trusted floor can observe an older or initial state. The experiment therefore does not claim local anti-rollback from authenticated files alone.

A caller-provided non-rollbackable trusted generation/counter floor is required to reject that deletion/replay class.

## What remains open

Even after a complete local acceptance report, issue #11 still requires production qualification beyond this deterministic mechanism evidence:

- physical power-loss/filesystem qualification of the file/directory sync sequence;
- an explicit supported local-filesystem matrix and network-filesystem policy;
- a non-rollbackable external freshness anchor if rollback resistance is claimed;
- production AES/HMAC key provisioning, storage, rotation, and failure policy;
- a stronger mechanism or explicit same-UID isolation assumption for the final verification-to-unlink race;
- free-space/inode competition policy beyond arithmetic admission;
- production compaction cadence and forensic-retention policy;
- qualification outside the native Linux x86_64 crypto evidence;
- confidentiality policy for clear sorter object IDs/spill geometry;
- a stable or accepted EXP-0003 wire format.

Compacting a Terminal record intentionally gives up indefinite local forensic replay of that completed cleanup event. The mechanism preserves current nonce/restart safety; it is not an audit-log retention system or forensic secure deletion mechanism.
