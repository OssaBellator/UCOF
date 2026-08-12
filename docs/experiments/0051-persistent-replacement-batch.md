# Experiment 0051 — Persistent arbitrary-depth replacement batches

**Status:** Reusable Rust writer evidence  
**Scope:** Immutable-page successor research; no epoch allocation or compatibility promise

## Question

Can the reusable byte writer preserve exact historical page identities for multi-object replacements without materializing or rebuilding the entire active tree?

## Implementation

`append_persistent_batch` now has two explicit modes:

- `CopyOnWriteReplacements`: every operation replaces an existing identifier. New object records are appended, only page ranges containing replacements are traversed, affected leaves are rewritten, and changed ancestors are emitted once.
- `FullRebuildShapeChange`: an insertion or deletion changes tree shape, so the existing deterministic mixed-batch rebuild is used without claiming page reuse.

The copy-on-write traversal checks authenticated page references, routes replacements by canonical page ranges, reuses untouched `PageRef` values byte-for-byte, publishes only newly written pages in the commit page count, and strictly revalidates the final exact-end file.

## Evidence

Rust integration tests cover:

- one changed leaf in a three-leaf level-1 tree: two pages written and two reused;
- two changed leaves sharing one rewritten root: three pages written and one reused;
- a level-2 tree above `LEAF_CAPACITY * INTERNAL_FANOUT`: three pages written along one leaf-to-root path;
- deterministic equality under reversed caller operation order;
- explicit full-rebuild reporting for insertion/deletion batches.

A dedicated cargo-fuzz target generates bounded replacement and insertion batches, checks exact-end validation, verifies mode reporting, and compares canonical output across caller order.

## Findings

1. Immutable content-addressed pages support exact reuse in the reusable Rust byte writer, not only in standalone models.
2. Replacement work is proportional to changed paths and tree depth rather than total page count.
3. Multiple changed leaves share rewritten ancestors deterministically.
4. Shape-changing updates still require integration of split, redistribution, merge, root-height change, and recursive underflow algorithms.
5. Falling back to a full rebuild is acceptable as explicit evidence but cannot satisfy the final large-update efficiency objective.

## Remaining work

- integrate insertion split propagation;
- integrate deletion redistribution, merge, recursive underflow, and root collapse;
- batch planner scheduling that avoids rewriting the same path repeatedly;
- source-based writing without whole-file materialization;
- hostile arbitrary-depth operation fuzzing with a differential logical model;
- page-write and spill budgets for production-scale commits.
