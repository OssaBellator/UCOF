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
- [ ] A same-generation checkpoint and surviving nonce record must agree on the exact counter floor.
- [ ] A caller trusted floor rejects deletion/replay rollback below the externally remembered generation/counter authority.

## Authenticated metadata graph

- [ ] More than one fresh retirement generation for one crashed generation fails closed before checkpoint creation.
- [ ] Prepared and Terminal records for one pair must agree on stage, manifest, output length, and output digest authority.
- [ ] Terminal retirement coexisting with a live stage manifest fails closed.
- [ ] A source-set record must belong to a matching live manifest or an active/terminal cleanup lineage.
- [ ] A live source-set record must match manifest operation, stage identity, role, and object count.
- [ ] Manifest/source/retirement generations may not claim state ahead of current global nonce authority.

## Bounded-work invariants

- [ ] Discovery and pruning work is bounded by actual directory inventory, not by looping over the numeric generation range.
- [ ] Directory-entry and authenticated-byte limits fail closed before unbounded diagnostic/cleanup work.
- [ ] Repeated compaction does not accumulate an unbounded chain of obsolete checkpoints.
- [ ] Private-storage admission charges the additional 112-byte checkpoint before checkpoint creation when one is not already present.
- [ ] One-byte-short compaction/restart caps fail before persistent/private side effects.

## Live-authority preservation

- [ ] A live encrypted restart-stage manifest protects the original nonce generation record needed to verify that stage's lease range.
- [ ] A non-terminal source-set authority protects its crashed generation's nonce record.
- [ ] Prepared retirement preserves both crashed and fresh nonce generations until matching Terminal authority is durable.
- [ ] Prepared retirement authority itself survives until matching Terminal authority is durable.
- [ ] A source-bound restart remains executable after checkpointing when its live generation record is preserved.
- [ ] A generation-1 stage remains retryable after generation 2 is durably burned, and the retry allocates generation 3 rather than reusing generation 2.

## Reclamation invariants

- [ ] Prepared + Terminal records for one completed pair become reclaimable only after Terminal authority exists.
- [ ] Source-set authority is reclaimed only for a crashed generation proven terminally retired.
- [ ] An obsolete nonce generation is reclaimed only when no live restart/cleanup lineage protects it.
- [ ] A newer checkpoint becomes authoritative before an older checkpoint is removed.
- [ ] Every selected nonce/checkpoint/retirement/source-set file is reopened, re-authenticated, and compared with the inventory snapshot immediately before unlink.
- [ ] Crash after pruning but before final directory sync is conservatively recoverable from the already-authoritative checkpoint.

## Local verification gate

- [ ] `tools/verify_phase3_local.py --acceptance` runs from the exact candidate Git SHA.
- [ ] The generated `target/phase3-local-verification.json` has `mode: "acceptance"` and `ok: true`.
- [ ] The report contains no skipped checks and the worktree state is recorded.
- [ ] The static wiring guard confirms every 0179 implementation/regression file is in the Rust compilation graph.
- [ ] The independent 0179 model passes its fixed cases, state matrix, and randomized campaigns.
- [ ] Rust fmt, Clippy with `-D warnings`, wired Rust tests, workspace/doc tests, Rust 1.85, i686, powerpc64, HTTP/S3/policy/vector gates, and fuzz smoke all pass locally.
- [ ] No historical GitHub Actions run is used as 0179 acceptance evidence.

## Explicit non-claims

- [ ] HMAC integrity is not described as local anti-rollback against deletion/replay of older valid checkpoints.
- [ ] `sync_all` ordering evidence is not described as physical power-loss qualification without filesystem/platform testing.
- [ ] The existing Linux same-UID final identity-check -> unlink race remains documented unless a stronger primitive is introduced.
- [ ] Metadata compaction is not described as forensic secure deletion.
- [ ] A compaction mechanism is kept separate from a production cadence/retention policy.
- [ ] Deleting historical Terminal records is documented as loss of indefinite local cleanup-history evidence, not loss of current nonce/restart safety.
