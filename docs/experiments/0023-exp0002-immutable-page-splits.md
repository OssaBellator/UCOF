# Experiment 0023: Immutable-Page Splits, Merges, and Root Height

- **Status:** Prototype
- **Date:** 2026-07-30
- **Related:** FCP-0002, Experiments 0014–0016
- **Script:** `tools/experiment_exp0002_immutable_page_splits.py`

## Question

Can immutable content-addressed directory pages support deterministic append-only insertion, leaf splitting, deletion, merge, and root-height changes while reusing unaffected historical pages exactly?

## Scope

The prototype imports the immutable-page byte codec from Experiment 0015 and adds structural updates over the same geometry:

- 16 KiB pages;
- 185 leaf entries;
- 255 internal children;
- authenticated child ranges and page digests;
- append-only emission of changed pages;
- no in-place modification of historical pages.

It remains a microformat rather than a successor epoch. Object records are synthetic locators, and the experiment does not publish complete snapshots, footers, source readers, recovery, or cross-language vectors.

## Insertion

Insertion descends to the owning leaf range, rejects duplicate identifiers, and emits:

- one replacement leaf when capacity remains;
- two leaves when the target overflows;
- replacement ancestors only along the changed path;
- a new root one level higher when the old root splits.

The split point is deterministic and independent of input iteration order.

## Deletion and merge

The current deletion prototype covers a leaf root or a height-one root:

- delete the selected identifier;
- keep the replacement leaf when it remains above minimum occupancy;
- otherwise combine it with one deterministic adjacent sibling;
- emit one merged leaf when the combined entries fit;
- otherwise redistribute into two deterministic leaves;
- collapse the root when only one child remains.

The prototype rejects deletion of the final tree entry rather than defining an empty-tree encoding.

## Required evidence

The executable checks require:

- insertion into a full leaf to create a split;
- unaffected sibling pages to remain byte-identical and reachable;
- deletion of the inserted identifier to merge the path deterministically;
- deterministic output bytes for repeated insert and delete operations;
- root-height increase from leaf root to internal root;
- root-height collapse after the inverse deletion;
- complete sorted identifier preservation;
- duplicate insertion rejection.

## Security interpretation

Immutable historical pages are reusable only when authenticated by the new root path and revalidated at their stored digest. Page reuse does not permit a writer to retain stale ranges, duplicate identifiers, overlapping child ranges, or under-specified split behavior.

Deletion is particularly sensitive: a writer must not make an object unreachable without an explicit logical deletion request, and compaction must distinguish physically retired pages from retained historical snapshots.

## Remaining work

A complete successor still requires:

- recursive deletion and rebalancing beyond height one;
- multi-level internal-page splits and merges;
- batched insertion, replacement, and deletion in one transaction;
- complete object records and overlap checks;
- roots, capabilities, extensions, snapshots, and exact-end publication;
- interrupted update and recovery vectors;
- independent implementation and invalid corpus;
- work, allocation, spill, and output limits;
- fuzzing across arbitrary operation sequences.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_splits.py
```
