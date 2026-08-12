# UCOF-EXP-0003 — Immutable-Page Interoperability Draft

**Status:** Draft for review; experimental epoch not yet allocated  
**Date:** 2026-08-13  
**Related:** FCP-0003, issues #13, #16, #76  
**Compatibility promise:** None

## 1. Purpose

This document is the first self-contained byte-level draft for the proposed `UCOF-EXP-0003` disposable interoperability epoch.

Its purpose is to make the immutable-page successor independently implementable without requiring access to Rust or Python reference-source internals.

This document is **not yet an allocated epoch specification**. All magic values, field widths, algorithms, and identities below are proposed review targets until FCP-0003 is accepted for experimentation.

If the proposal changes any byte-significant rule before allocation, all affected vectors must be regenerated. Experimental files created from this draft have no migration or compatibility guarantee.

## 2. Scope

EXP-0003 is intended to exercise only the structural core needed to validate the UCOF architecture:

- fixed bootstrap framing;
- opaque object records;
- immutable authenticated B+tree directory pages;
- append-only complete snapshots;
- exact-end commit publication;
- persistent page reuse;
- deterministic insertion, deletion, and batch transition rules;
- strict bounded validation;
- authenticated lookup and absence;
- linked-history verification;
- separately requested bounded recovery;
- verified rewrite/compaction inputs;
- capability and extension preservation boundaries;
- strong-version remote source assurance contracts.

EXP-0003 does **not** define normative compression, transforms, schemas, signatures, provenance, encryption, selective disclosure, external references, Archive/Table profiles, or stable registry assignments.

## 3. Architectural principle

UCOF is a **universal container, not a universal representation**.

The EXP-0003 core therefore treats application payloads as opaque bytes. It defines structural validity and authenticated object location, but it does not infer application-level dependencies, execute embedded code, fetch external resources, or require domain-specific interpretation.

Semantic compaction requires profile/application dependency rules outside this core specification.

## 4. Common representation rules

### 4.1 Integer encoding

Unless explicitly stated otherwise, multi-byte integer fields are unsigned little-endian integers.

All conversions and arithmetic used to interpret ranges, lengths, counts, sequences, and offsets must be checked before use.

### 4.2 Object identifiers

`ObjectId` is exactly 16 bytes.

It is an opaque lookup key, not an integer arithmetic type, content digest, signature identity, or globally guaranteed unique name.

Canonical ordering is unsigned lexicographic byte ordering over the 16 stored bytes. This is equivalent to ordering an unsigned 128-bit value encoded big-endian, but implementations should treat the field as a fixed byte key.

The all-zero `ObjectId` is reserved and invalid.

### 4.3 Absolute offsets

All stored physical offsets are unsigned 64-bit absolute byte offsets from the beginning of the file.

An implementation must reject an offset/length combination that overflows, exceeds the validated prefix, violates the required record boundary, or aliases a forbidden structural range.

### 4.4 Reserved bytes

Every reserved byte is zero unless this specification explicitly assigns another meaning.

Non-zero reserved bytes are malformed EXP-0003 input.

### 4.5 Padding

Unused bytes inside fixed-size pages are zero and participate in page identity.

### 4.6 Exact-end authority

A commit footer has active-state authority only when it occupies the exact final `FOOTER_LEN` bytes of the prefix being strictly validated.

Strict validation never searches backward for another footer.

Recovery is a separate operation defined later in this document.

### 4.7 Non-empty active tree

This draft does not represent an empty active object tree.

A valid snapshot contains at least one active object. Deleting the final active object is an error.

This remains a proposed FCP decision until EXP-0003 is allocated.

## 5. Cryptographic identity

### 5.1 Baseline algorithm

EXP-0003 proposes SHA-256 as its single baseline digest algorithm.

Algorithm agility is intentionally deferred. A future incompatible change to mandatory digest behavior requires another experimental epoch unless the accepted EXP-0003 design explicitly adds an algorithm field before allocation.

### 5.2 Domain prefixes

The proposed exact ASCII byte prefixes are:

| Scope | Prefix bytes |
|---|---|
| Object | `UCOF-EXP-0003-OBJECT\0` |
| Page | `UCOF-EXP-0003-PAGE\0` |
| Snapshot | `UCOF-EXP-0003-SNAPSHOT\0` |
| Commit | `UCOF-EXP-0003-COMMIT\0` |

The terminating NUL byte is part of each prefix.

### 5.3 Object identity

```text
object_digest = SHA-256(OBJECT_DOMAIN || exact_object_record_bytes)
```

The exact object record includes its fixed header and complete stored payload.

### 5.4 Page identity

```text
page_digest = SHA-256(PAGE_DOMAIN || exact_16_KiB_page_bytes)
```

Page identity excludes physical offset, active snapshot sequence, and file-instance commit identity because none of those values appears in the page bytes.

### 5.5 Snapshot identity

```text
snapshot_digest = SHA-256(SNAPSHOT_DOMAIN || exact_snapshot_bytes)
```

### 5.6 Commit identity

Commit identity is defined with the footer in Section 12.

### 5.7 Non-claims

These digests provide integrity relative to authenticated references. They do not establish authenticity, signer trust, confidentiality, provenance, authorization, freshness, or rollback resistance.

## 6. Proposed constants

| Name | Proposed value |
|---|---:|
| `BOOTSTRAP_LEN` | 64 |
| `OBJECT_HEADER_LEN` | 64 |
| `PAGE_SIZE` | 16,384 |
| `PAGE_HEADER_LEN` | 80 |
| `LEAF_ENTRY_LEN` | 64 |
| `INTERNAL_ENTRY_LEN` | 72 |
| `SNAPSHOT_LEN` | 96 |
| `FOOTER_LEN` | 128 |
| `LEAF_CAPACITY` | 254 |
| `LEAF_MIN_OCCUPANCY` | 127 |
| `INTERNAL_FANOUT` | 226 |
| `INTERNAL_MIN_OCCUPANCY` | 113 |

The capacities are derived as:

```text
LEAF_CAPACITY = floor((PAGE_SIZE - PAGE_HEADER_LEN) / LEAF_ENTRY_LEN)
              = floor(16304 / 64)
              = 254

INTERNAL_FANOUT = floor((PAGE_SIZE - PAGE_HEADER_LEN) / INTERNAL_ENTRY_LEN)
                = floor(16304 / 72)
                = 226
```

These values are Draft review targets, not allocated epoch constants yet.

## 7. Bootstrap header

The file begins with exactly 64 bytes.

Proposed magic: ASCII `UCOFIM03`.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCOFIM03` |
| 8 | 56 | Zero reserved bytes |

The bootstrap header is included in the genesis commit preimage.

No active root, mutable pointer, allocator state, or file length is stored in the bootstrap header. Active publication is determined only by the exact-end footer.

## 8. Object record

An object record is a 64-byte fixed header followed immediately by `stored_length` payload bytes.

Proposed magic: ASCII `UCOBOBJ3`.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCOBOBJ3` |
| 8 | 2 | Header length, exactly 64 |
| 10 | 2 | Non-zero object kind |
| 12 | 4 | Flags, zero in this epoch |
| 16 | 16 | Non-zero `ObjectId` |
| 32 | 8 | Stored payload length |
| 40 | 8 | Logical payload length |
| 48 | 16 | Zero reserved bytes |
| 64 | variable | Exact stored payload bytes |

The exact object record length is:

```text
OBJECT_HEADER_LEN + stored_payload_length
```

Because transforms/compression are outside EXP-0003, this draft requires:

```text
logical_payload_length == stored_payload_length
```

A later transform epoch/profile may distinguish these values, but EXP-0003 must not infer transform semantics from a mismatch.

The primary locator authenticates the complete object record by its SHA-256 digest.

## 9. Directory page envelope

Every page is exactly 16,384 bytes.

Proposed magic: ASCII `UCPGIM03`.

### 9.1 Page header

The page header is exactly 80 bytes.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCPGIM03` |
| 8 | 1 | Page kind: 1 leaf, 2 internal |
| 9 | 1 | Page level; leaves use 0 |
| 10 | 2 | Header length, exactly 80 |
| 12 | 4 | Entry count |
| 16 | 2 | Entry width |
| 18 | 2 | Flags, zero in this epoch |
| 20 | 16 | Minimum `ObjectId` |
| 36 | 16 | Maximum `ObjectId` |
| 52 | 28 | Zero reserved bytes |
| 80 | variable | Packed entries |
| after entries | remainder | Zero padding through byte 16,383 |

The minimum and maximum fields are inclusive key bounds and must equal the first/last entry bounds represented by the page.

The complete page bytes are authenticated. Active sequence and physical offset are deliberately absent from page identity.

### 9.2 Page-kind rules

For a leaf page:

- `kind == 1`;
- `level == 0`;
- `entry_width == 64`.

For an internal page:

- `kind == 2`;
- `level > 0`;
- `entry_width == 72`.

Unknown page kinds are malformed/unsupported required structure and fail closed.

## 10. Leaf locator

A primary leaf locator is exactly 64 bytes.

| Offset within entry | Size | Field |
|---:|---:|---|
| 0 | 16 | `ObjectId` |
| 16 | 8 | Absolute object record offset |
| 24 | 8 | Exact object record length |
| 32 | 32 | Object-record digest |

No object kind, logical length, transform identifier, schema identifier, or permanently reserved tail is mirrored in the primary locator.

Those facts are authenticated inside the object record or in optional profile/index structures.

Leaf locators are strictly increasing by `ObjectId`.

The first locator ID equals the page header minimum and the final locator ID equals the page header maximum.

### 10.1 Locator/header cross-check

When an object record is validated from a locator, the implementation must verify at least:

- locator range is in bounds;
- record header magic and header length;
- record `ObjectId` equals locator `ObjectId`;
- record length equals locator record length;
- object kind is non-zero;
- flags/reserved bytes satisfy the epoch rules;
- logical/stored length rule;
- complete object digest equals locator digest.

## 11. Internal child reference

An internal child reference is proposed as exactly 72 bytes.

| Offset within entry | Size | Field |
|---:|---:|---|
| 0 | 16 | Child minimum `ObjectId` |
| 16 | 16 | Child maximum `ObjectId` |
| 32 | 8 | Absolute child page offset |
| 40 | 32 | Child page digest |

The child page length is omitted because all EXP-0003 pages are exactly `PAGE_SIZE` bytes.

The child level is omitted because it is exactly `parent_level - 1`.

Child references must satisfy:

- non-zero minimum and maximum IDs;
- `minimum <= maximum`;
- strictly ordered, non-overlapping key ranges;
- first child minimum equals parent page minimum;
- final child maximum equals parent page maximum;
- referenced page has the expected level;
- referenced page digest matches the exact child page bytes;
- referenced page header minimum/maximum agree with the child entry.

There is no implied requirement that adjacent child ranges be numerically contiguous. Object IDs are sparse opaque keys.

## 12. Snapshot record

A snapshot is exactly 96 bytes.

Proposed magic: ASCII `UCSNIM03`.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCSNIM03` |
| 8 | 8 | Snapshot sequence |
| 16 | 8 | Directory root page offset |
| 24 | 8 | Directory root level encoded as `u64` |
| 32 | 32 | Directory root page digest |
| 64 | 32 | Parent snapshot digest; all zero for genesis |

The complete 96 bytes are authenticated under the snapshot domain.

A snapshot is complete: it identifies one complete active directory root. EXP-0003 does not define partial-progress snapshots as active states.

Catalog roots, capabilities, optional indexes, and profile declarations should be represented through authenticated objects/roots rather than expanding this fixed structural snapshot record unless FCP review selects another explicit layout.

## 13. Commit footer

A commit footer is exactly 128 bytes.

Proposed magic: ASCII `UCFTIM03`.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCFTIM03` |
| 8 | 8 | Sequence |
| 16 | 8 | Snapshot offset |
| 24 | 8 | Snapshot length, exactly 96 |
| 32 | 8 | Previous footer offset, or `u64::MAX` for genesis |
| 40 | 8 | Count of page records emitted by this commit |
| 48 | 32 | Snapshot digest |
| 80 | 32 | Commit digest |
| 112 | 16 | Zero reserved bytes |

### 13.1 Footer semantics preimage

The footer semantics value is exactly 72 bytes:

```text
sequence                 u64 little-endian
snapshot_offset          u64 little-endian
snapshot_length          u64 little-endian
previous_footer_offset   u64 little-endian
page_count_current       u64 little-endian
snapshot_digest          32 bytes
```

### 13.2 Current commit byte range

For genesis, current commit bytes begin at file offset zero.

For a child commit, current commit bytes begin immediately after the complete previous 128-byte footer.

Current commit bytes end immediately before the new footer.

### 13.3 Commit digest

```text
commit_digest = SHA-256(
    COMMIT_DOMAIN ||
    exact_current_commit_bytes_before_footer ||
    footer_semantics
)
```

The commit digest authenticates newly appended bytes and the footer semantics. It does not rehash unchanged historical objects/pages; those are authenticated through object/page references from the new snapshot.

### 13.4 Sequence/linkage rules

Genesis:

- sequence is zero;
- parent snapshot digest is all zero;
- previous footer offset is `u64::MAX`.

Child commit:

- sequence equals parent sequence plus one;
- parent snapshot digest equals the parent snapshot digest identity;
- previous footer offset identifies the exact parent footer;
- previous footer physically precedes the child commit.

## 14. Physical record ordering

### 14.1 Genesis

A deterministic genesis writer emits:

1. bootstrap header;
2. object records in canonical object-ID order;
3. canonical leaf pages;
4. canonical internal pages from leaf level toward root;
5. snapshot;
6. exact-end footer.

### 14.2 Append

A child commit may append:

1. new/replacement object records required by the transition;
2. new immutable leaf/internal pages required by the transition;
3. new snapshot;
4. exact-end footer.

Unchanged historical object/page records may be referenced at older offsets.

An interrupted append does not become active because it lacks a complete valid exact-end footer.

## 15. Canonical occupancy

For one page kind:

```text
C = maximum entries
M = ceil(C / 2)
```

Proposed EXP-0003 values are:

```text
leaf:     C = 254, M = 127
internal: C = 226, M = 113
```

### 15.1 Root exceptions

- A root leaf contains `1..LEAF_CAPACITY` entries.
- A root internal page contains `2..INTERNAL_FANOUT` children.
- Every non-root leaf contains `LEAF_MIN_OCCUPANCY..LEAF_CAPACITY` entries.
- Every non-root internal page contains `INTERNAL_MIN_OCCUPANCY..INTERNAL_FANOUT` children.

### 15.2 Canonical bulk grouping

For ordered entries at one level with total `N > 0`:

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

This is the canonical **bulk construction / canonical rewrite** grouping rule.

It is applied independently to the leaf level and then repeatedly to each internal level until one root remains.

### 15.3 Proposed boundary examples

Leaf `C=254`, `M=127`:

| N | Groups |
|---:|---|
| 1 | `1` |
| 253 | `253` |
| 254 | `254` |
| 255 | `128, 127` |
| 380 | `253, 127` |
| 381 | `254, 127` |
| 508 | `254, 254` |
| 509 | `254, 128, 127` |

Internal `C=226`, `M=113`:

| N | Groups |
|---:|---|
| 2 | root `2` |
| 225 | root `225` |
| 226 | root `226` |
| 227 | `114, 113` beneath a new root |
| 338 | `225, 113` |
| 339 | `226, 113` |
| 452 | `226, 226` |
| 453 | `226, 114, 113` |

These examples are authoritative only if the proposed field widths/capacities are accepted unchanged.

## 16. Deterministic persistent insertion

Insertion is deterministic relative to the prior valid tree and requested object.

### 16.1 Routing

At an internal page, choose the child as follows:

1. if the new ID falls within one existing child inclusive range, choose that child;
2. if the ID lies in a gap, choose the first child whose maximum ID is greater than the new ID;
3. if the ID is greater than every child maximum, choose the final child.

This rule is equivalent to routing the ID to its ordered insertion position while preserving sparse key ranges.

### 16.2 Leaf insertion

- Reject an all-zero ID.
- Reject a duplicate active ID.
- Insert the new locator in sorted order.
- If the leaf does not overflow, emit one replacement leaf.
- On overflow from `C` to `C+1`, split:

```text
left_count  = ceil((C + 1) / 2)
right_count = floor((C + 1) / 2)
```

For the proposed leaf capacity 254, overflow produces 255 entries split `128, 127`.

### 16.3 Internal propagation

Replace the changed child reference with the one or two result references.

If the parent remains within capacity, emit one replacement internal page.

If it overflows, use the same split formula on ordered child references.

For proposed internal fanout 226, overflow produces 227 child references split `114, 113`.

### 16.4 Root growth

If the old root splits, create one new internal root with exactly the two result children and level `old_root_level + 1`.

## 17. Deterministic persistent deletion

Deletion is deterministic relative to the prior canonical-occupancy-valid tree and requested object ID.

### 17.1 Preconditions

- ID is non-zero;
- ID exists exactly once in the active directory;
- active object count is greater than one;
- input tree passes strict validation and occupancy validation.

### 17.2 Delete path

Remove the target locator from its leaf.

If a non-root changed page remains at or above minimum occupancy, emit the changed page and propagate the new reference upward.

### 17.3 Underflow repair

For a non-root underfull page:

1. borrow one final entry/child from the left sibling if the left sibling remains at or above minimum after borrowing;
2. otherwise borrow one first entry/child from the right sibling under the same rule;
3. otherwise merge with the left sibling when a left sibling exists;
4. otherwise merge with the right sibling.

Borrow and merge preserve global key ordering.

Any sibling whose bytes change is re-emitted.

### 17.4 Recursive repair

When merge removes a child from an internal parent, apply the same minimum-occupancy repair recursively.

### 17.5 Root collapse

If an internal root is left with exactly one child, the child becomes the new root.

## 18. Batch semantics

A batch consists of `Put` and `Delete` operations.

### 18.1 Identifier rules

- Each operation identifies exactly one non-zero `ObjectId`.
- At most one operation per ID is permitted.
- Duplicate operation IDs are rejected rather than resolved by caller order.

### 18.2 Semantic classification

Against the original active state:

- `Put(existing_id)` is a replacement;
- `Put(absent_id)` is an insertion;
- `Delete(existing_id)` is a deletion;
- `Delete(absent_id)` is an error.

### 18.3 Caller-order independence

The caller-provided operation order must not affect the semantic result or bytes produced by the specified deterministic batch algorithm.

Implementations canonicalize batch operations by `ObjectId` before byte-significant planning.

### 18.4 Shared-path writes

One batch computes one next complete snapshot. Shared changed ancestors are emitted once for the selected deterministic transition algorithm.

## 19. Canonical bulk identity versus persistent transition identity

This Draft explicitly distinguishes two deterministic identities rather than requiring every update history to collapse to one global page partition.

### 19.1 Canonical bulk/rewrite form

Given the same ordered active object records/locators and the same epoch-level metadata, canonical bulk construction uses Section 15 grouping and produces one deterministic fresh-tree layout.

A canonical rewrite/compaction operation may use this form to provide reproducible state reissuance.

### 19.2 Persistent transition form

Normal append mutation is deterministic from:

- the exact prior valid snapshot/tree;
- the canonicalized operation batch;
- the EXP-0003 mutation rules.

It may preserve historical pages whose exact bodies remain valid and therefore need not produce the same page partition/root digest as a fresh canonical bulk rewrite of the resulting logical active state.

### 19.3 Consequence

EXP-0003 structural snapshot/root identity is intentionally **history-sensitive** under persistent mutation.

The format does not claim that equal logical active object sets necessarily have equal root digests across different update histories.

Applications/profiles requiring a history-independent logical-state identity must define one separately or use canonical rewrite output under an appropriate profile rule.

### 19.4 Rationale

Requiring globally canonical page partition after every mutation can force broad repartitioning and undermine the core Phase 3 reason for immutable page reuse. Scoped determinism preserves both reproducibility and efficient persistent updates without pretending structural identity is semantic identity.

This section is a proposed resolution of the FCP-0003 canonical-final-state question and requires explicit Review approval.

## 20. Snapshot publication

A writer may report a new snapshot/commit as successfully constructed only after all bytes needed by that commit have been generated and the footer digest can be finalized.

The wire format itself does not define operating-system durability.

A storage implementation must distinguish constructed bytes from durable publication according to its qualified platform contract.

## 21. Strict active-state validation

A strict validator validates one exact-end prefix and never invokes recovery.

It must enforce all required checks before returning success. Implementations may reject unsafe input earlier than this conceptual order.

Conceptual validation sequence:

1. enforce configured file/read/request/allocation/object/page/depth/hash limits before unsafe work;
2. validate bootstrap magic/reserved bytes;
3. require exact-end footer;
4. validate footer fixed fields/reserved bytes and in-bounds snapshot range;
5. authenticate/parse snapshot;
6. validate genesis or parent-link fields required by the active commit;
7. recompute current commit digest;
8. derive the authenticated root page reference;
9. recursively authenticate every reachable directory page;
10. reject page cycles or forbidden duplicate structural references;
11. validate page kind/level/header length/entry width/count/ranges/padding;
12. enforce root/non-root occupancy;
13. validate internal child ordering/ranges and leaf locator ordering;
14. collect active locators in strict `ObjectId` order;
15. reject forbidden physical overlap among active object ranges and structural ranges;
16. parse/cross-check every active object header against its locator;
17. recompute every active object digest;
18. return verified active state only after all required checks succeed.

A malformed file must not cause unchecked arithmetic, out-of-bounds access, or allocation based solely on untrusted declared lengths.

## 22. Targeted authenticated lookup and absence

Targeted lookup has a deliberately narrower assurance scope than full validation.

It verifies at least:

1. bootstrap, exact-end footer, snapshot, current commit identity, and required active linkage facts;
2. one authenticated root-to-leaf page path;
3. sufficient leaf ordering/range facts to establish selected entry or authenticated absence;
4. if present, selected object header, locator cross-check, range, and digest.

It does not claim that unrelated pages or payload objects were rehashed.

An absence result is authenticated only when the validated leaf/range ordering proves no matching ID can be present in the active tree.

## 23. Verified linked history

Linked-history verification is a separate requested assurance mode.

Starting from a strict active prefix, follow `previous_footer_offset` toward genesis.

For each ancestor:

- ancestor footer physically precedes its child;
- sequence decrements exactly by one;
- child `parent_snapshot_digest` equals the ancestor snapshot digest;
- ancestor prefix ending after its footer passes complete strict validation;
- cumulative source/read/hash/depth budgets are enforced;
- footer cycles or repeated ancestor offsets are rejected.

A newest active snapshot may be strictly valid even when older linked history is corrupt; in that case strict active validation can succeed while verified-history validation fails.

## 24. Recovery

Recovery is explicit, report-only, and independently bounded.

Footer magic is a discovery hint, not authority.

A recovery operation may scan a caller-bounded suffix for candidate footer magic. For each candidate offset it chooses to evaluate, it validates the exact prefix ending after that footer using complete strict validation.

Recovery may report only prefixes that pass strict validation.

Recovery must not:

- silently replace the exact-end active state;
- choose one candidate as authorized/freshest merely by physical recency;
- update a trusted freshness checkpoint;
- treat an invalid near-match as a valid partial commit.

Budgets should independently bound suffix bytes, scan operations, magic matches, candidate validations, returned candidates, cumulative source bytes, allocation, and linked-history work.

## 25. Repair and rewrite

Repair/rewrite operations accept only sources that satisfy the assurance preconditions promised by the API/profile.

A rewrite that emits different bytes creates new byte and commit identity.

### 25.1 Canonical rewrite

A canonical active-state rewrite emits a new genesis file using:

- selected active objects in `ObjectId` order;
- canonical object emission rules;
- Section 15 bulk tree grouping;
- sequence zero;
- zero parent digest;
- genesis previous-footer sentinel.

### 25.2 Selected rewrite

A selected rewrite emits only caller-selected active objects after verifying that every selected ID exists and after applying any profile-required dependency/preservation rules.

The core does not claim semantic completeness from caller selection alone.

## 26. Semantic compaction boundary

The core understands structural references needed to validate the UCOF container.

It does not inspect arbitrary opaque payloads to discover application dependencies.

A semantic compaction profile must define:

- trusted/root objects;
- dependency extraction for every interpreted object kind;
- maximum dependency counts/depth/work;
- missing-dependency behavior;
- cycle behavior;
- unknown-semantics policy;
- extension/provenance/signature preservation or reissuance rules;
- history-retention policy.

Unknown semantic behavior must either fail closed or trigger an explicit conservative retention policy. Retaining only the unknown object is not generally conservative because it may reference otherwise unselected objects.

## 27. Capability and extension behavior

The exact EXP-0003 catalog/profile declaration encoding remains an open Draft field-table item.

Whatever encoding is selected must preserve these semantics:

- unknown **required** capability blocks safe interpretation requiring that capability;
- unknown **optional** capability may be skipped where the operation does not need it;
- structural/integrity evidence should remain reportable where safe even when interpretation is unsupported;
- rewrite APIs that promise optional-extension preservation must preserve unknown bytes exactly or reject the operation;
- implementations must not silently drop unknown required data during rewrite/compaction.

Before FCP-0003 moves to Review, the exact authenticated location and canonical encoding of catalog roots/capabilities/extensions must be specified or explicitly narrowed out of the epoch.

## 28. Remote source contract

An assurance operation over a mutable remote object must be bound to one strong non-ABA source version.

A conforming remote adapter must ensure that every accepted metadata/range operation belongs to that same source version or terminate the operation.

Stable source view does not imply freshness.

The wire format does not prescribe HTTP/cloud APIs, but implementations claiming remote support must expose limits and failures distinctly enough to separate:

- version change;
- cancellation;
- deadline;
- transient transport failure;
- authentication/authorization failure;
- malformed or contradictory range response;
- resource limit exhaustion.

Maintained adapter qualification is a Phase 3 implementation exit gate, not a byte-validity rule.

## 29. Freshness and rollback resistance

EXP-0003 provides authenticated sequence, snapshot, commit, and ancestry evidence.

It does not establish that the current file is the newest authorized state.

Applications requiring rollback/fork resistance need a protected trusted checkpoint and explicit initial-pin/advance authorization policy as described in `docs/security/FRESHNESS_CHECKPOINT_AUTHORIZATION.md`.

Integrity verification must not silently establish or advance application trust.

## 30. Unknown/trailing bytes

Strict active validation requires the footer at exact end. Therefore arbitrary trailing bytes after a valid footer make the larger prefix not strictly valid.

Recovery may separately discover the earlier valid prefix.

This rule is intentional: strict active validity and recovery evidence must not collapse into one ambiguous open behavior.

## 31. Resource-conformance requirements

A production-quality EXP-0003 parser must permit caller-controlled limits for relevant work, including:

- source bytes read;
- individual request size;
- number of requests;
- bytes hashed;
- object count;
- page count;
- page depth;
- history depth;
- recovery scan bytes/attempts/results;
- single allocation;
- cumulative allocation where tracked;
- diagnostic count;
- output size for writer/rewrite operations.

A resource-policy refusal is not proof that the file is malformed.

No required parser path may depend on native pointer width or host endianness for wire interpretation.

## 32. Malformed conditions

At minimum, strict validation rejects:

- wrong magic or fixed lengths;
- non-zero reserved/unsupported flag fields;
- all-zero object IDs where an object ID is required;
- zero object kind;
- logical/stored length mismatch in this epoch;
- range arithmetic overflow/out-of-bounds ranges;
- duplicate or unordered active IDs;
- invalid page level/kind/entry width;
- count beyond derived page capacity;
- under-minimum non-root occupancy;
- internal root with fewer than two children;
- invalid page padding;
- child ranges out of order or overlapping;
- parent/child range mismatch;
- page digest mismatch;
- object locator/header mismatch;
- object digest mismatch;
- forbidden physical overlap;
- page cycles/repeated structural references where prohibited;
- snapshot digest mismatch;
- footer/snapshot fixed-field contradiction;
- commit digest mismatch;
- invalid parent sequence/snapshot linkage when verifying history;
- unsupported required capabilities.

## 33. Writer determinism requirements

Where this specification defines a deterministic writer algorithm, conforming implementations must reproduce the same bytes given the same complete byte-significant inputs and prior snapshot.

Determinism must not depend on:

- caller operation order where operations are defined as a set;
- hash-map iteration order;
- host endianness;
- pointer width;
- thread scheduling;
- temporary spill run size;
- merge fan-in;
- temporary filenames;
- encrypted-spill nonces;
- allocation strategy.

Persistent transition identity may depend on the exact previous tree because historical page reuse is an explicit input to that transition.

## 34. Authoritative interoperability corpus required before allocation

Before `UCOF-EXP-0003` is allocated, publish authoritative valid cases for at least:

1. smallest valid genesis;
2. leaf capacity minus one/exact/plus one;
3. multi-leaf genesis;
4. internal fanout boundaries;
5. replacement with exact page reuse;
6. insertion without split;
7. leaf split;
8. internal split/root growth;
9. deletion without underflow;
10. left borrow;
11. right borrow;
12. merge;
13. recursive underflow/root collapse;
14. canonicalized mixed batch;
15. linked history;
16. interrupted append with older valid prefix;
17. recovery with multiple validated candidates;
18. unknown optional capability behavior;
19. unknown required capability behavior;
20. canonical selected rewrite;
21. at least one profile-defined semantic compaction example.

Publish invalid/adversarial cases for all major malformed conditions in Section 32 and around every publication/occupancy boundary.

Each authoritative case must pin:

- exact byte length;
- SHA-256 of complete file bytes;
- expected active sequence;
- object inventory;
- root level/page counts;
- expected strict/lookup/history/recovery outcome as relevant;
- generation recipe or annotated layout.

## 35. Independent implementation requirement

Before Phase 3 exit, at least one separately maintained implementation or documented external clean-room implementation/review must independently interpret this specification and reproduce/cross-accept authoritative EXP-0003 evidence.

A second implementation maintained in the same repository by the same authors is valuable differential evidence but does not alone satisfy the independence gate.

Every disagreement must be recorded before either side is changed merely to match the other.

## 36. Production publication boundary

EXP-0003 byte conformance defines complete exact-end commit validity but does not define a universal filesystem durability mechanism.

Production writer/storage implementations should satisfy the separate requirements in `docs/security/PHASE_3_PRODUCTION_SPILL_REQUIREMENTS.md` or document an equivalent qualified contract.

A format-valid byte sequence is not automatically durably published.

## 37. Compatibility

EXP-0003 is intentionally incompatible with EXP-0001 and EXP-0002.

Readers must reject an unrecognized experimental epoch rather than guess another epoch layout.

No migration support is promised for disposable experimental files.

Acceptance of EXP-0003 would establish only an interoperability experiment, not UCOF 1.0 compatibility.

A later incompatible experimental byte change requires a new experimental epoch.

## 38. Open Draft decisions

Before FCP-0003 moves to Review, resolve at least:

1. whether the proposed 128-bit `ObjectId` is accepted;
2. whether the 64-byte primary locator is accepted;
3. whether the 80-byte page header is accepted;
4. whether the 72-byte internal child reference is accepted;
5. whether 16 KiB remains the page size;
6. whether the resulting 254/226 capacities are accepted;
7. whether half-full occupancy and the exact split/delete rules are accepted;
8. whether the non-empty active-tree rule is accepted;
9. whether Section 19's history-sensitive persistent root identity is accepted as the resolution of the canonical-final-state question;
10. exact catalog/root/capability/extension encoding or explicit removal from EXP-0003 scope;
11. whether SHA-256 remains the only mandatory digest for this epoch;
12. exact object-kind namespace policy for experimental vectors;
13. which invalid conditions are malformed versus structurally valid-but-unsupported capability states.

## 39. Review rule

Nothing in this Draft becomes authoritative because reference code already behaves similarly.

The direction of authority is:

```text
accepted FCP/specification rule
    -> authoritative vector
        -> reference implementation
        -> independent implementation comparison
```

Implementation findings may and should cause the proposal/specification to change before epoch allocation, but undocumented implementation behavior is never silently normative.
