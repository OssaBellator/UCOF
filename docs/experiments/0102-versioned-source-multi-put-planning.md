# Experiment 0102: versioned-source shared multi-Put planning

## Question

Can a canonical batch of insertions and replacements be planned directly from a bounded strongly versioned random-access source without retaining the complete base file, while preserving the owned shared writer's exact tail and reuse accounting?

## Construction

The planner:

1. pins one opaque strong non-ABA source version;
2. strictly validates the exact-end active snapshot and all active objects;
3. independently validates canonical occupancy and authenticates the root range;
4. hashes the complete source to obtain a length/SHA-256 base identity;
5. sorts inputs by object identifier and rejects duplicates or invalid identities;
6. appends replacement/insertion object records in canonical order;
7. authenticates every affected page;
8. merges each affected leaf once and counts identifiers absent from the original leaf;
9. shares rewritten ancestors across all updates;
10. canonically regroups overflowing leaves and internal pages and grows the root when required;
11. emits the same absolute-offset append tail as the owned shared multi-Put writer.

All strict, canonical, identity, and path reads share one cumulative request/byte budget and one version token. Only the append tail, sorted update locators, and affected path state are retained.

## Evidence

Deterministic Rust tests cover:

- two insertions routed to one leaf;
- cross-leaf updates;
- mixed insertions and replacements;
- caller-order independence;
- simultaneous splits and root growth;
- duplicate identifiers;
- source-version mutation;
- cumulative source-budget exhaustion.

The hostile fuzz target varies canonical base size, payloads, request sizes, caller order, mixed insertion/replacement batches, the full-root split boundary, duplicates, and version mutation. Successful plans must match `append_persistent_put_batch` tail bytes, report, page-write count, and reuse count exactly.

## Result

The experiment provides bounded-memory source-backed shared multi-`Put` planning for the current research layout. It does not claim minimal source traffic because strict active validation and the whole-file identity pass remain complete reads. Canonical mixed deletion-plus-other-operation source planning remains the final in-memory writer mode without a source-backed planner.
