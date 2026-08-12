# Immutable-Page Successor Microformat

**Status:** Non-normative executable research draft  
**Date:** 2026-07-30  
**Epoch allocation:** None  
**Compatibility promise:** None  
**Related:** FCP-0002, `docs/PHASE_3_SUCCESSOR_EVIDENCE.md`

## 1. Purpose and boundary

This document freezes the byte relationships currently exercised by the immutable-page successor experiments. It exists so independent tests do not need to treat Python source code as the only description of the bytes.

This document does **not**:

- allocate Candidate 2 or any stable epoch;
- amend or reinterpret `UCOF-EXP-0002` Candidate 1;
- select final identifier or locator widths;
- define permanent magic values or registry identifiers;
- provide authenticity, confidentiality, trusted freshness, or rollback resistance;
- claim production-ready spill, remote-source, repair, or compaction behavior.

Every value below is disposable research data.

## 2. Common rules

- Integers are unsigned little-endian.
- Offsets are absolute byte offsets from file start.
- All fixed-width arithmetic is checked before use.
- Unused and reserved bytes are zero.
- Object identifiers and object kinds are non-zero.
- Directory identifiers are strictly increasing.
- A footer has authority only when it occupies the exact final 128 bytes of the validated prefix.
- Recovery is a separate operation and never changes strict exact-end behavior.

## 3. Domain-separated digests

Candidate algorithms use SHA-256 with these exact research prefixes:

| Scope | Prefix bytes |
|---|---|
| Object | `UCOF-IMMUTABLE-OBJECT\0` |
| Page | `UCOF-IMMUTABLE-PAGE\0` |
| Snapshot | `UCOF-IMMUTABLE-SNAPSHOT\0` |
| Commit | `UCOF-IMMUTABLE-COMMIT\0` |

For object, page, and snapshot identity:

```text
SHA-256(domain_prefix || exact_bytes)
```

Commit identity is defined in Section 10.

## 4. Bootstrap header

The file begins with 64 bytes.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCOFIM02` |
| 8 | 56 | Zero reserved bytes |

The header is included in the genesis commit preimage.

## 5. Object record

An object record is a 48-byte header followed immediately by payload bytes.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCOBOBJ2` |
| 8 | 2 | Header length, exactly 48 |
| 10 | 2 | Non-zero object kind |
| 12 | 4 | Flags, currently zero |
| 16 | 8 | Non-zero object identifier |
| 24 | 8 | Payload length |
| 32 | 8 | Logical length |
| 40 | 8 | Zero reserved bytes |
| 48 | variable | Exact payload bytes |

Current experiments require payload length equal to logical length. The exact record length is `48 + payload_length`.

The leaf locator authenticates the complete header and payload under the object domain.

## 6. Directory page envelope

Every page is exactly 16,384 bytes. The complete page, including unused padding, is authenticated under the page domain.

### 6.1 Page header

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCPGIM02` |
| 8 | 1 | Page kind: 1 leaf, 2 internal |
| 9 | 1 | Page level; leaves use 0 |
| 10 | 2 | Zero reserved |
| 12 | 4 | Entry count |
| 16 | 4 | Entry width |
| 20 | 8 | Minimum object identifier |
| 28 | 8 | Maximum object identifier |
| 36 | 28 | Zero reserved bytes |
| 64 | variable | Packed entries |
| after entries | remainder | Zero padding through byte 16,383 |

Page identity is immutable content identity. It contains no active snapshot sequence.

## 7. Leaf entry

A leaf entry is 88 bytes.

| Offset within entry | Size | Field |
|---:|---:|---|
| 0 | 8 | Object identifier |
| 8 | 2 | Object kind |
| 10 | 6 | Zero reserved bytes |
| 16 | 8 | Object record offset |
| 24 | 8 | Exact object record length |
| 32 | 8 | Logical length |
| 40 | 32 | Object-record digest |
| 72 | 16 | Zero reserved bytes |

The page header entry width is 88. At most 185 entries fit in one page.

Leaf entries are strictly increasing by identifier. The first and last identifiers equal the page header minimum and maximum.

The 16-byte tail reserve is retained only because the current successor microformat inherited the Candidate 1 leaf shape. Experiments show it should be removed unless a concrete use is selected.

## 8. Internal entry

An internal entry is 64 bytes.

| Offset within entry | Size | Field |
|---:|---:|---|
| 0 | 8 | Child minimum identifier |
| 8 | 8 | Child maximum identifier |
| 16 | 8 | Child page offset |
| 24 | 8 | Child page length, exactly 16,384 |
| 32 | 32 | Child page digest |

The page header entry width is 64. At most 255 entries fit in one page.

Child ranges are strictly ordered and non-overlapping. Each child level is exactly parent level minus one. The first and last child ranges equal the parent page range.

## 9. Snapshot record

A snapshot is exactly 96 bytes.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCSNIM02` |
| 8 | 8 | Sequence |
| 16 | 8 | Directory root page offset |
| 24 | 8 | Directory root level encoded as `u64` |
| 32 | 32 | Directory root page digest |
| 64 | 32 | Parent snapshot digest; zero for genesis |

The complete 96 bytes are authenticated under the snapshot domain.

The current catalog experiment represents roots, capabilities, and extensions as an ordinary authenticated object rather than adding fields to this fixed snapshot record. That placement remains experimental.

## 10. Commit footer

A commit footer is exactly 128 bytes.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCFTIM02` |
| 8 | 8 | Sequence |
| 16 | 8 | Snapshot offset |
| 24 | 8 | Snapshot length, exactly 96 |
| 32 | 8 | Previous footer offset, or `u64::MAX` for genesis |
| 40 | 8 | Count of pages emitted by this commit |
| 48 | 32 | Snapshot digest |
| 80 | 32 | Commit digest |
| 112 | 16 | Zero reserved bytes |

### 10.1 Footer semantics preimage

The footer semantics value is 72 bytes:

```text
sequence                 u64
snapshot_offset          u64
snapshot_length          u64
previous_footer_offset   u64
page_count_current       u64
snapshot_digest          32 bytes
```

### 10.2 Commit digest

For genesis, current commit bytes begin at offset zero.

For a child, current commit bytes begin immediately after the previous 128-byte footer.

```text
SHA-256(
  COMMIT_DOMAIN ||
  exact_current_commit_bytes_before_footer ||
  footer_semantics
)
```

The commit digest does not rehash unchanged historical objects or pages. Their leaf and internal references authenticate those bytes individually.

## 11. Genesis publication

A genesis file is written in this order:

1. bootstrap header;
2. complete object records;
3. canonical leaf pages;
4. canonical internal pages until one root remains;
5. 96-byte snapshot;
6. 128-byte exact-end footer.

Genesis uses:

- sequence zero;
- zero parent snapshot digest;
- previous footer offset `u64::MAX`.

## 12. Append publication

A child commit appends, in order:

1. new or replacement object records;
2. new immutable leaf and internal pages required by the operation;
3. a new snapshot;
4. a new exact-end footer.

Unchanged objects and pages may be referenced at historical offsets. The new snapshot sequence is the previous sequence plus one, its parent snapshot digest equals the previous snapshot digest, and its previous-footer offset identifies the exact parent footer.

A footer publishes only when complete at exact end. Interrupted append bytes remain unpublished tail data.

## 13. Strict validation order

A strict validator should fail closed in this order, subject to bounded implementation details:

1. enforce file, read, allocation, hash, object, page, and depth policy before unsafe work;
2. validate the 64-byte bootstrap header;
3. require an exact-end 128-byte footer;
4. validate footer fixed fields, reserved bytes, and snapshot range;
5. authenticate and parse the snapshot;
6. validate genesis or parent linkage;
7. recompute the current commit digest;
8. derive the root reference from the authenticated snapshot;
9. recursively authenticate and parse every reachable page;
10. reject page cycles, repeated references, invalid levels, ranges, entry widths, ordering, and non-zero padding;
11. collect active leaf locators in strict identifier order;
12. reject object/object and object/page/snapshot/footer physical overlap;
13. parse each active object header and cross-check locator claims;
14. recompute each active object digest;
15. return a verified active result only after all required checks succeed.

An implementation may reject an unsafe file earlier. It must never accept a file that violates a later required check.

## 14. Targeted authenticated lookup

A targeted lookup verifies:

1. header, exact-end footer, snapshot, parent linkage, and current commit digest;
2. one authenticated root-to-leaf path;
3. leaf canonicality sufficient to establish the selected entry or absence;
4. the selected object header, locator agreement, physical range, and digest.

It does not claim unrelated pages or objects were rehashed.

## 15. Verified history

Verified history starts with the active exact-end footer and follows previous-footer offsets. Each ancestor is revalidated as an independent exact-end prefix.

The traversal checks:

- strictly decreasing footer offsets;
- sequence decrement by exactly one;
- parent snapshot digest agreement;
- cycle and depth limits;
- cumulative read and hash work.

Active validation and verified-history validation intentionally have different assurance scopes.

## 16. Recovery

Recovery is explicit and separately bounded. Footer magic is only a discovery hint.

For every candidate footer offset, recovery validates the prefix ending immediately after that footer using the complete strict procedure. It reports verified prefixes without selecting one as active.

Recovery limits include suffix bytes, request size, scan operations, magic matches, candidate validations, cumulative candidate reads, linked depth, and returned results.

## 17. Metadata catalog experiment

The current catalog object uses reserved object identifier `u64::MAX` and kind `0xffff` within the research microformat. Those values are not registry allocations.

Its payload carries:

- sorted root identifiers;
- sorted capability records with required criticality;
- a canonical extension block.

Unknown required capabilities prevent interpretation while leaving structural and cryptographic validation evidence intact. Unknown optional extension records are preserved byte-for-byte during known-field replacement.

The exact catalog payload is documented by Experiment 0034 and remains replaceable.

## 18. Canonical tree construction

The current deterministic bulk builder:

1. sorts unique locators by object identifier;
2. packs leaves to the fixed maximum except the final leaf;
3. emits leaves in identifier order;
4. packs each internal level to fixed maximum fanout except the final page;
5. emits levels from leaves toward the root.

The update experiments use deterministic split, merge, redistribution, and path-copy rules documented in Experiments 0023–0027. Those policies are not yet normative.

## 19. Resource and security requirements

Implementations must expose independent bounds for:

- total source bytes and reads;
- request size;
- object and page counts;
- page depth;
- bytes hashed;
- single and cumulative allocation;
- recovery scan work;
- linked-history depth;
- spill bytes, runs, merge fan-in, and output bytes;
- diagnostics.

Resource-policy refusal is not malformed-file evidence.

SHA-256 identity does not provide authenticity, confidentiality, signer trust, external freshness, or rollback resistance. Stable transport version evidence prevents mixed-version reads but does not prove the source is newest.

## 20. Current vectors

### Stored exact bytes

`tests/vectors/exp-0002-immutable/genesis-four.hex`:

- 16,886 decoded bytes;
- SHA-256 `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23`;
- four complete objects;
- one leaf root;
- one exact-end footer.

### Compact generated recipes

`tests/vectors/exp-0002-immutable/generated-recipes.json` pins:

- a parent-linked replacement append of 33,550 bytes with SHA-256 `e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc`;
- a 400-object, four-page, level-one genesis of 89,316 bytes with SHA-256 `d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70`.

Python and independent Rust generators must reproduce these identities.

### Compact invalid recipes

`tests/vectors/exp-0002-immutable-invalid/cases.json` pins thirteen deterministic malformed or interrupted cases and coarse rejection layers.

## 21. Unresolved choices before any new candidate

- final magic and epoch allocation;
- identifier width;
- leaf field set and reserve removal;
- page size and occupancy minima;
- bulk, split, merge, and redistribution policy;
- mixed-operation batch semantics;
- catalog and extension placement;
- support profiles;
- production spill and publication policy;
- remote-source and asynchronous API contract;
- repair and compaction output semantics;
- signatures, encryption, provenance, and external freshness;
- independently maintained implementation and review.

Until those choices are resolved, these bytes remain disposable research evidence.
