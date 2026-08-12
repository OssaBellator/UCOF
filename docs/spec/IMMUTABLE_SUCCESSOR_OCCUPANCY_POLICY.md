# Immutable successor canonical occupancy policy

**Status:** proposed policy companion to FCP-0003 Draft  
**Rebased:** 2026-08-13 against consolidated `main`  
**Scope:** page grouping, split preservation, deletion preconditions, transition identity, and vector requirements  
**Tracking:** issues #16 and #76

## 1. Terms

For one page kind:

- `C` is the maximum number of entries in a page;
- `M = ceil(C / 2)` is the minimum number of entries in every non-root page;
- `N` is the ordered entry count to partition at one tree level.

A leaf entry is one primary locator. An internal entry is one authenticated child reference.

The active tree is non-empty. A root leaf may contain `1..C` entries. An internal root must contain `2..C` children. Every other page must contain `M..C` entries.

The current EXP-0003 Draft proposes:

- 16 KiB pages;
- 80-byte page headers;
- 64-byte leaf locators;
- 72-byte internal child references.

That yields the proposed review geometry:

```text
leaf:     C = floor((16384 - 80) / 64) = 254
          M = 127

internal: C = floor((16384 - 80) / 72) = 226
          M = 113
```

These numbers remain Draft until FCP-0003 is accepted for experimentation.

## 2. Canonical bulk construction

The canonical **bulk/rewrite** writer partitions each level independently and does not redistribute entries across more pages than necessary.

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

## 3. Proposed EXP-0003 boundary examples

For proposed leaf geometry `C = 254`, `M = 127`:

| N | Canonical groups |
|---:|---|
| 1 | `1` |
| 253 | `253` |
| 254 | `254` |
| 255 | `128, 127` |
| 380 | `253, 127` |
| 381 | `254, 127` |
| 508 | `254, 254` |
| 509 | `254, 128, 127` |

For proposed internal geometry `C = 226`, `M = 113`:

| N | Canonical groups |
|---:|---|
| 2 | root `2` |
| 225 | root `225` |
| 226 | root `226` |
| 227 | `114, 113` beneath a new root |
| 338 | `225, 113` |
| 339 | `226, 113` |
| 452 | `226, 226` |
| 453 | `226, 114, 113` |

For comparison only, the consolidated 64-bit/88-byte research layout used leaf `C = 185`, `M = 93` and internal `C = 255`, `M = 128`. Those old capacities are historical evidence, not EXP-0003 constants.

## 4. Insertion preservation

Insertion into a non-full page increases occupancy and therefore cannot violate the minimum.

Overflow produces `C + 1` entries. The proposed deterministic split is:

```text
left = ceil((C + 1) / 2)
right = floor((C + 1) / 2)
```

Both results must be at least `M`.

For the proposed EXP-0003 geometry:

```text
leaf overflow:     255 -> 128, 127
internal overflow: 227 -> 114, 113
```

Internal overflow applies the same rule to ordered child references. When the root splits, the new root contains exactly two children.

The split formula is byte-significant and must be identical in FCP-0003, `spec/experimental/UCOF-EXP-0003.md`, reference code, and authoritative vectors.

## 5. Deletion precondition and repair

Persistent deletion may assume that its input passed complete strict validation plus the selected occupancy validation.

It must not silently reinterpret earlier research layouts as canonical EXP-0003 state.

After deleting an entry from one page:

1. if the page remains at or above `M`, rewrite the changed path only;
2. otherwise borrow one entry from the left sibling when that sibling remains at or above `M`;
3. otherwise borrow from the right sibling under the same condition;
4. otherwise merge with the left sibling when present, or with the right sibling when no left sibling exists;
5. apply the same repair recursively to internal pages;
6. collapse an internal root with one child;
7. reject deletion of the final active object.

Sibling preference and merge direction are byte-significant and must be shared by every implementation.

## 6. Validation

Occupancy validation is a strict invariant, not a recovery heuristic.

A conforming EXP-0003 validator must:

- first establish complete exact-end validity of the active snapshot;
- traverse the authenticated tree under explicit page, depth, allocation, source-read, and request limits;
- reject a non-root leaf with fewer than `M_leaf` locators;
- reject a non-root internal page with fewer than `M_internal` children;
- reject an internal root with fewer than two children;
- permit a root leaf with one or more locators;
- reject counts above capacity;
- never repair, redistribute, or select an earlier snapshot while validating.

A tool may expose legacy structural validation separately during experimental migration, but it must not label such output EXP-0003-conforming.

## 7. Scoped determinism: bulk identity versus persistent transition identity

The current EXP-0003 Draft proposes a deliberate distinction rather than requiring all update histories to converge to one page partition.

### Canonical bulk/rewrite form

Given the same ordered active object/locator set and the same epoch-level metadata, a fresh genesis/rewrite uses Section 2 grouping and therefore has one deterministic bulk tree layout.

### Persistent transition form

A normal append mutation is deterministic from:

- the exact prior valid tree;
- the canonicalized operation set;
- the specified insertion/deletion/batch algorithm.

It may preserve historical pages whose exact bodies remain valid. Therefore it need not have the same root digest as a fresh bulk rewrite of the resulting logical active state.

### Proposed identity claim

Structural root/snapshot identity is history-sensitive under persistent mutation.

Equal logical active object sets are **not** guaranteed to have equal root digests when reached through different histories.

Profiles requiring a history-independent logical-state identity must define that identity separately or use canonical rewrite output.

This scoped-determinism proposal is intended to preserve the principal value of immutable-page copy-on-write while keeping reproducible bulk/rewrite output. It remains a Review decision, not an accepted epoch rule.

## 8. Vector requirements

Cross-language authoritative EXP-0003 vectors must pin at least:

- `1`, `C - 1`, `C`, and `C + 1` entries;
- final remainder `M - 1`, `M`, and `M + 1`;
- exact two-page and three-page boundaries;
- internal `C - 1`, `C`, and `C + 1` child counts;
- authenticated non-root pages at `M - 1` that fail only the occupancy rule;
- insertion into a non-full page;
- leaf split, internal split, and root-height increase;
- left borrow, right borrow, left merge, right merge, recursive internal repair, and root collapse;
- deterministic equivalence across caller operation order;
- at least one case demonstrating that a persistent transition and a canonical fresh rewrite may legitimately have different structural root identities while representing the same logical active object set.

Every valid identity affected by the accepted EXP-0003 field layout or grouping rules must be regenerated.

Earlier research identities must be explicitly marked historical evidence rather than left ambiguously valid.

## 9. Compatibility and decision boundary

The current research implementation already enforces a half-full policy for its own geometry. Adopting this companion for EXP-0003 is still a separate normative decision because EXP-0003 changes identifier/locator/page-header layout and therefore capacities and all affected identities.

For the old 400-object research fixture, prior maximum-packed `185, 185, 30` became `185, 122, 93`; page count stayed the same but page/root/snapshot/commit/file identities changed. This remains useful evidence that occupancy policy is byte-significant.

Adopting this policy does not by itself allocate `UCOF-EXP-0003` or accept FCP-0003. It provides one explicit review target so the specification, writers, validators, and independent vectors can converge.
