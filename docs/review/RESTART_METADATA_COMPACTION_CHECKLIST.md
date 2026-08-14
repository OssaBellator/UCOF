# Restart metadata compaction review checklist

This checklist is non-normative implementation review support for issue #11 / Experiment 0179. It does not alter FCP-0003 or EXP-0003.

The boxes remain unchecked until the **wired Rust implementation** passes a complete pinned local acceptance report. Presence of code or a passing independent model is not sufficient to mark a row accepted.

## Recovery-base invariants

- [ ] A new authenticated nonce checkpoint is completely written and file-synced before it can be observed as a candidate recovery base.
- [ ] No destructive pruning begins before the descriptor-pinned journal directory containing that checkpoint has been synchronized.
- [ ] A retry that finds a matching checkpoint left after file sync re-verifies and re-syncs the pinned directory before pruning.
- [ ] The checkpoint binds key identity, nonce prefix, durable generation, and exact `next_unreserved` counter floor.
- [ ] Authenticated checkpoint floors are monotonic; a newer valid-HMAC checkpoint cannot move the nonce floor backward.
- [ ] Recovery selects the highest valid checkpoint and accepts only a contiguous post-checkpoint nonce-generation suffix beginning at the exact checkpoint counter floor.
- [ ] Every surviving nonce record is still authenticated/context-checked even when older than the selected checkpoint.
- [ ] A nonce record replayed under the wrong generation filename fails closed even when its HMAC is valid.
- [ ] A same-generation checkpoint and surviving nonce record must agree on the exact counter floor.
- [ ] Checkpoint bytes do not consume the legacy ordinary-journal byte cap during a checkpoint+full-journal crash window; they remain bounded/accounted separately.
- [ ] A caller trusted floor rejects deletion/replay rollback below the externally remembered generation/counter authority.

## Authenticated metadata graph

- [ ] More than one fresh retirement generation for one crashed generation fails closed before checkpoint creation.
- [ ] Prepared and Terminal records for one pair must agree on stage, manifest, output length, and output digest authority.
- [ ] Terminal retirement coexisting with a live stage manifest fails closed.
- [ ] A source-set record must belong to a matching live manifest or an active/terminal cleanup lineage.
- [ ] A live source-set record must match manifest operation, stage identity, role, and object count.
- [ ] After the live manifest is gone, surviving source-set stage identity must match its retirement lineage.
- [ ] A live stage manifest must match the original ordinary nonce record generation/key/prefix/operation context.
- [ ] Authenticated retirement/source-set records from a foreign journal key/prefix context fail closed before checkpoint creation.
- [ ] Manifest/source/retirement generations may not claim state ahead of current global nonce authority.

## Bounded-work invariants

- [ ] Discovery and pruning work is bounded by actual directory inventory, not by looping over the numeric generation range.
- [ ] Directory-entry and ordinary-journal authenticated-byte limits fail closed before unbounded diagnostic/cleanup work.
- [ ] Repeated compaction does not accumulate an unbounded chain of obsolete checkpoints.
- [ ] Private-storage admission charges the additional 112-byte checkpoint before checkpoint creation when one is not already present.
- [ ] One-byte-short compaction, continuation, and durable-publication caps fail before persistent/private side effects.

## Live-authority preservation

- [ ] A live encrypted restart-stage manifest protects the original ordinary nonce generation record needed to verify that stage's lease range.
- [ ] Source-set authority is tied to a matching live manifest or durable cleanup lineage but does not independently force retention of an ordinary nonce file.
- [ ] Prepared retirement authority survives until matching Terminal authority is durable.
- [ ] Prepared retirement does **not** unnecessarily retain the fresh publication generation's ordinary nonce record; the checkpoint carries global nonce history.
- [ ] Prepared cleanup still reaches Terminal after that fresh ordinary nonce record has been compacted away.
- [ ] A source-bound restart remains executable after checkpointing when its live crashed-generation record is preserved.
- [ ] A generation-1 stage remains retryable after generation 2 is durably burned **and compacted away**, and the retry allocates generation 3 rather than reusing generation 2.
- [ ] The exact historical nonce record reopened for restart preparation is revalidated against the manifest before a fresh lease is committed.

## Publication and retirement continuity

- [ ] A compacted source-bound restart can stage canonical output and reach `PublishedAndDurable` after an intermediate burned generation was pruned.
- [ ] Compacted-aware Prepared retirement uses current checkpointed nonce authority without changing the accepted retirement record format.
- [ ] Existing Terminal retirement execution works after the fresh publication generation ordinary nonce record is no longer present.
- [ ] The full `gen1 -> burn/prune gen2 -> publish gen3 -> Prepared -> Terminal -> final compaction` path preserves canonical output and ends with only the current checkpoint when no live stage remains.

## Reclamation invariants

- [ ] Prepared + Terminal records for one completed pair become reclaimable only after Terminal authority exists.
- [ ] Source-set authority is reclaimed only for a crashed generation proven terminally retired.
- [ ] An ordinary nonce generation is reclaimed once no live encrypted stage requires it for exact lease verification.
- [ ] Destructive ordering remains `eligible nonce history -> terminal source-set -> Prepared retirement -> Terminal retirement -> old checkpoint` after the current checkpoint is durable.
- [ ] A crash after terminal source-set pruning leaves Prepared/Terminal authority intact and a compaction retry finishes reclamation.
- [ ] A crash after Prepared retirement pruning leaves Terminal authority intact and a compaction retry finishes reclamation.
- [ ] Terminal retirement is the last completed-lineage authority removed, so no compaction crash prefix can strand a Prepared-only completed cleanup state.
- [ ] A newer checkpoint becomes authoritative before an older checkpoint is removed.
- [ ] Every selected nonce/checkpoint/retirement/source-set file is reopened, re-authenticated, and compared with the inventory snapshot immediately before unlink.
- [ ] Crash after pruning but before final directory sync is conservatively recoverable from the already-authoritative checkpoint.

## Local verification gate

- [ ] `tools/verify_phase3_local.py --acceptance` runs from the exact candidate Git SHA and refuses a dirty worktree before expensive checks.
- [ ] The generated `target/phase3-local-verification.json` has `mode: "acceptance"` and `ok: true`.
- [ ] The report contains no skipped checks and records tool versions/worktree state.
- [ ] The static wiring guard confirms every 0179 implementation/regression file is in the Rust compilation graph.
- [ ] The static destructive-order guard records `nonce -> terminal source-set -> Prepared -> Terminal -> old checkpoint` and the acceptance recorder requires that check.
- [ ] The independent 0179 model passes fixed graph/crash/quota/full-lifecycle cases, the state matrix, and randomized campaigns.
- [ ] Rust fmt, Clippy with `-D warnings`, wired Rust tests, workspace/doc tests, Rust 1.85, i686, powerpc64, HTTP/S3/policy/vector gates, and fuzz smoke all pass locally.
- [ ] `tools/record_phase3_local_acceptance.py` accepts the report from the same clean SHA and writes a SHA-bound record under `docs/verification/`.
- [ ] No historical GitHub Actions run is used as 0179 acceptance evidence.

## Explicit non-claims

- [ ] HMAC integrity is not described as local anti-rollback against deletion/replay of older valid checkpoints.
- [ ] `sync_all` ordering evidence is not described as physical power-loss qualification without filesystem/platform testing.
- [ ] The existing Linux same-UID final identity-check -> unlink race remains documented unless a stronger primitive is introduced.
- [ ] Metadata compaction is not described as forensic secure deletion.
- [ ] Arithmetic private-storage admission is not described as filesystem free-space or inode reservation.
- [ ] A compaction mechanism is kept separate from a production cadence/retention policy.
- [ ] Deleting historical Terminal records is documented as loss of indefinite local cleanup-history evidence, not loss of current nonce/restart safety.