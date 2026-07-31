# Experiment 0060 — Shared persistent multi-Put writer

**Status:** active copy-on-write successor experiment  
**Date:** 2026-07-31

## Question

Can a canonical batch containing multiple insertions and replacements rewrite every affected leaf and shared ancestor once, rather than rebuilding the complete tree or applying order-dependent single-object updates?

## Planner

The writer first validates canonical occupancy, canonicalizes operations by object identifier, rejects duplicate operation identifiers, appends new object records in canonical order, and creates a sorted locator update set.

At each authenticated internal page, updates are routed by the same first-maximum rule used by single insertion. Unaffected child references are reused. Each affected child is visited once with its complete sorted update slice.

At a leaf, existing locators and updates are merged in one linear pass. Equal identifiers replace the prior locator; absent identifiers are inserted. Overflow is partitioned with the canonical final-two-page occupancy algorithm. Internal overflow uses the same grouping rule, and root overflow creates as many levels as required under configured depth and page limits.

## Evidence

Integration tests cover:

- two insertions in one leaf with one leaf and one root rewrite;
- insertions in different leaves sharing one root rewrite;
- insertion and replacement in one leaf;
- two insertions splitting one full leaf once;
- two full-leaf splits producing two internal pages and a new level-two root;
- caller-order determinism;
- duplicate operation rejection;
- routing equivalence through the general persistent-batch API;
- explicit full-rebuild fallback for a batch combining deletion with another operation.

## Accounting

`pages_written` counts newly appended leaf, internal, and root pages. `pages_reused` counts prior reachable pages not visited by the planner. The final current-tree page count therefore equals reused plus newly written pages, even when multiple splits increase tree height.

## Assurance boundary

This planner supports any non-empty batch containing only `Put` operations, including mixed insertion and replacement. Pure replacement batches retain the established replacement-only mode, and one absent `Put` retains the established single-insertion API and mode.

Batches containing a deletion plus another operation still use the deterministic canonical full rebuild. Coordinating deletion repair with simultaneous inserts and replacements remains a separate frontier because sibling choice and intermediate occupancy are byte-significant.
