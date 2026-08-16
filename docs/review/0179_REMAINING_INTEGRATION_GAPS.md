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

Destructive compaction is not yet equally strict for every below-cap unknown entry. An older compactor therefore needs an explicit schema/forward-compatibility contract before it can safely coexist with future authenticated metadata families. Production integration should choose one of two policies:

- fail closed before checkpoint creation/pruning when the journal directory contains any unrecognized metadata family; or
- define a versioned registry and retention rule proving unknown/newer metadata can never depend on history selected for deletion by the older compactor.

Until that policy exists, unknown future metadata plus destructive compaction is an open compatibility boundary.

## 3. Restart-metadata mutation serialization is executable

The candidate now acquires one non-blocking exclusive advisory lock on an independently opened descriptor for the same pinned restart-metadata directory before any durable authority mutation. Ordinary and compacted nonce allocation, durable stage-manifest publication, source-set authority publication, retirement preparation/execution, and compaction all participate in the same lock. Compaction holds it from the first recovery/classification scan through checkpoint creation, pruning, and final directory sync.

The lock is tied to the directory inode rather than a lockfile: acquisition re-verifies device/inode identity against the pinned descriptor, contention fails closed before mutation, and descriptor close/process exit releases the advisory lock without stale cleanup state. The low-level nonce and retirement persistence/prune helpers require a guard parameter, so their production callers cannot bypass the lock accidentally.

A regression opens independent handles to the same directory, holds one mutation guard, and proves both a nonce commit and compaction make no side effects while contended; after guard release both succeed. This closes the candidate's deterministic concurrent-writer gap for cooperating writers using these entry points.

The lock is advisory. A rogue or same-UID process that ignores the protocol remains outside this mechanism and belongs to the explicit deployment/same-UID isolation assumptions in platform qualification.

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
