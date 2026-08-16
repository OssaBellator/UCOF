# Experiment 0179 remaining integration gaps

This note records unresolved integration boundaries discovered while auditing the restart-metadata compaction candidate. It is non-normative and does not change EXP-0003, FCP-0003, or any public immutable-successor bytes.

Experiment 0179 remains **pending**. None of the items below may be silently inherited as a production guarantee from the deterministic mechanism tests.

## 1. Post-checkpoint legacy allocation guard is executable

`CompactedNonceJournal` is the authority-aware allocation/recovery path once a compaction checkpoint exists. The older `LinuxDurableNonceJournal` scanner still intentionally understands only the original contiguous ordinary-generation journal, but its allocator now cross-checks that legacy view against `CompactedNonceJournal::scan` before reserving any nonce lease.

Legacy allocation is therefore permitted only while the two durable authority views agree. This preserves the immediate checkpoint crash window where the complete ordinary prefix still represents the checkpointed state, while failing closed once checkpoint-covered ordinary history has been reclaimed and only the authenticated checkpoint retains that authority. A regression proves both cases and verifies that the rejected path cannot recreate generation 1 after compaction.

This closes the deterministic fallback item for the candidate. It does not make the legacy scanner itself checkpoint-aware, and it does not replace the remaining synchronization, platform-qualification, or production entry-point work below.

## 2. Unknown metadata during destructive compaction

The lightweight compacted nonce recovery scan is deliberately tolerant of bounded unrelated directory entries. This lets nonce recovery ignore non-authoritative files while still enforcing the directory-entry work bound.

Quota accounting is stricter: `scan_compacted_persistent_inventory` now rejects any unrecognized private metadata entry, even below the directory-count limit, because otherwise unknown bytes could disappear from private-storage arithmetic.

Destructive compaction is not yet equally strict for every below-cap unknown entry. An older compactor therefore needs an explicit schema/forward-compatibility contract before it can safely coexist with future authenticated metadata families. Production integration should choose one of two policies:

- fail closed before checkpoint creation/pruning when the journal directory contains any unrecognized metadata family; or
- define a versioned registry and retention rule proving unknown/newer metadata can never depend on history selected for deletion by the older compactor.

Until that policy exists, unknown future metadata plus destructive compaction is an open compatibility boundary.

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
