# Experiment 0048 — Reusable Mixed-Batch Byte-Writer Baseline

**Status:** Implemented executable baseline  
**Date:** 2026-07-31  
**Scope:** Immutable-page successor research microformat; no epoch allocation or compatibility promise

## Question

Can the reusable Rust successor writer express one complete append containing mixed insertion, replacement, and deletion operations with deterministic bytes and strict active-state validation before the arbitrary-depth copy-on-write planner is integrated?

## Implementation

`ucof_experiments::immutable_successor::append_batch` accepts a strictly valid exact-end source and a batch of `ImmutableBatchOperation` values:

- `Put` inserts a new object or replaces the active object with the same identifier;
- `Delete` removes an active object and fails if the identifier is not active.

The writer:

1. strictly validates the current active source;
2. sorts operations by object identifier;
3. rejects duplicate operation identifiers rather than assigning caller-order semantics;
4. preflights invalid inputs, missing deletions, empty results, object-count limits, allocation limits, output limits, and sequence overflow;
5. appends replacement and insertion object records in canonical identifier order;
6. rebuilds the active directory tree at arbitrary depth;
7. publishes one complete next-sequence snapshot and exact-end footer;
8. strictly validates the resulting bytes before returning them.

Caller operation order therefore does not affect the output bytes.

## Executable evidence

The integration tests cover a 400-object, level-one tree with one batch that:

- replaces object 200;
- inserts object 401;
- deletes objects 2 and 399;
- retains 399 active objects;
- produces a strictly valid sequence-1 state with three leaves and one internal root;
- reproduces identical bytes when the caller reverses operation order;
- verifies inserted, replaced, retained, and deleted state through strict selected-object rewrite.

Negative cases cover:

- an empty operation batch;
- duplicate operation identifiers;
- deletion of a missing identifier;
- an invalid zero object kind;
- deletion of every active object;
- insertion beyond the configured object-count limit.

## Result

The reusable byte writer now has deterministic mixed-operation semantics and can emit a complete arbitrary-depth active tree. This removes the earlier single-replacement API restriction and provides a byte-level integration target for the existing mixed-batch models.

## Important limitation

This experiment is not the final persistent-tree writer. It rebuilds every active directory page in the appended commit and therefore does not preserve unchanged page identities. It does not close the Phase 3 blocker requiring integration of arbitrary-depth mixed batching with copy-on-write page reuse, split, redistribution, merge, and recursive underflow behavior.

The next implementation step is to replace the full-tree rebuild behind the same deterministic batch semantics with a path-copy planner while retaining strict publication and validation boundaries.
