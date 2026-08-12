# Experiment 0027: Recursive Immutable Deletion and Root Collapse

- **Status:** Prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0023, 0025, and 0026
- **Script:** `tools/experiment_exp0002_immutable_page_recursive_delete.py`

## Question

Can immutable append-only pages propagate deletion underflow through multiple levels, merge or redistribute deterministic siblings, and collapse a level-two root without rewriting unrelated pages?

## Algorithm

Deletion descends by authenticated child ranges and emits one replacement leaf. A non-root child is underfull when it falls below:

- 93 entries for a 185-entry leaf;
- 128 children for a 255-child internal page.

The parent selects a deterministic adjacent sibling:

- the right sibling when available;
- otherwise the left sibling.

The two sibling contents are combined:

- if they fit in one page, emit one merged page;
- otherwise redistribute at the deterministic midpoint into two pages.

Underflow propagates to the parent. A root with one remaining child collapses to that child reference.

## Recursive boundary fixture

The fixture starts with 255 full leaves and one full level-one root. Inserting identifier `1` produces:

- two split leaves;
- two level-one internal pages;
- one level-two root.

Deleting identifier `1` then causes:

1. the split leaf pair to merge;
2. the first level-one page to become underfull;
3. the two level-one siblings to merge into one 255-child page;
4. the level-two root to collapse.

Expected active-tree delta relative to the inserted state:

- two new pages: merged leaf and merged internal root;
- 254 reused historical leaves;
- five retired inserted structural pages.

## Ordinary deep deletion

A second fixture deletes a non-boundary identifier from an underfull split leaf without triggering merge. The operation emits exactly one copied page per level: leaf, level-one parent, and level-two root.

## Findings

1. Underflow can be propagated using the same immutable child-reference mechanism as split propagation.
2. Deterministic sibling selection and midpoint redistribution are required byte semantics, not writer preferences.
3. A recursive split inverse needs only two new pages without historical-content lookup.
4. A content-indexed writer may reduce the same inverse to zero directory pages by reusing the historical root, as Experiment 0026 demonstrates.
5. Ordinary deep deletion returns to one copied page per level.

## Remaining work

- arbitrary mixed batched inserts, replacements, and deletes;
- deletion from trees deeper than the exercised level-two boundary;
- proof that occupancy policy is appropriate for append and inventory workloads;
- concurrent history retention and compaction semantics;
- complete snapshot publication and recovery vectors;
- fuzzing of arbitrary operation sequences and malformed sibling references;
- independent implementation.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_recursive_delete.py
```
