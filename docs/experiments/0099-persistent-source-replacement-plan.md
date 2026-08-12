# Experiment 0099: Source-backed replacement tail planning

## Question

Can a replacement-only persistent append tail be planned directly from one strongly versioned bounded random-access source without retaining the complete base file, while preserving exact in-memory writer bytes and canonical preconditions?

## Construction

`plan_persistent_replacement_tail_at` accepts a `PersistentVersionedReadAt` source, replacement-only batch operations, and cumulative source/format limits. The planner:

1. canonicalizes operations by object identifier and rejects duplicates, deletes, empty batches, and invalid object identities;
2. captures one strong non-ABA source version and brackets every length and range operation;
3. strictly validates the exact-end active snapshot and all active object records;
4. independently traverses the authenticated page tree to enforce canonical leaf and internal occupancy;
5. hashes the complete source to return a pinned length/SHA-256 base identity;
6. appends replacement object records to an owned absolute-offset tail in canonical operation order;
7. rereads only tree paths whose ranges contain replacement identifiers;
8. preserves untouched child references byte-for-byte;
9. tracks every matched identifier and rejects mixed existing/missing batches;
10. emits the same page, snapshot, footer, report, write, and reuse accounting as the in-memory persistent replacement writer;
11. accumulates strict, occupancy, identity, and path-read statistics under one operation-wide read budget.

## Evidence

Rust tests cover:

- exact tail/report/page-accounting equality with the owned writer across first and last leaves;
- caller-order independence;
- mixed existing and missing identifier rejection;
- source-version change rejection;
- cumulative read-budget exhaustion.

The `immutable_successor_persistent_source_replacement` fuzz target varies canonical base sizes across root-leaf and internal-tree shapes, payload bytes and lengths, replacement order, read/hash chunk sizes, missing identifiers, and source-version changes. Successful plans must equal the owned writer's append tail byte-for-byte.

## Boundary

This closes bounded-memory replacement tail construction only. Strict validation and current-commit authentication still read all active data, and the separate whole-file identity pass reads the complete file; the experiment does not claim minimal source traffic. Insertion, deletion, shared multi-`Put`, and canonical mixed source-backed planners remain open. Provider adapters and staged publication composition remain separate layers.
