# Experiment 0058 — Persistent deletion writer

**Status:** active copy-on-write successor experiment  
**Date:** 2026-07-31

## Question

Can one active object be deleted from a canonically occupied immutable page tree while rewriting only the affected path and the minimum sibling set required for deterministic repair?

## Preconditions

- The input passes complete exact-end validation and canonical occupancy validation.
- Every non-root leaf and internal page is at least half full, rounded up.
- The active snapshot contains more than one object.

Legacy sparse-tail research trees are rejected rather than silently reinterpreted.

## Deterministic repair order

After deletion:

1. retain the page when it remains at or above minimum;
2. borrow one entry from the left sibling when it can remain at minimum;
3. otherwise borrow one entry from the right sibling under the same condition;
4. otherwise merge with the left sibling when present;
5. otherwise merge with the right sibling;
6. apply the same rule recursively to internal pages;
7. collapse an internal root with one child.

Only final repaired pages are appended. Unaffected page references are reused byte-for-byte.

## Evidence

Integration tests cover:

- root-leaf deletion;
- deletion without underflow and reuse of unrelated pages;
- left borrow;
- right borrow from the leftmost page;
- merge and root collapse;
- recursive level-two internal borrow after a leaf merge;
- deterministic replay;
- missing identifier and final-object rejection;
- equivalence between the direct deletion API and one-operation persistent batches.

## Accounting

`pages_written` counts newly appended repaired pages. `pages_reused` counts reachable pages from the previous tree that remain referenced by the new root. Root collapse can therefore write fewer pages than the number of original pages replaced.

## Assurance boundary

This supports one deletion per complete append. Multi-operation shape-changing batches still use the deterministic full rebuild until a shared planner can coordinate inserts, deletes, replacements, sibling repair, and shared ancestors without order-dependent intermediate trees.
