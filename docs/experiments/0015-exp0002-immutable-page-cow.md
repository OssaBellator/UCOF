# Experiment 0015: Immutable-Page Copy-on-Write Byte Prototype

- **Status:** Prototype
- **Date:** 2026-07-30
- **Related:** FCP-0002, Experiments 0008, 0009, 0011, and 0014
- **Script:** `tools/experiment_exp0002_immutable_page_cow.py`

## Question

Can a byte-level paged directory remove Candidate 1's active-sequence field, reuse unchanged historical pages exactly, validate mixed-age trees strictly, and preserve exact-end interrupted-append behavior?

## Scope

This is a directory and publication microformat, not `UCOF-EXP-0002` Candidate 2. It deliberately reuses Candidate 1 geometry while isolating the page-identity question:

- 16 KiB pages;
- 64-byte page headers;
- 88-byte leaf entries;
- 64-byte internal entries;
- authenticated parent child locators and page digests;
- immutable page bytes with no snapshot sequence field;
- append-only snapshots and exact-end footers;
- parent snapshot digest and previous-footer linkage.

Leaf object locators are synthetic authenticated test data. The prototype does not encode real object records, page splits, insertions, deletions, capabilities, roots, repair, or the complete Candidate 1 field set.

## Prototype algorithm

Genesis:

1. deterministically sort 100,000 locator entries;
2. emit canonical leaf pages;
3. emit canonical internal pages bottom-up;
4. publish a snapshot containing the root locator and parent identity;
5. publish an exact-end footer and commit digest.

Append update:

1. strictly validate the source;
2. sort and deduplicate replacement locators;
3. descend only into ranges containing changed identifiers;
4. emit a replacement leaf for each changed range;
5. emit only ancestors whose child locators changed;
6. preserve every unaffected historical page offset and digest exactly;
7. publish a new snapshot and exact-end footer.

Strict validation accepts pages from earlier commits only when they are reachable from the authenticated current root. It checks exact page digests, canonical ordering, ranges, levels, padding, physical bounds, parent linkage, snapshot digest, commit digest, and exact file end.

## Required checks

The executable prototype requires:

- deterministic output for the same logical update;
- exactly one new page per changed path level for a no-split single update;
- exact reuse of every unaffected page;
- retirement of the replaced historical path from the new root;
- corruption of a reused historical page to fail at its page digest even though it lies outside the current commit digest;
- an interrupted latest footer to fail strict validation;
- the previous complete prefix to remain strictly valid.

## Expected geometry

For 100,000 objects using Candidate 1 capacities:

```text
leaf pages:       541
internal pages:     3
root pages:         1
total pages:      545
depth:              3
```

A no-split single locator replacement should append three pages rather than rebuilding all 545 pages.

## Security interpretation

The prototype demonstrates that page membership does not require an active snapshot sequence inside each immutable page. Membership is authenticated by:

- the page digest in its parent entry or snapshot root;
- the authenticated root path;
- strict page range, level, and canonicality checks;
- the publishing snapshot and exact-end commit.

Historical page mutation remains detectable because every reused page is rehashed when traversed. The current commit digest need not cover old page bytes as long as the active authenticated tree carries each page digest and strict validation rechecks it.

This does not provide authenticity or external freshness. Replaying an older valid whole file remains possible without trusted external state.

## Limitations before a successor byte candidate

A complete successor still needs:

- deterministic insertion, deletion, split, merge, and root-height changes;
- batching across many changed leaves with shared ancestors emitted once;
- bounded external sorting integrated with page emission;
- object-record and page-range overlap checks;
- exact roots, capabilities, and preservation semantics;
- cross-language valid and invalid vectors;
- random-access lookup and recovery over mixed-age pages;
- writer failure and spill cleanup policy;
- fuzzing and portability evidence.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_cow.py
```
