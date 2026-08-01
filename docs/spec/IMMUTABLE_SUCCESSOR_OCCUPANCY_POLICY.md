# Immutable successor canonical occupancy policy

**Status:** proposed policy companion to FCP-0003 Draft  
**Scope:** page grouping, split preservation, deletion preconditions, and transition-vector requirements

## 1. Terms

For one page kind:

- `C` is the maximum number of entries in a page;
- `M = ceil(C / 2)` is the minimum number of entries in every non-root page;
- `N` is the ordered entry count to partition at one tree level.

A leaf entry is one primary locator. An internal entry is one authenticated child reference.

The active tree is non-empty. A root leaf may contain `1..C` entries. An internal root must contain `2..C` children. Every other page must contain `M..C` entries.

## 2. Canonical full construction

The writer partitions each level independently. It must not redistribute entries across more pages than necessary.

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
5. the algorithm is independent of caller chunking and allocation strategy.

The same algorithm is applied first to ordered leaf locators, then repeatedly to each ordered level of child references until one root remains.

## 3. Boundary examples

For the current research leaf geometry, `C = 185` and `M = 93`:

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

For the current research internal geometry, `C = 255` and `M = 128`:

| N | Canonical groups |
|---:|---|
| 2 | root `2` |
| 255 | root `255` |
| 256 | `128, 128` beneath a new root |
| 382 | `254, 128` |
| 383 | `255, 128` |
| 510 | `255, 255` |
| 511 | `255, 128, 128` |

These numerical values are research-layout examples. A later 128-bit/64-byte-locator epoch must recompute `C` and `M` from its accepted field table while retaining the same algorithm unless the proposal is amended.

## 4. Insertion preservation

Insertion into a non-full page increases its occupancy and therefore cannot violate the minimum.

Overflow produces `C + 1` entries. The deterministic lower-median split emits:

```text
left = ceil((C + 1) / 2)
right = floor((C + 1) / 2)
```

For both odd and even `C`, each result must be at least `M`. Internal overflow applies the same rule to child references. When the root splits, the new root contains exactly two children.

## 5. Deletion precondition

Persistent deletion may assume that its input passed complete strict validation plus canonical occupancy validation. It must not silently reinterpret an earlier sparse-tail research tree as canonical.

After deleting an entry from one page:

1. if the page remains at or above `M`, rewrite the changed path only;
2. otherwise borrow one entry from the left sibling when the sibling would remain at or above `M`;
3. otherwise borrow from the right sibling under the same condition;
4. otherwise merge with the left sibling when present, or with the right sibling when no left sibling exists;
5. apply the same repair recursively to internal pages;
6. collapse an internal root with one child;
7. reject deletion of the final active object.

The exact sibling preference is byte-significant and must be shared by every implementation.

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

A tool may expose legacy structural validation separately during migration, but it must not label such output canonical or epoch-conforming.

## 7. Vector requirements

Cross-language vectors must pin at least:

- `1`, `C - 1`, `C`, and `C + 1` entries;
- final remainder `M - 1`, `M`, and `M + 1`;
- exact two-page and three-page boundaries;
- internal `C - 1`, `C`, and `C + 1` child counts;
- authenticated non-root pages at `M - 1` that fail only the occupancy rule;
- insertion into a non-full page;
- leaf split, internal split, and root-height increase;
- left borrow, right borrow, left merge, right merge, recursive internal repair, and root collapse;
- deterministic equivalence across caller operation order.

Every valid vector identity affected by page redistribution must be regenerated. Earlier identities must be explicitly marked as pre-convergence research evidence rather than left ambiguously valid.

## 8. Compatibility and decision boundary

This policy is byte-changing for trees whose earlier final page was below minimum. For the current 400-object research fixture, `185, 185, 30` becomes `185, 122, 93`; page count remains four including the root, but page digests, root digest, snapshot digest, commit digest, and whole-file identity change.

Adopting this companion document does not allocate an epoch or accept FCP-0003. It removes ambiguity from one policy dimension so writer, validator, deletion, and vector work can converge against the same rule.
