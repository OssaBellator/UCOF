# Experiment 0025: Recursive Immutable Internal Split

- **Status:** Prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0015, 0023, and 0024
- **Script:** `tools/experiment_exp0002_immutable_page_internal_split.py`

## Question

Can immutable child references propagate a split through a completely full internal root while reusing every unaffected historical leaf?

## Boundary fixture

Using the Candidate 1 geometry:

- 185 entries per leaf;
- 255 children per internal page;
- 47,175 even-numbered object identifiers;
- exactly 255 full leaves beneath one full level-one root.

The original tree therefore contains 256 reachable pages.

## Recursive insertion

Inserting identifier `1` into the first full leaf causes:

1. one historical leaf to split into two new leaves;
2. the full 255-child root to acquire a 256th child;
3. that internal page to split into two new level-one pages;
4. a new level-two root to publish the two internal children.

Expected active-tree delta:

- five new pages;
- 254 reused historical leaves;
- two retired pages: the old leaf and old root.

A repeated operation from the same original root must produce identical bytes and root identity.

## Follow-up path copy

A second insertion beyond the current maximum does not split a page. From the level-two root it emits exactly:

- one replacement leaf;
- one replacement level-one parent;
- one replacement level-two root.

All other active pages remain exact historical references.

## Security interpretation

Recursive split propagation does not weaken page authentication. Each new parent authenticates exact child range, offset, length, level, and digest. Historical children remain acceptable only when reachable from the new authenticated root and when their page digests still verify.

The experiment does not define recursive deletion or internal merges. A successor must prevent stale separators, overlapping child ranges, lost identifiers, duplicate child references, and unbounded root growth.

## Remaining work

- recursive internal deletion, redistribution, merge, and root collapse;
- batched updates that split multiple branches in one publication;
- complete object, snapshot, footer, recovery, and source semantics;
- operation and output limits;
- invalid mutation corpus and fuzzing;
- independent implementation.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_internal_split.py
```
