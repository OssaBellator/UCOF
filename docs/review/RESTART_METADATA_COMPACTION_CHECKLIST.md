# Restart metadata compaction review checklist

This checklist is non-normative implementation review support for issue #11 / Experiment 0179. It does not alter FCP-0003 or EXP-0003.

## Recovery-base invariants

- [ ] A new authenticated nonce checkpoint is completely written and file-synced before it can be observed as a candidate recovery base.
- [ ] No destructive pruning begins before the descriptor-pinned journal directory containing that checkpoint has been synchronized.
- [ ] The checkpoint binds key identity, nonce prefix, durable generation, and exact `next_unreserved` counter floor.
- [ ] Recovery selects the highest valid checkpoint and accepts only a contiguous post-checkpoint nonce-generation suffix beginning at the exact checkpoint counter floor.
- [ ] Every surviving nonce record is still authenticated/context-checked even when older than the selected checkpoint.
- [ ] A caller trusted floor rejects deletion/replay rollback below the externally remembered generation/counter authority.

## Bounded-work invariants

- [ ] Discovery and pruning work is bounded by actual directory inventory, not by looping over the numeric generation range.
- [ ] Directory-entry and authenticated-byte limits fail closed before unbounded diagnostic/cleanup work.
- [ ] Repeated compaction does not accumulate an unbounded chain of obsolete checkpoints.

## Live-authority preservation

- [ ] A live encrypted restart-stage manifest protects the nonce generation record needed to verify that stage's original lease range.
- [ ] A non-terminal source-set authority protects its crashed generation's nonce record.
- [ ] Prepared retirement authority survives until matching Terminal authority is durable.
- [ ] A Terminal record coexisting with a live stage manifest is treated as contradictory rather than silently pruned.
- [ ] A source-bound restart remains executable after checkpointing when its live generation record is preserved.

## Reclamation invariants

- [ ] Prepared + Terminal records for one completed pair become reclaimable only after Terminal authority exists.
- [ ] Source-set authority is reclaimed only for a crashed generation proven terminally retired.
- [ ] An obsolete nonce generation is reclaimed only when no live manifest/source authority requires it.
- [ ] A newer checkpoint becomes authoritative before an older checkpoint is removed.
- [ ] Crash after pruning but before final directory sync is conservatively recoverable from the already-authoritative checkpoint.

## Explicit non-claims

- [ ] HMAC integrity is not described as local anti-rollback against deletion/replay of older valid checkpoints.
- [ ] `sync_all` ordering evidence is not described as physical power-loss qualification without filesystem/platform testing.
- [ ] The existing Linux same-UID final identity-check -> unlink race remains documented unless a stronger primitive is introduced.
- [ ] Metadata compaction is not described as forensic secure deletion.
- [ ] A compaction mechanism is kept separate from a production cadence/retention policy.
- [ ] Deleting historical Terminal records is documented as loss of indefinite local cleanup-history evidence, not loss of current nonce/restart safety.
