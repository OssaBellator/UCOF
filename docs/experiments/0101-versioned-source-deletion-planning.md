# Experiment 0101: versioned-source deletion planning

## Question

Can deterministic persistent deletion repair be planned directly from a bounded strongly versioned random-access source without retaining the complete base file, while preserving the owned writer's exact tail and reuse accounting?

## Construction

The planner:

1. pins one opaque strong non-ABA source version;
2. strictly validates the exact-end active snapshot and all active objects;
3. independently validates canonical occupancy and authenticates the root range;
4. hashes the complete source to obtain a length/SHA-256 base identity;
5. authenticates the deletion path and any inspected sibling pages;
6. deletes the selected locator;
7. repairs underflow deterministically by borrowing left, borrowing right, merging left, or merging right;
8. propagates internal underflow recursively and collapses a one-child root;
9. emits the same absolute-offset append tail as the owned deletion writer.

Only modified original pages reduce reuse accounting. Source statistics still include non-borrowable sibling pages inspected during deterministic repair selection. All strict, canonical, identity, target, and sibling reads share one cumulative budget and one version token.

## Evidence

Deterministic Rust tests cover:

- root-leaf deletion;
- a multi-page deletion without underflow;
- left borrowing;
- merge and root collapse;
- missing and final-object rejection;
- source-version mutation;
- cumulative source-budget exhaustion.

The hostile fuzz target varies canonical tree sizes, selected identifiers, request sizes, and repair geometries including stable paths, borrowing, and merge/root collapse. Successful plans must match the owned writer's tail bytes, report, page-write count, and reuse count exactly.

## Result

The experiment provides bounded-memory source-backed single deletion for the current research layout. It does not claim minimal source traffic because strict active validation and the whole-file identity pass remain complete reads, and sibling inspection may read pages that remain reusable. Shared multi-`Put` and canonical mixed source-backed planning remain separate frontiers.
