# Immutable successor canonical occupancy policy

**Status:** proposed policy companion to FCP-0003 Draft  
**Rebased:** 2026-08-13 against consolidated `main`  
**Scope:** page grouping, split preservation, deletion preconditions, and transition-vector requirements  
**Tracking:** issues #16 and #76

## 1. Terms

For one page kind:

- `C` is the maximum number of entries in a page;
- `M = ceil(C / 2)` is the minimum number of entries in every non-root page;
- `N` is the ordered entry count to partition at one tree level.

A leaf entry is one primary locator. An internal entry is one authenticated child reference.

The active tree is non-empty. A root leaf may contain `1..C` entries. An internal root must contain `2..C` children. Every other page must contain `M..C` entries.

`C` is derived independently for each page kind from the accepted EXP-0003 field table. Numeric capacities from the current research implementation are examples, not proposed epoch constants.

## 2. Canonical full construction

The writer partitions each level independently and does not redistribute entries across more pages than necessary.

Given ordered entries and `N > 0`:

```text
if N <= C:
    emit [N]
else:
    full, remainder = divmod(N, C)

    if remainder == 0:
        emit [C] repeated full times
    else if remainder >= M:
        emit [C] repeated full times, then [remainder]
    else:
        emit [C] repeated (full - 1) times
        transfer = M - remainder
        emit [C - transfer, M]
```

The result must satisfy all of the following:

1. sizes sum exactly to `N`;
2. every size is at most `C`;
3. when more than one page is emitted, every size is at least `M`;
4. all pages except possibly the final two are full;
5. the algorithm is independent of caller chunking, allocation strategy, spill/run size, and merge fan-in.

The same algorithm is applied first to ordered leaf locators, then repeatedly to ordered child references until one root remains.

## 3. Current research examples

The consolidated research implementation currently uses leaf capacity `C = 185`, giving `M = 93`:

| N | Canonical groups |
|---:|---|
| 1 | `1` |
| 184 | `184` |
| 185 | `185` |
| 186 | `93, 93` |
| 277 | `184, 93` |
| 278 | `185, 93` |
| 370 | `185, 185` |
| 371 | `185, 93, 93` |
| 400 | `185, 122, 93` |

The current research internal geometry uses `C = 255`, giving `M = 128`:

| N | Canonical groups |
|---:|---|
| 2 | root `2` |
| 255 | root `255` |
| 256 | `128, 128` beneath a new root |
| 382 | `254, 128` |
| 383 | `255, 128` |
| 510 | `255, 255` |
| 511 | `255, 128, 128` |

These values are **not** to be copied into EXP-0003 blindly. A proposed 128-bit identifier / 64-byte primary-locator epoch must recompute leaf/internal capacities from the accepted field tables.

## 4. Insertion preservation

Insertion into a non-full page increases occupancy and therefore cannot violate the minimum.

Overflow produces `C + 1` entries. The deterministic split must produce two ordered pages whose sizes are both at least `M` and whose concatenation equals the pre-split ordered entry sequence.

The current proposal uses:

```text
left = ceil((C + 1) / 2)
right = floor((C + 1) / 2)
```

For both odd and even `C`, each result must be at least `M`.

Internal overflow applies the same rule to child references. When the root splits, the new root contains exactly two children.

The exact formula is byte-significant and must be stated identically in FCP-0003 and the EXP-0003 specification.

## 5. Deletion precondition

Persistent deletion may assume that its input passed complete strict validation plus canonical occupancy validation.

It must not silently reinterpret an earlier sparse-tail research tree as canonical EXP-0003 state.

After deleting an entry from one page:

1. if the page remains at or above `M`, rewrite the changed path only;
2. otherwise borrow one entry from the left sibling when that sibling would remain at or above `M`;
3. otherwise borrow from the right sibling under the same condition;
4. otherwise merge with the left sibling when present, or with the right sibling when no left sibling exists;
5. apply the same repair recursively to internal pages;
6. collapse an internal root with one child;
7. reject deletion of the final active object.

Sibling preference and merge direction are byte-significant and must be shared by every implementation.

## 6. Validation

Canonical occupancy validation is an additional strict invariant, not a recovery heuristic.

A canonical validator must:

- first establish complete exact-end validity of the active snapshot;
- traverse the authenticated tree under explicit page, depth, allocation, source-read, and request limits;
- reject a non-root leaf with fewer than `M_leaf` locators;
- reject a non-root internal page with fewer than `M_internal` children;
- reject an internal root with fewer than two children;
- permit a root leaf with one or more locators;
- never repair, redistribute, or select an earlier snapshot while validating.

A tool may expose legacy structural validation separately during experimental migration, but it must not label such output canonical or epoch-conforming.

## 7. Canonical final-state question

FCP-0003 must explicitly decide whether all valid construction paths for the same ordered active object set must converge to the same page partition/root identity.

There is a tension between two useful properties:

- **canonical final-state identity:** full rebuild and logically equivalent mixed operations produce the same page partition and root identity;
- **historical page reuse:** path-local persistent mutation may preserve exact old pages even when a canonical full rebuild would repartition them.

EXP-0003 must state which property wins when they conflict, or define separate canonicalization modes with unambiguous identity claims.

This decision must not be left to implementation accident.

## 8. Vector requirements

Cross-language authoritative vectors must pin at least:

- `1`, `C - 1`, `C`, and `C + 1` entries;
- final remainder `M - 1`, `M`, and `M + 1`;
- exact two-page and three-page boundaries;
- internal `C - 1`, `C`, and `C + 1` child counts;
- authenticated non-root pages at `M - 1` that fail only the occupancy rule;
- insertion into a non-full page;
- leaf split, internal split, and root-height increase;
- left borrow, right borrow, left merge, right merge, recursive internal repair, and root collapse;
- deterministic equivalence across caller operation order;
- any divergence cases relevant to the canonical-final-state decision.

Every valid identity affected by the accepted EXP-0003 field layout or page redistribution must be regenerated.

Earlier identities must be explicitly marked historical research evidence rather than left ambiguously valid.

## 9. Compatibility and decision boundary

The current research implementation already enforces the half-full policy for its own geometry. Adopting this companion document for EXP-0003 remains a separate normative decision because the proposed epoch changes identifier/locator layout and therefore page capacities.

For the old research 400-object fixture, maximum-packed `185, 185, 30` changed to `185, 122, 93`; the page count stayed the same but page/root/snapshot/commit/file identities changed.

Adopting this policy does not by itself allocate `UCOF-EXP-0003` or accept FCP-0003. It removes ambiguity from one byte-significant dimension so the specification, writers, validators, and independent vectors can converge.
