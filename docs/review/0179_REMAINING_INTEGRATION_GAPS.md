# Experiment 0179 remaining integration gaps

This note records unresolved integration boundaries discovered while auditing the restart-metadata compaction candidate. It is non-normative and does not change EXP-0003, FCP-0003, or any public immutable-successor bytes.

Experiment 0179 remains **pending**. None of the items below may be silently inherited as a production guarantee from the deterministic mechanism tests.

## 1. Post-checkpoint allocation must not fall back to the legacy journal view

`CompactedNonceJournal` is the authority-aware allocation/recovery path once a compaction checkpoint exists. The older `LinuxDurableNonceJournal` scanner intentionally understands only the original contiguous ordinary-generation journal.

That distinction is safe only if integration makes it impossible to choose the legacy allocator after ordinary history has been reclaimed. In particular, a checkpoint may summarize a durable generation/counter floor while the corresponding ordinary generation file has already been deleted. A legacy-only view can then observe less authority than the checkpointed state. A caller that allocates from that lower view could attempt to reuse a generation/counter range that compaction intentionally preserved only in the checkpoint.

Before 0179 can be treated as an integration-ready mechanism, one of these must be made explicit and executable:

- the legacy allocator fails closed whenever checkpointed state is not fully represented by the surviving contiguous ordinary journal; or
- the production integration statically/structurally makes `CompactedNonceJournal` the only allocation entry point once compaction is enabled, with a regression proving no post-checkpoint path can reach the legacy allocator.

Immediate checkpoint crash windows where the complete ordinary prefix is still present are different: the legacy journal can still reconstruct the same or stronger contiguous ordinary authority there. The unsafe class is fallback after checkpoint-covered ordinary history has been pruned.

## 2. Unknown metadata during destructive compaction

The lightweight compacted nonce recovery scan is deliberately tolerant of bounded unrelated directory entries. This lets nonce recovery ignore non-authoritative files while still enforcing the directory-entry work bound.

Quota accounting is stricter: `scan_compacted_persistent_inventory` now rejects any unrecognized private metadata entry, even below the directory-count limit, because otherwise unknown bytes could disappear from private-storage arithmetic.

This child selects the fail-closed policy for destructive compaction. `scan_compaction_metadata` rejects any unrecognized entry before checkpoint creation, and `compaction_nonce_prune_inventory` rechecks the same condition before returning any deletion inventory. Regressions require both pre-checkpoint and pre-prune rejection while preserving the unknown file and all ordinary nonce records.

The lightweight `CompactedNonceJournal` recovery scan remains deliberately tolerant of bounded unrelated entries because it does not delete them. This change therefore closes the static forward-compatibility boundary for destructive compaction without redefining recovery semantics.

This does **not** establish concurrent mutation safety: an entry created after the final prune-inventory scan remains part of the exclusive-mutation/synchronization gap below.

## 3. Compaction requires exclusive restart-metadata mutation or stronger synchronization

The compactor authenticates a bounded metadata graph, creates/synchronizes a replacement checkpoint, then reopens and re-authenticates each selected file immediately before unlink. Those checks narrow stale-file replacement mistakes, but the graph is still a snapshot.

A concurrent operation can create new metadata after graph classification. For example, a new live manifest could begin depending on an ordinary generation that the earlier snapshot considered reclaimable. The eventual result is designed to fail closed rather than reuse a nonce, but availability/restart continuity under arbitrary concurrent mutation is not established.

Production integration therefore needs either:

- an exclusive journal/restart-metadata mutation lock spanning compaction classification through final directory sync; or
- a stronger transactional/versioned protocol whose tests prove concurrent creators cannot acquire authority over metadata already selected for reclamation.

The same caveat applies to the new ordinary-record capacity preflight: it is a logical pre-write guard, not a filesystem reservation against another actor consuming an entry between the preflight and `create_new`.

## 4. Local acceptance requires an exclusive candidate checkout

`tools/verify_phase3_local.py --acceptance` pins a clean candidate SHA before expensive work and rechecks HEAD/worktree state before a successful report. `tools/record_phase3_local_acceptance.py` additionally requires the recorded candidate SHA, final report SHA, and current clean checkout SHA to agree, and hashes the exact report bytes.

Those checks prevent ordinary stale-report promotion. They do not cryptographically lock the checkout while every long-running command executes. The acceptance procedure therefore assumes the candidate checkout is not concurrently rewritten and restored during a command. This is an operational requirement for the person or system producing the local acceptance record.

## 5. Platform qualification remains external

Even after all deterministic integration gaps above are resolved and a full local acceptance report passes, issue #11 still requires external/platform evidence for:

- physical power-loss behavior of file `sync_all` + directory `sync_all` on each supported filesystem;
- supported local-filesystem and network-filesystem policy;
- production AES/HMAC key provisioning, storage, rotation, and failure handling;
- a non-rollbackable freshness anchor if deletion/replay rollback resistance is claimed;
- stronger same-UID isolation or an explicit deployment assumption for the final verification-to-unlink race;
- free-space/inode competition policy beyond arithmetic admission.

## Promotion rule

A complete local Rust acceptance report is necessary but not sufficient to erase these boundaries. Any 0179 promotion or issue-#11 closure record must either resolve the integration items above with executable evidence or carry them forward as explicit unsupported/externally-qualified assumptions.
