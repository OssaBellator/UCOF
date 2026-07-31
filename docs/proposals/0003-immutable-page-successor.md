# FCP-0003 — Immutable-page directory successor

- **Status:** Draft
- **Authors:** UCOF maintainers
- **Created:** 2026-07-31
- **Supersedes:** The reusable-page direction of FCP-0002 Candidate 1
- **Proposed experimental epoch:** `UCOF-EXP-0003`, allocated only if this proposal is accepted

## Summary

This proposal defines the review target for a new disposable experimental epoch based on immutable content-addressed directory pages, complete append-only snapshots, exact-end active publication, explicit linked history, separately requested recovery, and bounded random-access validation.

It does not stabilize UCOF, allocate permanent registry identifiers, or make the current microformat compatible with future revisions. The proposed `UCOF-EXP-0003` marker remains unallocated while this FCP is Draft.

## Motivation

`UCOF-EXP-0002` Candidate 1 demonstrated bounded lookup, strict validation, append publication, linked history, recovery, repair, rewrite, and cross-language bytes. It also authenticated the active snapshot sequence inside every directory page. An unchanged historical page therefore could not be reused byte-for-byte: changing the sequence changed the page digest and every ancestor.

The immutable-page successor experiments remove active sequence from page identity. Existing pages are referenced by authenticated content digest and may be reused across snapshots. Rust and Python evidence now covers deterministic bytes, mixed-age validation, replacement copy-on-write, modeled insertion/deletion algorithms, bounded source operations, linked history, report-only recovery, rewrite, and hostile-input campaigns.

## Scope

The proposed epoch includes:

- a fixed bootstrap header identifying one disposable epoch;
- complete immutable object records;
- fixed-size immutable authenticated B+tree pages;
- complete snapshot records and exact-end commit footers;
- append-only publication with a previous-footer link;
- strict active-state validation that never invokes recovery;
- authenticated lookup and absence;
- fully revalidated linked history;
- bounded suffix recovery that reports candidates without selecting one;
- verified-source rewrite and policy-driven semantic compaction inputs;
- strong-version source-view requirements for mutable remote objects.

Transforms, compression, encryption, signatures, provenance, external references, schemas, and stable profiles remain outside this epoch except for opaque capability and extension preservation fields needed to test evolution behavior.

## Proposed wire-policy package

The exact field table remains in `docs/spec/IMMUTABLE_SUCCESSOR_MICROFORMAT.md`. Acceptance of this FCP requires that document to be revised into one self-contained epoch specification and that all placeholder values be replaced by explicitly allocated experimental values.

### Identifiers

- Primary object identifiers are proposed as **128-bit opaque unsigned values**, ordered lexicographically by their 16-byte big-endian key representation.
- Zero is reserved as absent and is not a valid object identifier.
- Identifiers are lookup keys, not content digests, authenticity claims, or globally collision-free names.
- Applications selecting random identifiers remain responsible for an adequate collision policy.

Rationale: identifier longevity and namespace independence should be decided separately from locator density. Existing evidence shows that a compact 128-bit locator can remain smaller than Candidate 1's 64-bit 88-byte entry.

### Primary locator

The primary leaf entry is proposed as a fixed **64-byte minimal authenticated locator** containing:

- 16-byte object identifier;
- 8-byte record offset;
- 8-byte record length;
- 32-byte object digest.

Object kind and logical length remain authenticated inside the object header but are not mirrored in the primary locator. Profiles needing broad inventory without per-object header reads may define an authenticated optional secondary inventory index. No permanent reserved tail is included in every entry.

### Page geometry

- Page size: 16 KiB for this disposable epoch.
- Page identity: domain-separated digest over the complete canonical page bytes.
- Page identity excludes active snapshot sequence, physical offset, and file-instance commit identity.
- Leaf and internal entry layouts are version-fixed for the epoch; unknown page kinds fail closed.
- All unused page bytes are zero and participate in the page digest.

### Occupancy

- Non-root pages must contain at least half of their maximum entry capacity, rounded up.
- The root may contain one leaf entry or two internal children; an empty active tree is not representable in this epoch.
- Genesis construction and full rebuild pack pages left-to-right to maximum capacity except that the final two pages are redistributed when needed to satisfy minimum occupancy.

### Deterministic insertion

- Route by inclusive child key ranges.
- Replace an existing identifier in place semantically, producing new immutable bytes along its path.
- Insert a missing identifier into the target leaf in key order.
- On overflow, split at the lower median: the left page receives `ceil(n/2)` entries and the right page receives the remainder.
- Propagate one replacement child plus one new right sibling upward.
- Apply the same lower-median rule to internal overflow.
- Create a new root when the prior root splits.

### Deterministic deletion

- Delete only an existing identifier; deleting an absent identifier is an error.
- If a non-root page underflows, first borrow one entry from the left sibling when that sibling remains at or above minimum occupancy.
- Otherwise borrow from the right sibling under the same rule.
- Otherwise merge with the left sibling when present; if there is no left sibling, merge with the right.
- Recompute parent ranges after redistribution or merge.
- Apply underflow repair recursively to internal pages.
- Collapse an internal root containing one child to that child.
- Reject deletion of the final active object rather than representing an empty tree.

### Batch semantics

- A batch contains at most one operation per identifier.
- Operation order supplied by the caller does not affect bytes.
- The writer canonicalizes operations by identifier and computes one complete next snapshot.
- Multiple updates sharing ancestors emit each changed page once.
- Publication occurs only after all object and page bytes are complete.

## Assurance boundaries

The following claims remain distinct:

- **Strict active validity:** exact-end current commit and all active objects/pages validate.
- **Targeted lookup:** the active commit, one authenticated path, and the selected object or absence validate; unrelated objects are not claimed valid.
- **Verified history:** every linked prefix is strictly revalidated.
- **Recovery evidence:** bounded scanning reports exact strictly valid prefixes without selecting one.
- **Freshness:** requires trusted external state and is not established by file integrity or a stable source view.
- **Semantic compaction:** requires a profile/application dependency resolver and explicit unknown-semantics policy.

## Compatibility impact

This proposal is intentionally incompatible with `UCOF-EXP-0001` and `UCOF-EXP-0002`. Readers must reject unknown epochs rather than infer a layout. No migration support is promised for disposable experimental files.

If accepted, later incompatible byte changes require another experimental epoch. Acceptance does not create a stable 1.0 compatibility commitment.

## Security requirements

An implementation must:

- use checked arithmetic for every range and count;
- reject object/object, page/page, and object/structural overlap;
- bound file bytes, source reads, requests, hashes, allocations, objects, pages, depth, history, and recovery attempts;
- keep exact-end validation separate from recovery;
- require one strong source-version token for one remote assurance operation;
- reject unknown required capabilities while preserving integrity evidence where possible;
- preserve unknown optional extension bytes only under an explicit rewrite policy;
- treat integrity as distinct from authenticity, confidentiality, provenance, signer trust, and freshness;
- never claim that byte-scoped signatures survive rewrite.

## Required acceptance evidence

Before this FCP may move from Draft to Review:

1. revise the microformat into a complete independently implementable epoch draft;
2. integrate insertion, split, deletion, redistribution, merge, recursive underflow, and root-height changes into the reusable Rust byte writer;
3. publish cross-language vectors for every structural transition;
4. publish valid, invalid, interrupted, fork, recovery, compaction, and selected support-profile boundary vectors;
5. demonstrate source-based history, recovery, rewrite, and compaction without whole-file materialization where claimed;
6. implement and test at least one concrete conditional HTTP or cloud source adapter;
7. document production spill confidentiality, cleanup, descriptor, durability, and publication requirements;
8. run arbitrary-depth operation, hostile-source, and layer-targeted fuzzing continuously;
9. obtain an independently maintained parser/implementation or a documented external review with disposition of every material finding;
10. define maintainer disposition of FCP-0002 Candidate 1 and all unresolved objections;
11. record trusted freshness policy guidance for applications requiring rollback resistance.

## Alternatives considered

### Keep Candidate 1 and optimize the writer

Rejected. Active sequence is part of page identity, so exact historical reuse is impossible without changing bytes and digests. This is a format blocker, not merely a writer optimization.

### Use 64-bit identifiers to minimize entries

Not selected for this proposal. The eight-byte saving is material but does not outweigh the namespace and longevity benefit of an opaque 128-bit key in a universal container experiment. Review may reverse this choice only with explicit workload and collision-policy evidence.

### Mirror kind and logical length in every primary locator

Deferred to an optional authenticated inventory index. Mirroring reduces header requests for broad inventory but permanently increases every primary entry. The primary locator is optimized for authenticated lookup; profiles may add inventory acceleration.

### Variable-length primary entries

Rejected for this epoch. Offset tables, canonical packing, and parser differential risk add complexity before fixed-entry policy is independently reproduced.

## Open objections requested

Review should focus on:

- 128-bit identifier cost and ordering;
- whether primary locators should mirror inventory fields;
- minimum occupancy and deterministic sibling preference;
- whether empty active trees must be representable;
- batch canonicalization and duplicate-operation behavior;
- catalog and extension placement;
- exact source-view and freshness requirements;
- whether independent implementation is required before Review or only before acceptance.
