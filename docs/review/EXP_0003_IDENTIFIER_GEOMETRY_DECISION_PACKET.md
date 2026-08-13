# EXP-0003 Identifier and Primary-Directory Geometry Decision Packet

**Status:** maintainer-review packet; recommendation only  
**Date:** 2026-08-13  
**Target:** FCP-0003 Draft and `spec/experimental/UCOF-EXP-0003.md`  
**Evidence:** Experiments 0010, 0107–0109, 0135–0137  
**Tracking:** issues #13, #16, #76; catalog PR #79

## Purpose

The first self-contained EXP-0003 Draft intentionally fixed concrete bytes early enough to expose disagreements. It currently proposes:

```text
ObjectId              16 bytes
object header         64 bytes
page header           80 bytes
leaf locator          64 bytes
internal child ref    72 bytes
leaf capacity/min    254 / 127
internal fanout/min  226 / 113
```

Those values have now been tested from four different directions:

1. page-size and locator-density evidence;
2. identifier-width and fixed-header cost;
3. ObjectId namespace semantics;
4. internal-range and targeted-lookup assurance behavior;
5. fixed-header reserve versus alignment.

The geometry evidence is mature enough to stop adding variants and present one explicit maintainer decision.

This packet recommends revising the Draft to a **tight 64-bit local structural ObjectId geometry with explicit child minimum + maximum ranges**.

It does **not** make that decision. Merging this packet means only that the decision surface is ready for Review.

## Recommendation in one block

Recommended EXP-0003 Review candidate:

```text
ObjectId width                 8 opaque bytes
ObjectId ordering              unsigned lexicographic byte order
all-zero ObjectId              invalid/reserved
ObjectId scope                 container-context structural lookup key
core no-remap merge promise    none
object header                  40 bytes
page size                      16,384 bytes
page header                    40 bytes
leaf locator                   56 bytes
internal child reference       56 bytes, explicit min + max
leaf capacity                  291
leaf minimum occupancy         146
internal fanout                291
internal minimum occupancy     146
leaf overflow split            292 -> 146,146
internal overflow split        292 -> 146,146
```

The recommendation keeps SHA-256 object/page authentication, fixed-size pages, exact object offset/length in the leaf locator, and explicit child minimum/maximum ranges unchanged in concept.

## Decision 1 — ObjectId semantic scope

### Recommended contract

`ObjectId` should be defined as a **container-context structural lookup key**.

Normative intent if accepted:

1. The field is exactly eight opaque bytes.
2. The all-zero value is invalid/reserved.
3. Canonical order is unsigned lexicographic byte order over those eight bytes.
4. The field is not an arithmetic integer type even though it has 64 bits of namespace.
5. Every active primary-directory key is unique in that active snapshot.
6. `Put(existing_id)` continues to mean replacement of the active structural slot identified by that key.
7. Core does not claim that the key is a UUID, content identity, signer identity, globally unique semantic identity, or cross-container persistent identity.
8. Core makes no generic guarantee that independently created containers can be combined without ObjectId collision/remapping.
9. Profiles/applications needing globally portable semantic identity define that identity separately.
10. The format does not infer the writer's allocation strategy from observed key bytes.

### Why width must follow this contract

The collision model depends on how identifiers are generated.

For independently random IDs, 64-bit birthday risk becomes material at hundred-million-to-billion-object scale. That is evidence against 64-bit **if** EXP-0003 promises uncoordinated random generation and no-remap coexistence.

The proposed scope above does not make that promise.

For a coordinated output namespace, cardinality is the relevant bound instead. With `u64` physical byte offsets and the recommended 40-byte minimum object header, the entire addressable byte space can contain at most:

```text
floor((2^64 - 1) / 40)
= 461,168,601,842,738,790
```

minimum-size object records even in the impossible best case where every byte belongs to an object header.

The nonzero 64-bit keyspace contains:

```text
2^64 - 1
= 18,446,744,073,709,551,615
```

values.

So the identifier namespace is still about **40x larger than the conservative physical minimum-record bound**. Real files also contain payloads, directory pages, snapshots, footers, and historical bytes, so their achievable object count is lower.

This makes 64-bit namespace exhaustion the wrong reason to reject a coordinated local structural key.

### Cross-container combination

Width by itself does not solve local-namespace merge.

Two independent files that each allocate dense keys beginning from the same origin can have deterministic conflicts whether the field is 64 or 128 bits.

Because UCOF Core treats payloads as opaque, generic safe remapping is not always possible: application/profile data may contain semantic references that Core cannot discover or rewrite.

Therefore cross-container semantic merge belongs above the structural-key layer unless a future epoch explicitly chooses a stronger global identifier contract.

## Decision 2 — key representation and ordering

The recommended eight-byte ObjectId remains an opaque byte string.

Do **not** make the wire semantic "little-endian `u64`" merely because the current Rust research implementation often represents IDs as integers.

Recommended rule:

```text
compare ObjectId bytes from byte 0 through byte 7 as unsigned octets
```

This is equivalent to ordering an unsigned 64-bit value encoded big-endian, but implementations should treat it as fixed bytes rather than doing arithmetic on it.

Benefits:

- preserves the first Draft's opaque-key abstraction;
- makes byte ordering language-neutral;
- avoids host integer/endian leakage;
- allows random, sequential, structured, or externally assigned byte keys without changing the parser;
- makes the research `u64` representation implementation evidence rather than normative inertia.

## Decision 3 — object header

### Recommended exact 40-byte layout

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Epoch object magic |
| 8 | 2 | Header length, exactly 40 |
| 10 | 2 | Non-zero object kind |
| 12 | 4 | Flags; zero unless this epoch assigns bits |
| 16 | 8 | Non-zero opaque `ObjectId` |
| 24 | 8 | Stored payload length |
| 32 | 8 | Logical payload length |
| 40 | variable | Exact stored payload bytes |

These fields consume exactly 40 bytes. No object-header reserve is required.

Transforms/compression are outside EXP-0003, so the Draft can continue requiring:

```text
logical_payload_length == stored_payload_length
```

A future incompatible epoch can add the transform grammar it actually needs rather than charging every EXP-0003 object for undefined future fields.

### Why per-object reserve should be removed

The prior `compact-64` experiment still reserved eight bytes in each 48-byte object header.

Those eight bytes cost:

```text
800,000,000 bytes at 100 million objects
8,000,000,000 bytes at one billion objects
```

No accepted EXP-0003 field currently uses them.

The experiment is disposable and epoch-scoped; round-number reserve is not free forward compatibility.

## Decision 4 — page header

### Recommended exact 40-byte layout

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Epoch page magic |
| 8 | 1 | Page kind: leaf/internal |
| 9 | 1 | Page level; leaf is zero |
| 10 | 2 | Header length, exactly 40 |
| 12 | 4 | Entry count |
| 16 | 2 | Entry width |
| 18 | 2 | Flags; zero in this epoch unless assigned |
| 20 | 8 | Minimum `ObjectId` |
| 28 | 8 | Maximum `ObjectId` |
| 36 | 4 | Zero alignment bytes |
| 40 | variable | Packed fixed-width entries |

The semantic fields consume 36 bytes. Four required-zero bytes keep the first fixed-width entry 8-byte aligned.

These four bytes are alignment filler, not a generic extension promise.

### Why page reserve is different from object reserve

A page is always exactly 16 KiB.

Reducing the page header from 48 to 40 bytes does not change the tight-64 capacity plateau. It merely moves zero bytes from the header to the authenticated tail padding.

Therefore the 40-byte choice is primarily about the smallest clear aligned grammar, not a claim of eight bytes saved per page.

## Decision 5 — leaf locator

Recommended leaf locator: **56 bytes**.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | `ObjectId` |
| 8 | 8 | Absolute object-record offset |
| 16 | 8 | Exact object-record length |
| 24 | 32 | Object-record SHA-256 digest |

Keep `record_length` in the locator.

Although removing it could shrink the entry further, retaining exact length lets a bounded/random-access implementation know the authenticated range it must fetch before parsing the object header. That is valuable for remote range planning and pre-allocation bounds.

Do not mirror object kind, logical length, schema, transform, or profile metadata in the primary locator. Optional authenticated inventory/index structures can serve broad-inventory workloads without permanently expanding every primary key.

## Decision 6 — internal child reference

Recommended child reference: **56 bytes with explicit minimum and maximum bounds**.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Child minimum `ObjectId` |
| 8 | 8 | Child maximum `ObjectId` |
| 16 | 8 | Absolute child-page offset |
| 24 | 32 | Child-page SHA-256 digest |

Child length remains implicit because every page is exactly 16 KiB.

Child level remains implicit as parent level minus one.

### Why retain both bounds

Experiment 0136 showed that max-only child entries are structurally possible and increase fanout, but the gain is not free.

With explicit ranges, an authenticated internal page can:

- reject overlapping declared sibling ranges locally;
- prove an inter-child gap absence without reading another child page.

With max-only entries, strict recursive validation can reconstruct omitted minima, but a targeted query in a gap may need one additional authenticated child page to discover that the query precedes the child's actual minimum.

At 10M–1B objects, max-only saved only about 0.05% of directory bytes in the 64-bit compact model while changing that targeted remote-read behavior.

The first interoperability epoch should prefer the clearer authenticated range contract and leave max-only routing as an explicitly documented future incompatible optimization if real scale/provider evidence justifies it.

## Decision 7 — derived page geometry

With:

```text
PAGE_SIZE = 16,384
PAGE_HEADER_LEN = 40
LEAF_ENTRY_LEN = 56
INTERNAL_ENTRY_LEN = 56
```

both page kinds derive the same capacity:

```text
floor((16,384 - 40) / 56)
= floor(16,344 / 56)
= 291
```

Tail padding on a full page is:

```text
16,344 - 291 * 56 = 48 bytes
```

The 48 bytes remain zero and authenticated as part of the fixed page.

### Half-full occupancy

Keeping the existing half-full non-root policy:

```text
C = 291
M = ceil(C / 2) = 146
```

Therefore both leaf and internal non-root pages use:

```text
146..291 entries/children
```

Root exceptions remain separately specified.

### Overflow split

Both page kinds overflow from 291 to 292 entries/children.

Using the existing split rule:

```text
left  = ceil(292 / 2) = 146
right = floor(292 / 2) = 146
```

So both leaf and internal overflow use exactly:

```text
292 -> 146,146
```

This symmetry removes several distinct boundary constants from the independent-implementation burden.

## Boundary examples if accepted

For canonical bulk grouping with `C=291`, `M=146`:

| N | Groups |
|---:|---|
| 1 | root `1` where page-kind root rules allow it |
| 290 | `290` |
| 291 | `291` |
| 292 | `146,146` |
| 436 | `290,146` |
| 437 | `291,146` |
| 582 | `291,291` |
| 583 | `291,146,146` |

The exact root interpretation differs for leaf versus internal levels, but the non-root grouping arithmetic is shared.

Issue #16 should regenerate the full authoritative boundary matrix from these values only after maintainer disposition.

## Structural-density comparison

Using full child ranges and 16 KiB pages, Experiment 0137 gives at 100 million objects:

| Candidate | Directory bytes | Object-header bytes | Combined structural bytes | Bytes/object |
|---|---:|---:|---:|---:|
| first Draft 128-bit | 6,479,101,952 | 6,400,000,000 | 12,879,101,952 | 128.791 |
| tight 128-bit | 6,453,690,368 | 4,800,000,000 | 11,253,690,368 | 112.537 |
| tight 64-bit | 5,649,694,720 | 4,000,000,000 | 9,649,694,720 | 96.497 |

Consequences at 100 million objects:

```text
tight-64 vs first Draft saving:
  3,229,407,232 bytes
  ~25.1% of modeled structural bytes

tight-64 vs tight-128 saving:
  1,603,995,648 bytes
  ~16.04 bytes/object
  ~14.25% of tight-128 modeled structural bytes
```

At one billion objects:

```text
tight-128 = 112,536,576,000 structural bytes
tight-64  =  96,496,603,136 structural bytes
```

The density difference remains material even when tree path depth is the same.

## Interoperability simplification

The tight-64 candidate has a property the tight-128 candidate does not:

```text
object header       40
page header         40
leaf entry          56
internal entry      56
leaf capacity      291
internal fanout    291
leaf minimum       146
internal minimum   146
leaf overflow      292 -> 146,146
internal overflow  292 -> 146,146
```

That reduces:

- distinct fixed widths;
- distinct page-capacity constants;
- distinct minimum-occupancy constants;
- distinct overflow boundary recipes;
- opportunities for independent implementations to accidentally apply leaf constants to internal pages or vice versa.

This is a real interoperability benefit in a disposable epoch whose purpose is to obtain independent reproduction.

## Tight 128-bit alternative

If maintainers decide that Core should intentionally support uncoordinated identifier generation/no-remap coexistence, the recommended alternative is **tight 128-bit**, not the current round-number first Draft.

Alternative geometry:

```text
ObjectId             16 opaque bytes
object header        48
page header          56
leaf locator         64
internal ref         72, explicit min + max
leaf cap/min         255 / 128
internal cap/min     226 / 113
leaf overflow        256 -> 128,128
internal overflow    227 -> 114,113
```

This retains the larger random namespace while still removing avoidable per-object reserve.

The question for maintainers is therefore not "old Draft versus risky compression." It is:

> Does the mandatory structural key need the stronger distributed-generation/no-remap namespace contract enough to justify its permanent density and geometry complexity cost?

## Catalog proposal consequence

PR #79 currently proposes a mandatory snapshot-selected catalog using a 16-byte ObjectId and a 112-byte snapshot.

If this packet's 8-byte ObjectId recommendation is accepted **and** the catalog proposal is later accepted, its snapshot can naturally become:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Snapshot magic |
| 8 | 8 | Sequence |
| 16 | 8 | Directory root page offset |
| 24 | 1 | Directory root level |
| 25 | 7 | Zero reserved/alignment bytes |
| 32 | 32 | Directory root page digest |
| 64 | 8 | Catalog `ObjectId` |
| 72 | 32 | Parent snapshot digest |
| total | **104** | |

Catalog root-object IDs would also become eight-byte structural keys.

This packet does not accept that snapshot grammar. It records the direct consequence so #79 can be rebased once identifier geometry is dispositioned rather than moving the catalog byte table twice.

## Catalog/empty-application-tree follow-up

A separate catalog review issue remains important and should not be smuggled into the width decision.

If EXP-0003 makes the catalog mandatory, the physical primary directory is inherently non-empty because the current snapshot must always reach its catalog object.

That creates a promising distinction:

```text
physical tree non-empty
application root set may be empty
```

A later #79 revision should evaluate:

- `root_count == 0` as a valid catalog state;
- catalog-only directory as the representation of zero application roots;
- snapshot-selected catalog ObjectId lifecycle across replacement;
- whether the current "cannot delete final active application object" rule should become "the current catalog must remain reachable/replaced atomically."

Those semantics are independent of whether ObjectId is 8 or 16 bytes and remain an open catalog decision.

## ObjectId reuse after deletion

This packet does **not** add a never-reuse rule.

Reasons:

- current mutation semantics treat ObjectId as an active structural slot, not immutable global object identity;
- enforcing lineage-wide never-reuse after compaction/rewrite would require additional allocator/history state or a stronger retained-history contract;
- profiles needing durable semantic identity should not infer it from the structural lookup key.

A writer must still reject duplicate IDs in one active primary directory.

An allocator may choose monotonic/non-reuse policy as an implementation/profile convention, but EXP-0003 Core should not make it byte-semantic unless Review identifies a concrete requirement.

## Security boundary

Changing key width does not weaken SHA-256 object/page integrity because ObjectId is not the digest.

Regardless of width:

- duplicate active IDs are invalid;
- locators authenticate complete object records;
- child references authenticate complete child pages;
- strict validation checks ranges and cross-references;
- random-generation collision policy remains the writer/application's responsibility unless Core explicitly promises a generation scheme;
- authenticity, freshness, provenance, authorization, and confidentiality remain separate claims.

A malicious duplicate key is a structural validity error, not a probabilistic event to tolerate.

## Research-byte mismatch

The consolidated Rust research implementation is implementation evidence, not the accepted EXP-0003 wire format.

If the recommendation is adopted, the reference implementation must be migrated to the exact accepted byte tables and opaque lexicographic key semantics before authoritative EXP-0003 vectors are generated.

Existing research identities remain historical/non-authoritative.

Do not preserve current bytes merely to reduce implementation churn; the purpose of the pre-allocation Draft is to remove that inertia.

## Proposed maintainer disposition

Leave exactly one of these selected in the committed disposition record:

- [ ] **Adopt tight 64-bit local structural geometry**: 8-byte opaque ObjectId; object/page headers 40/40; leaf/internal entries 56/56; full child ranges; `C=291`, `M=146` for both page kinds.
- [ ] **Adopt tight 128-bit geometry**: 16-byte opaque ObjectId; object/page headers 48/56; leaf/internal entries 64/72; full child ranges; leaf `255/128`, internal `226/113`.
- [ ] **Defer geometry disposition** pending one named blocking requirement/measurement: ____________________.

**Packet recommendation:** the first option.

No checkbox is selected by this packet.

## If tight 64-bit is selected

The next normative edit should be one coordinated amendment, not piecemeal changes:

1. update FCP-0003 identifier scope/rationale;
2. update `spec/experimental/UCOF-EXP-0003.md` ObjectId width/order and exact field tables;
3. update object/page/leaf/internal constants;
4. update occupancy companion to `C=291`, `M=146` for both page kinds;
5. update split/bulk boundary examples;
6. rebase #79 catalog/snapshot/root-ID widths against the accepted key width;
7. disposition the deletion borrower rule separately from the already-merged deletion decision packet;
8. settle catalog/empty-application-root semantics;
9. settle remaining digest-domain/object-kind/scoped-determinism Review items;
10. only then generate the first authoritative EXP-0003 corpus;
11. migrate Rust generation/validation to those exact accepted bytes;
12. obtain independent reproduction against the same corpus.

## What this packet resolves and does not resolve

### Ready for maintainer decision

- local structural ObjectId scope versus stronger global/no-remap scope;
- 8 versus 16 byte ObjectId;
- object-header reserve;
- aligned page-header size;
- leaf locator width;
- explicit full child-range width;
- resulting capacities/minima/split boundaries.

### Still separate

- deletion borrower preference (decision packet recommends fuller eligible sibling with left tie-break; maintainer disposition still required);
- catalog/capability/extension grammar (#79);
- application-root-empty/catalog lifecycle semantics;
- exact digest/domain/object-kind final policy;
- Candidate 1/FCP-0002 disposition;
- FCP-0003 acceptance/allocation;
- authoritative vector generation;
- production HTTP/cloud, spill/publication, and independent implementation exit gates.

## Boundary

This document is a Review decision packet only.

It does **not**:

- change the EXP-0003 Draft wire bytes;
- select 64-bit identifiers by itself;
- accept FCP-0003;
- allocate `UCOF-EXP-0003`;
- select the deletion borrower rule;
- accept catalog PR #79;
- change current Rust research bytes;
- regenerate or promote authoritative vectors;
- claim stable compatibility or production readiness.

Its purpose is to give maintainers one bounded, evidence-backed choice so identifier/primary-directory geometry stops blocking the rest of EXP-0003 convergence.
