# EXP-0003 Scoped Determinism Decision Packet

**Status:** maintainer-review packet; recommendation only  
**Date:** 2026-08-13  
**Target:** FCP-0003 and Section 19 of `spec/experimental/UCOF-EXP-0003.md`  
**Evidence:** Experiments 0111 and 0138; primary history-independent dynamic-partitioning research  
**Tracking:** issues #13, #16, #76

## Purpose

The current EXP-0003 Draft distinguishes:

```text
canonical bulk/rewrite form
    same ordered active state + same epoch metadata
    -> deterministic fresh-tree layout

persistent transition form
    exact prior valid bytes + canonicalized operation batch
    -> deterministic next transition
    -> may preserve old pages/offsets
    -> equal logical active states may have different root digests
```

Section 19 calls this **scoped determinism**.

Experiment 0111 originally supported that direction because simple current-set partition families failed one or more of hard maximum, half-full occupancy, locality, or chosen-key robustness.

Newer primary research changes part of that premise: history-independent dynamic `(B,2)` partitioning now exists with deterministic `B/2..B` group bounds and a history-independent B-tree construction.

Experiment 0138 therefore re-evaluates Section 19 against the current EXP-0003 byte grammar rather than freezing the old rationale.

## Recommendation

Retain scoped determinism for the first EXP-0003 interoperability epoch, but revise the rationale.

Recommended normative principle:

> **Fresh canonical rewrite defines the current-set canonical structural form. Persistent append mutation is deterministic from the exact prior valid state and canonicalized batch, but persistent structural root/snapshot identity is not required to be history-independent. This is a deliberate consequence of authenticated physical offsets plus immutable record/page reuse, not a claim that history-independent B-tree partitioning is impossible.**

## The research result that must be acknowledged

Bender, Farach-Colton, Goodrich, and Komlós show history-independent dynamic partitioning where, after initial random hash functions are fixed, the current set has a unique partition representation.

Their `(B,2)` construction guarantees every group deterministically satisfies:

```text
B/2 <= size <= B
```

and they recursively apply independent hash functions at successive levels to build a history-independent B-tree.

For the tight 64-bit geometry recommended by #113:

```text
B = C = 291
integer B/2 lower bound -> 146
```

so the theoretical occupancy envelope is exactly:

```text
146..291
```

which matches the proposed UCOF half-full page rule.

Therefore Section 19 should not say or imply that efficient history-independent half-full partitioning is unavailable.

## Why that still does not canonicalize EXP-0003 root bytes

The active EXP-0003 authenticated representation contains absolute physical offsets in all three relevant layers:

```text
leaf locator:
    object_record_offset

internal child reference:
    child_page_offset

snapshot:
    root_page_offset
```

Page and snapshot digests authenticate those bytes.

Therefore the same logical object/tree relationships at different physical locations produce different authenticated structural bytes.

Experiment 0138 pins this mechanically for both the first-Draft 128-bit layout and the tight-64 Review candidate:

```text
same object record relocated:
    object digest same
    leaf page digest different

same child page digest relocated:
    parent page digest different

same hypothetical root digest relocated:
    snapshot digest different
```

A canonical partition rule cannot remove those fields.

## Relation to persistent reuse

Persistent immutable mutation intentionally preserves unchanged historical bytes at their existing offsets.

That is a major Phase 3 benefit:

- unchanged object records remain reusable;
- unchanged directory pages remain reusable;
- path-local changes append only changed/new bytes;
- remote/bounded readers receive exact physical ranges from authenticated locators/references.

Requiring the active root digest to depend only on the current logical set would therefore require more than changing split/merge boundaries.

At least one of these would need to change:

1. canonical physical reallocation/rewrite after mutation;
2. offset-free/content-addressed logical references plus a separate physical-location structure;
3. a specified history-independent physical allocator/placement layer;
4. a separate logical-state identity that intentionally excludes placement.

The first sacrifices much of persistent reuse; the second/third are major wire changes; the fourth is compatible with current scoped determinism and belongs naturally in profile/service semantics if needed.

## Randomness and interoperability

The research construction is canonical after initial random hash functions are fixed. The B-tree uses independent hash functions per level.

EXP-0003 exact-byte interoperability would therefore need a normative way to derive or carry those functions.

### Public fixed derivation

A public deterministic derivation can make independent writers reproduce the same partition, but its update-locality proof cannot simply inherit the paper's random-hash/oblivious-adversary expectation when writers/users may choose ObjectIds after observing the public rule.

Experiment 0111 already demonstrated chosen-ID steering against simpler public hash-priority families. That does not invalidate the protected-Cartesian construction or its hard occupancy bounds; it means UCOF would need its own adaptive-key locality evidence before promising the research update bounds.

### Public per-lineage random seed

This preserves canonicality conditional on the seed, but equal logical sets in independent lineages need not have equal roots unless the seed is treated as part of state. Later writers can observe the public seed before choosing keys.

### Secret seed

This impairs independent writer/validator reproduction and therefore does not fit the first interoperability epoch.

None of these choices is required merely to retain scoped determinism.

## What scoped determinism does promise

If retained, the normative text should make these guarantees explicit.

### Fresh/canonical construction

Given:

- the same accepted object/locator bytes;
- the same ordered ObjectIds;
- the same accepted catalog/metadata facts;
- the same epoch rules;

canonical bulk/rewrite construction emits one deterministic active representation according to the accepted bulk grouping/physical ordering rules.

### Persistent mutation

Given:

- the same exact validated prior snapshot/file prefix;
- the same semantic batch;
- canonicalized operation ordering;

conforming persistent writers emit the same next transition bytes under the specified mutation policy.

### Not promised

Persistent mode does not promise:

```text
same current logical active set across different histories
-> same page partition
-> same physical offsets
-> same root digest
-> same snapshot digest
```

Applications must not treat persistent structural root digest as universal semantic-state identity.

## Canonical rewrite remains the normalization operation

A tool/profile that needs reproducible structural reissuance can perform canonical rewrite/compaction into the accepted fresh form.

That operation is the explicit boundary where UCOF chooses reproducibility over historical physical reuse.

A rewrite has new byte/commit identity and must not pretend to preserve the old append-history identity.

## Full-file history independence is out of scope

EXP-0003 may intentionally retain linked historical commits and old immutable bytes.

A file that explicitly retains history necessarily reveals that those historical states existed.

The relevant Review question is therefore active structural root canonicality, not full-file operation-order privacy.

This packet does not add history independence as a Core security claim.

## Proposed Section 19 replacement shape

If the recommendation is selected, update Section 19 along these lines:

### 19.1 Canonical bulk/rewrite form

Canonical bulk/rewrite construction is current-state deterministic and defines the epoch's normalized fresh structural representation.

### 19.2 Persistent transition form

Persistent mutation is deterministic from exact prior validated bytes plus canonicalized semantic operations and may reuse existing immutable records/pages at their existing authenticated offsets.

### 19.3 Placement-sensitive identity

Because locators, child references, and snapshots authenticate physical offsets, persistent structural root/snapshot identity may depend on prior physical history even if a history-independent partition algorithm could canonicalize group boundaries.

### 19.4 Identity boundary

Persistent structural digests are byte/integrity identities, not universal logical-state identities. Profiles/services that require a logical-state digest define it separately, or normalize through canonical rewrite.

### 19.5 Research note

A non-normative note may cite history-independent `(B,2)` partitioning as a possible future building block, while stating that adopting it for canonical persistent roots would require a coordinated reference/placement/randomness design.

## Future incompatible-epoch gate

A proposal to replace scoped determinism with history-independent persistent root identity should answer all of these before changing bytes:

1. How are object physical locations represented without making leaf identity placement-sensitive?
2. How are child physical locations represented without making ancestor identity placement-sensitive?
3. How is root location bound without making snapshot identity placement-sensitive?
4. What per-level priority/hash functions are normative?
5. How are they reproduced across languages?
6. What locality/write-amplification guarantee applies to adaptive writer-chosen ObjectIds?
7. How are exact accepted `[M,C]` bounds preserved?
8. How do recovery/repair/random-access locate physical bytes?
9. What persistent reuse/history properties are intentionally sacrificed or retained?
10. Is the desired identity structural bytes or a separate logical-state identity?

## Proposed maintainer disposition

Select exactly one:

- [ ] **Retain scoped determinism with the revised physical-offset rationale in this packet.**
- [ ] Replace scoped determinism with a history-independent persistent representation proposal that also specifies placement/reference/randomness semantics: ____________________.
- [ ] Defer pending one named blocker/experiment: ____________________.

**Packet recommendation:** first option.

No checkbox is selected by this packet.

## Boundary

This packet does **not**:

- claim history-independent B-trees are impossible;
- reject the Bender–Farach-Colton–Goodrich–Komlós construction as a future building block;
- change current Draft bytes;
- select identifier geometry;
- select deletion borrower policy;
- accept catalog/hash packets;
- define a new logical-state digest;
- accept FCP-0003;
- allocate EXP-0003;
- regenerate authoritative vectors.

It makes the Section 19 choice honest: the first epoch prefers append-local authenticated physical reuse and an explicit canonical-rewrite normalization boundary over a larger placement/reference redesign solely to canonicalize persistent root identity.
