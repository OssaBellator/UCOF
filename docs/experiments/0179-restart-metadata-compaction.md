# Experiment 0179 — Restart metadata compaction

**Status:** pending non-normative Phase 3 implementation evidence  
**Date:** 2026-08-15  
**Tracking:** issue #11  
**Depends on:** Experiments 0174–0178

## Purpose

Experiments 0174–0178 deliberately use append-only authenticated metadata for nonce authority, encrypted restart-stage manifests, retirement authority, and external source-set binding. That is conservative for crash recovery, but unbounded append-only growth cannot be a production contract.

Experiment 0179 adds a bounded reclamation mechanism without deleting history before replacement recovery authority exists and without deleting metadata still required by a live encrypted restart stage or unfinished cleanup authority.

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

1. scans only the descriptor-pinned private journal directory under the bounded directory-entry policy;
2. authenticates and context-checks every checkpoint and surviving ordinary nonce record;
3. requires an authenticated ordinary record's embedded generation to equal the generation encoded by its canonical filename;
4. rejects an authenticated checkpoint chain whose generation order or nonce floor moves backward;
5. checks **every authenticated checkpoint**, not only the newest one, against every surviving ordinary record at or below that checkpoint generation;
6. rejects any checkpoint below a surviving historical nonce floor and requires exact counter agreement when checkpoint and record have the same generation;
7. selects the highest consistent checkpoint as the global recovery base;
8. requires every post-checkpoint ordinary generation to be contiguous and to begin at the exact previous nonce floor;
9. applies any caller-provided trusted generation/counter floor.

The every-checkpoint rule prevents a newer valid checkpoint from masking a contradiction in an older still-present checkpoint. Dedicated regressions cover both an older checkpoint below a surviving earlier record and an older same-generation checkpoint whose counter disagrees with that generation's surviving record.

The filename/embedded-generation check matters even with a valid HMAC: replaying an authenticated generation-1 record under the generation-2 filename is rejected rather than being silently interpreted through pathname state.

The existing `max_journal_bytes` bound continues to constrain ordinary 128-byte nonce records. Checkpoint bytes are separately bounded and included in authenticated-byte diagnostics/private-storage accounting, so a file-synced checkpoint can coexist with an ordinary journal already at its byte ceiling during a crash/retry window.

New nonce reservations after compaction continue to use the accepted ordinary generation-record format. The checkpoint replaces history; it is not a new lease format.

## Directory-entry headroom

Checkpoint creation necessarily creates one new directory entry before obsolete metadata can be pruned. Treating the configured directory-entry ceiling as an absolute scan ceiling would therefore strand an otherwise valid retry when the directory was exactly full before checkpoint creation.

The compaction-aware path permits exactly one transient scan entry above `max_directory_entries` **only when an authenticated checkpoint is present and the over-cap directory contains no unrecognized entry**. An unrelated filename cannot borrow the checkpoint's transient slot merely because some valid checkpoint also exists.

The ordinary compacted nonce commit path reserves the next checkpoint slot before writing a new generation record. If the directory is already at the configured entry ceiling, a new nonce lease is rejected before a generation file is created. After compaction restores headroom, allocation can continue.

The corresponding regression cycle uses a two-entry limit: two ordinary generations fill the directory; another compacted commit is rejected; checkpoint creation temporarily reaches three entries; pruning returns the directory to one checkpoint; one ordinary generation can then be committed; the next commit is again rejected until compaction replaces history.

This is a bounded crash/retry allowance, not a general exemption from directory limits. A checkpoint plus an unrelated file at `max+1` is rejected by both recovery and compacted persistent-inventory accounting.

## Exhausted nonce authority

`next_unreserved: None` is the canonical exhausted nonce floor, not an absent value. The monotonic ordering therefore treats finite `Some(counter) → None` as forward progress, `None → None` as equal exhausted authority, and `None → Some(counter)` as rollback.

The wired Rust regressions create a real exhausted journal without allocating payload memory: one generation reserves counters `0..=u64::MAX-1`, the next reserves the final counter `u64::MAX`, and the durable state becomes `None`. Compaction must round-trip that state through a checkpoint, reclaim the ordinary records, and reject any future nonce reservation without creating another generation file. A later finite checkpoint after an exhausted checkpoint is rejected, and an exhausted trusted floor rejects finite current authority.

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
- source-set stage identity disagreement with its retirement lineage after the manifest is gone;
- a live stage manifest that does not match the preserved original nonce record generation/key/prefix/operation context;
- authenticated retirement or source-set metadata from a foreign journal key/prefix context;
- lifecycle metadata that claims generations ahead of current global nonce authority.

Cleanup work is derived from actual bounded directory inventory. It never loops from generation `1..N`; authenticated generation numbers are state, not acceptable work bounds.

## Exact nonce-record retention rule

A checkpoint replaces obsolete nonce history, not live stage-verification evidence.

An ordinary nonce generation record is retained **only while a live encrypted stage manifest still needs that original record to reconstruct and authenticate the stage lease**. A source-set record does not independently require the ordinary nonce file; it is tied either to the live manifest or to durable cleanup authority. An outstanding Prepared retirement likewise does not require the fresh publication generation's ordinary nonce file once the checkpoint has durably captured the global nonce floor.

This gives a smaller and more precise retention rule:

- live crashed stage manifest → preserve its crashed-generation ordinary nonce record;
- fresh publication generation with no live stage of its own → checkpoint is sufficient global nonce-history authority;
- Prepared cleanup with the old stage still live → preserve only the crashed-stage record, not the fresh publication record;
- Terminal cleanup after the stage manifest is gone → no ordinary nonce record is protected by that lineage.

A dedicated regression compacts the fresh generation-2 ordinary nonce record after durable Prepared(1→2), then executes the existing retirement protocol to Terminal successfully using the checkpoint for global nonce authority and the preserved generation-1 record only for stage verification.

## Restart across burned and pruned generations

The compacted restart path deliberately separates:

- **historical stage verification authority:** the preserved crashed-generation ordinary record; and
- **new allocation authority:** the current checkpoint/post-checkpoint global nonce state.

The hard lifecycle regression targets:

`stage gen1 → checkpoint1 → commit/burn gen2 → checkpoint2/prune gen2 → retry gen1 → allocate/publish gen3 → Prepared(1→3) → checkpoint3/prune gen3 → Terminal → reclaim gen1 + cleanup metadata`

The old stage is authenticated with generation 1, but new allocation starts strictly after the checkpoint-2 counter floor. Generation 2 is never reissued even though its ordinary file has already been reclaimed.

The compacted path is carried through canonical output staging and durable publication, not just in-memory continuation. `prepare_compacted_encrypted_restart_retirement` then uses checkpoint-aware current nonce authority while retaining the accepted Prepared/Terminal cleanup format and executor.

Restart preparation performs a second context check on the exact historical nonce record returned by the post-classification reopen before it commits a fresh lease. That ensures a metadata replacement between initial classification and the value actually used for transcoding is rejected before allocation rather than needlessly burning a fresh lease.

## Retirement and reclamation

Prepared retirement authority is preserved until matching Terminal authority exists. Once a pair is terminally retired, the matching Prepared/Terminal records, obsolete source-set record, and ordinary nonce records no longer required by a live stage become reclaimable.

Destructive reclamation is ordered by dependency after the current checkpoint is already durable and directory-synced:

1. eligible ordinary nonce history;
2. source-set authority for Terminal-retired crashed generations;
3. matching Prepared retirement records;
4. matching Terminal retirement records;
5. older superseded checkpoints.

The order is deliberate. Terminal retirement remains the last local completion authority for a reclaimed pair. A crash after source-set pruning still leaves both retirement records; a crash after Prepared pruning leaves Terminal. A retry can therefore finish compaction without being stranded in an orphan source-set or Prepared-only completed-lineage state. Dedicated Rust cuts exercise both prefixes.

Before each selected unlink, the compactor reopens the canonical file through the pinned directory, re-authenticates it, and compares the current record with the authenticated inventory snapshot. Only then does it unlink that pathname.

This narrows stale/replacement mistakes but does **not** eliminate the documented Linux same-UID final verification-to-unlink race; that remains an explicit production-qualification gap.

## Private-storage quota continuity

Checkpointing does not make persistent metadata free.

`CompactedPersistentInventory` authenticates and charges surviving ordinary nonce records, compaction checkpoints, retirement records, and source-set records. The base encrypted crash-resume plan already charges the durable encrypted stage, manifest, retained descriptor overlap, encrypted tree staging, output staging, and retirement overlap; checkpointed inventory is added on top without double-counting those stage/manifest bytes.

Even though manifest bytes are charged by the base crash-resume plan rather than added again to `CompactedPersistentInventory.total_bytes`, compacted quota inventory still loads, authenticates, and context-checks any canonical stage manifest it encounters. An authenticated manifest carrying a foreign key/prefix context is rejected rather than being accepted merely because the filename has manifest shape.

The compaction admission plan charges existing authenticated persistent metadata plus one 112-byte checkpoint when the exact current checkpoint does not already exist. A one-byte-short compaction metadata cap must fail before checkpoint creation or pruning. The exact computed cap may proceed.

Both compacted continuation **and durable publication** use the same pre-side-effect private-storage enforcement. The publication regression requires a one-byte-short cap to leave the publication backend untouched, create no private staged output, and mint no fresh nonce generation; the exact computed cap may publish durably.

## Crash and adversarial evidence

The wired Rust regression set now covers:

- checkpoint file sync before directory sync;
- retry from that cut through a fresh pinned-directory sync before pruning;
- checkpoint directory sync before pruning;
- terminal source-set pruning before retirement pruning, with retry from that cut;
- Prepared retirement pruning before Terminal retirement pruning, with Terminal retained as retry authority;
- pruning before final directory sync;
- checkpoint coexistence with an ordinary journal exactly at the legacy journal-byte ceiling;
- exactly one authenticated checkpoint directory-entry transient at the configured entry ceiling;
- rejection when an unrelated entry attempts to borrow that checkpoint headroom;
- pre-write compacted nonce admission that reserves the next checkpoint entry;
- repeated checkpoint replacement and future nonce monotonicity;
- authenticated checkpoint counter rollback;
- older checkpoint contradictions that cannot be masked by a newer valid checkpoint;
- exhausted nonce authority checkpoint round-trip, finite rollback rejection, future-commit rejection, and exhausted trusted-floor enforcement;
- authenticated nonce-record replay under the wrong generation filename;
- authenticated foreign journal context for retirement/source-set records;
- authenticated foreign stage-manifest context in compacted quota inventory;
- orphan and mismatched source-set authority;
- source-set/retirement identity lineage checks;
- live-manifest/original-nonce context checks;
- competing retirement generations;
- mismatched Prepared/Terminal payload authority;
- Terminal/live-manifest contradiction;
- live source-bound restart after checkpointing;
- retry after an intermediate generation is both burned and compacted away;
- a `DestinationExists` publication burn that is checkpointed/pruned before a later successful retry;
- compacted canonical publication, Prepared/Terminal retirement, and final terminal-lineage reclamation;
- trusted-floor rejection versus the explicit no-floor rollback non-claim;
- exact-cap and one-byte-short compaction, continuation, and durable-publication admission;
- authenticated byte-accounting of ordinary records versus checkpoint overhead;
- a repeated 32-generation Rust commit/compact campaign.

## Independent model evidence

`tools/verify_restart_metadata_compaction_model.py` is a standard-library-only state model that does not call or parse the Rust implementation. It independently models nonce generations, checkpoints, crash cuts, graph validity, live-stage nonce-record retention, preservation/reclamation, trusted floors, quota admission, retry across burned/pruned generations, every-checkpoint/surviving-history consistency, and bounded checkpoint directory headroom.

The current repository model declares:

```text
12 fixed crash/rollback/graph/quota/full-lifecycle/headroom cases
64 small-state matrix cases
```

A focused semantic rerun in the current development session also exercised:

```text
1024 randomized campaigns × 256 transitions = 262144 transitions
```

plus the tightened checkpoint/unknown-entry headroom cases, masked historical-checkpoint cases, and the `None` exhaustion ordering semantics. Result: **PASS**.

The fixed cases include both dependency-order cuts: source-set removed while Prepared+Terminal remain, and Prepared removed while Terminal remains. Both retry to a fully reclaimed terminal lineage. The full-lifecycle model also covers generation 1 live stage authority, generation 2 burn + compaction, generation 3 retry/publication authority, compaction of the fresh generation after Prepared, Terminal transition, and final reclamation to a single generation-3 checkpoint.

The independent model currently uses finite Python counters for its main transition campaign; the exhausted `None` authority boundary is additionally covered by the wired Rust regressions and the focused ordering cross-check described above.

The local verification runner itself has dedicated Python self-tests and refuses ambiguous or incomplete acceptance modes. Its acceptance candidate is pinned to a clean Git SHA before expensive checks and is rechecked for HEAD/worktree changes before the report can be accepted.

This independent evidence is useful but is **not** a substitute for compiling and executing the newly wired Rust implementation.

## Local acceptance gate

The repository replacement for the former workflow gate is:

```text
python3 tools/verify_phase3_local.py --acceptance
```

Use `--offline` when the required Cargo dependencies/toolchains are already available locally and network access should be forbidden.

A complete `--acceptance` run requires a clean resolvable Git HEAD before expensive work begins. It performs the Phase 3 wiring guard, explicit checkpoint-history consistency, directory-headroom and destructive-order static checks, the independent 0179 model, Python tool self-tests, Cargo metadata/fmt/Clippy/tests, workspace tests/docs, HTTP/S3 adapter tests, policy/vector checks, Rust 1.85 checks, i686 and powerpc64 checks, and a 256-run smoke pass over every locally installed fuzz target. The script never installs missing tools; acceptance fails if the required MSRV/targets/nightly/cargo-fuzz environment is unavailable.

The destructive-order check requires `nonce -> terminal source-set -> Prepared retirement -> Terminal retirement -> old checkpoint` in the compaction executor. Checkpoint-history and directory-headroom guards likewise appear as separate report checks rather than being implicit in generic wiring.

Every run writes `target/phase3-local-verification.json`, including exact Git SHA/branch, tool versions, dirty-worktree state, commands, elapsed times, skips, and final result.

A successful report can then be normalized into repository evidence with:

```text
python3 tools/record_phase3_local_acceptance.py
```

The recorder refuses stale-SHA, dirty, skipped, partial, model-only, missing-fuzz, or failed reports and requires the explicit 0179 checkpoint-history, directory-headroom, destructive-order, wiring, independent-model, and Python-self-test results before writing a SHA-bound record under `docs/verification/phase3-local-acceptance-<sha>.json`.

Experiment 0179 must remain **pending** until a report from the exact candidate head has:

- `mode: "acceptance"`;
- `ok: true`;
- no skipped checks;
- a clean candidate SHA that remained unchanged throughout the run;
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

The implementation also has a purely defensive extreme-limit cleanup item: the helper that computes one-entry scan headroom currently uses checked `max_directory_entries + 1`, so configuring `max_directory_entries == usize::MAX` returns a ceiling-overflow error instead of saturating. Ordinary production limits are far below this value, but the edge should be normalized before 0179 is considered fully polished.

Compacting a Terminal record intentionally gives up indefinite local forensic replay of that completed cleanup event. The mechanism preserves current nonce/restart safety; it is not an audit-log retention system or forensic secure deletion mechanism.
