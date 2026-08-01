# Experiment 0094: Unified persistent streaming dispatch

## Question

Can one public streaming entry point select the same persistent mode as the owned general batch API without cloning payload buffers or weakening fail-before-output behavior?

## Construction

`append_persistent_batch_to`:

1. rejects empty batches and strictly validates the exact-end canonical base;
2. canonicalizes operation identifiers and rejects duplicates before output;
3. routes one existing `Put` to replacement-tail output;
4. routes one absent `Put` to insertion-tail output;
5. routes multiple `Put` operations containing an insertion to the shared multi-`Put` tail planner using borrowed input references;
6. routes replacement-only multi-`Put` operations to replacement-tail output;
7. routes one `Delete` to deletion-tail output;
8. routes deletion combined with another operation to canonical mixed-tail output;
9. returns the specialized mode, page-write/reuse accounting, base/tail bytes, and bounded-write report unchanged.

All mode-specific writers retain their own exact validation and construction preflight. The dispatcher therefore performs conservative classification validation before invoking a specialized writer; no sink bytes are exposed during either validation pass.

## Evidence

Rust tests compare the dispatcher with `append_persistent_batch` for every current mode:

- replacement-only;
- single insertion;
- shared insertion/replacement `Put` batch;
- single deletion;
- canonical mixed deletion-plus-insertion.

The tests also verify caller-order independence, exact bytes, mode, report, page writes/reuse, bounded sink requests, and duplicate or empty batches failing before output.

The `immutable_successor_persistent_streaming_dispatch` fuzz target varies base shape, payloads, operation mode, target identifier, caller order, and sink chunk size. Every accepted case must match the owned general batch API byte-for-byte and pass canonical occupancy validation. Duplicate identifiers must leave the sink untouched.

## Boundary

This unifies current in-memory-slice dispatch only. It deliberately repeats canonical validation for conservative classification and specialized preflight. It does not provide bounded-source base reading, atomic private staging, durable publication, or proposed-epoch migration. Sink failure after output begins remains terminal.
