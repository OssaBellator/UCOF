# Experiment 0179 remaining integration gaps

This note records unresolved integration boundaries discovered while auditing the restart-metadata compaction candidate. It is non-normative and does not change EXP-0003, FCP-0003, or any public immutable-successor bytes.

Experiment 0179 remains **pending**. None of the items below may be silently inherited as a production guarantee from the deterministic mechanism tests.

## 1. Post-checkpoint legacy allocation guard is executable

`CompactedNonceJournal` is the authority-aware allocation/recovery path once a compaction checkpoint exists. The older `LinuxDurableNonceJournal` scanner still intentionally understands only the original contiguous ordinary-generation journal, but its allocator now cross-checks that legacy view against `CompactedNonceJournal::scan` while holding the restart-metadata mutation lock and before reserving any nonce lease.

Legacy allocation is therefore permitted only while the two durable authority views agree. This preserves the immediate checkpoint crash window where the complete ordinary prefix still represents the checkpointed state, while failing closed once checkpoint-covered ordinary history has been reclaimed and only the authenticated checkpoint retains that authority. Regressions prove both cases and verify that the rejected path cannot recreate generation 1 after compaction.

This closes the deterministic fallback item for the candidate. It does not make the legacy scanner itself checkpoint-aware, and it does not replace the remaining platform-qualification or production entry-point work below.

## 2. Unknown metadata during destructive compaction

The lightweight compacted nonce recovery scan is deliberately tolerant of bounded unrelated directory entries. This lets nonce recovery ignore non-authoritative files while still enforcing the directory-entry work bound.

Quota accounting is stricter: `scan_compacted_persistent_inventory` rejects any unrecognized private metadata entry, even below the directory-count limit, because otherwise unknown bytes could disappear from private-storage arithmetic.

The destructive compactor now selects the same fail-closed policy. `scan_compaction_metadata` rejects any unrecognized entry before checkpoint creation, and `compaction_nonce_prune_inventory` rechecks the condition before returning any deletion inventory. Regressions require both pre-checkpoint and pre-prune rejection while preserving the unknown file and all ordinary nonce records.

The production compactor also holds the shared mutation lock described below, so cooperating writers cannot insert restart metadata between classification and pruning. The lightweight `CompactedNonceJournal` recovery scan remains tolerant because it does not delete unrelated entries. Protocol-ignoring same-UID mutation remains an explicit deployment boundary rather than a deterministic compaction guarantee.

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
