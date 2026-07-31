# Experiment 0053 — Persistent insertion and split propagation

**Status:** Reusable Rust writer evidence  
**Scope:** One absent object per complete append; no epoch allocation or multi-insertion batch claim

## Question

Can the reusable immutable-page successor writer insert one absent object by rewriting only one leaf-to-root path, including deterministic leaf and internal splits and root-height increase?

## Implementation

`append_persistent_insert` now:

1. strictly validates the exact-end active source;
2. rejects zero identifiers, zero kinds, duplicate identifiers, object-count overflow, and output/depth/page limits;
3. appends the new immutable object record;
4. routes the identifier to the first child whose maximum is at least the identifier, or the final child when it exceeds every maximum;
5. inserts into the canonical sorted leaf;
6. emits one leaf when capacity permits or two lower-median leaves on overflow;
7. replaces the selected child in its parent and applies the same lower-median split rule to internal overflow;
8. creates a new root when the prior root splits;
9. reuses every page outside the touched path byte-for-byte;
10. publishes one complete next-sequence snapshot and strictly revalidates the result.

A one-operation absent `Put` passed to `append_persistent_batch` is routed through this path and reports `CopyOnWriteInsertion`. Deletions and multi-operation batches containing insertions remain explicit full-rebuild fallbacks.

## Evidence

Rust integration tests cover:

- insertion into a numeric gap of a multi-leaf sparse tree;
- exact reuse of two unaffected pages while one leaf and the root are rewritten;
- a full root leaf splitting into two leaves plus a new level-one root;
- a full level-one tree whose leaf split overflows the internal root, producing two internal pages plus a new level-two root;
- deterministic replay and duplicate rejection;
- explicit fallback for deletion and multi-insertion batches.

A dedicated cargo-fuzz target varies leaf occupancy through full capacity, chooses below-range, in-gap, and above-range absent identifiers, compares the direct and general APIs, verifies deterministic replay, and strictly validates every emitted file.

## Findings

1. Persistent insertion and split propagation now exist in the reusable byte writer rather than only in standalone models.
2. Gap routing is deterministic and agrees with the earlier executable split prototype.
3. Root-height increase writes only the selected path, split siblings, and new root; unrelated subtrees remain exact historical page references.
4. Single insertion does not solve canonical multi-operation planning. Replaying sorted insertions one at a time would rewrite shared ancestors repeatedly and is not the final batch algorithm.
5. Deletion redistribution, merge, recursive underflow, and root collapse remain separate work.

## Remaining work

- canonical shared-path planner for multiple insertions and replacements;
- persistent deletion with deterministic borrow, merge, recursive underflow, and root collapse;
- cross-language byte recipes for leaf split, internal split, and root-height increase;
- interrupted-publication vectors for split commits;
- arbitrary-depth differential mixed-operation fuzzing;
- source-based and spill-backed output for production-scale commits.
