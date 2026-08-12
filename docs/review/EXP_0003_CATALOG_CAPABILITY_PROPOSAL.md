# EXP-0003 Catalog, Roots, Capabilities, and Extension Binding Proposal

**Status:** Draft amendment proposal for FCP-0003 review  
**Date:** 2026-08-13  
**Target:** `spec/experimental/UCOF-EXP-0003.md`  
**Evidence:** Experiments 0020 and 0034  
**Tracking:** issues #13 and #76

## Purpose

The first self-contained EXP-0003 Draft deliberately left one major byte-layout question unresolved: how the active snapshot authenticates application roots, required/optional capabilities, and preservable future metadata without bloating every primary directory entry or creating unauthenticated side channels.

This proposal closes that gap with a small structural rule:

> Every EXP-0003 snapshot names exactly one ordinary immutable **catalog object** by `ObjectId`. The primary directory authenticates and locates that object. The catalog payload canonically lists application roots, capabilities, and extension records.

This proposal does not allocate EXP-0003, accept FCP-0003, or create permanent global registries. The identifiers/kinds/tags below are scoped to this disposable epoch.

## Why bind the catalog by ObjectId

The previous metadata experiment reserved one magic object identifier for the catalog. That proved the semantics but is not desirable for a 128-bit opaque application namespace.

Naming the catalog explicitly in the snapshot has several advantages:

- no `ObjectId` value is stolen from the application namespace;
- the catalog remains an ordinary immutable object with the same object/page authentication as every other object;
- unrelated persistent updates can reuse the exact historical catalog object and its directory path where unchanged;
- changing catalog metadata naturally creates a new catalog object record and new affected directory path;
- the snapshot says unambiguously which active object supplies the catalog, so there is no scan for a special ID;
- catalog physical location stays out of the snapshot and remains authenticated by the primary directory locator;
- the fixed snapshot remains small while variable capability/extension metadata lives in a bounded length-delimited object.

There is no circularity: the core directory format is independently parseable without catalog semantics. An implementation first authenticates the structural directory, then locates the snapshot-selected catalog object, then evaluates catalog semantics and interpretability.

## Proposed snapshot amendment

Change proposed `SNAPSHOT_LEN` from **96** to **112** bytes.

Keep proposed snapshot magic `UCSNIM03`.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCSNIM03` |
| 8 | 8 | Snapshot sequence |
| 16 | 8 | Directory root page offset |
| 24 | 1 | Directory root level |
| 25 | 7 | Zero reserved bytes |
| 32 | 32 | Directory root page digest |
| 64 | 16 | Catalog `ObjectId` |
| 80 | 32 | Parent snapshot digest; all zero for genesis |

The catalog `ObjectId` is non-zero.

Using an 8-bit root level aligns the snapshot with the proposed 8-bit page-level field. A tree requiring a level above 255 is unrepresentable in this epoch and must be rejected before publication.

The complete 112 snapshot bytes participate in `snapshot_digest`.

### Footer consequence

The footer remains 128 bytes. Its `snapshot_length` field must equal **112** for EXP-0003 rather than 96.

The footer semantics/commit-digest construction is otherwise unchanged.

## Catalog object identity and kind

The catalog is one ordinary active object whose ID equals the snapshot `catalog_object_id`.

Proposed epoch-local core object kind:

```text
CORE_KIND_CATALOG = 1
```

The catalog object header therefore has:

- `object_id == snapshot.catalog_object_id`;
- `kind == 1`;
- normal EXP-0003 object flags/reserved rules;
- `logical_length == stored_length` because transforms remain outside the epoch.

Other non-zero object kinds remain structurally opaque to the core unless another accepted EXP-0003 rule assigns them semantics. Kind `1` is an epoch-local assignment, not a permanent UCOF registry allocation.

A snapshot is invalid if the selected catalog object is absent, appears more than once, has the wrong kind, or fails normal locator/header/digest validation.

## Catalog payload

Proposed catalog magic: ASCII `UCCAT003`.

Catalog version: `1`.

The payload is:

```text
CatalogHeader
RootObjectId[root_count]
CapabilityRecord[capability_count]
ExtensionBlock
```

### Catalog header — 32 bytes

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCCAT003` |
| 8 | 2 | Catalog version, exactly 1 |
| 10 | 2 | Flags, zero in this epoch |
| 12 | 4 | Root-object count |
| 16 | 4 | Capability-record count |
| 20 | 4 | Exact extension-block byte length |
| 24 | 8 | Zero reserved bytes |

All counts and derived byte lengths are checked before allocation or range use.

EXP-0003 does not define one universal numeric maximum catalog size. Implementations must enforce caller-controlled catalog-byte, root-count, capability-count, extension-record, and allocation limits. A policy refusal is distinct from malformed catalog bytes.

## Root object list

Each root entry is exactly one 16-byte `ObjectId`.

Rules:

- `root_count >= 1`;
- every root ID is non-zero;
- roots are strictly increasing in canonical `ObjectId` byte order;
- duplicates are invalid;
- the catalog object's own ID must not appear as a root;
- every declared root must identify an active object in the same snapshot;
- root existence is a core catalog semantic invariant independent of whether an implementation understands the root object's application kind.

The root list expresses application/profile entry points. It does not assert that all active objects are reachable from those roots; semantic reachability requires profile/application dependency rules.

## Capability records

Each capability record is exactly 8 bytes.

| Offset within record | Size | Field |
|---:|---:|---|
| 0 | 4 | Non-zero capability identifier |
| 4 | 1 | Flags |
| 5 | 3 | Zero reserved bytes |

Defined capability flags:

```text
bit 0 = REQUIRED
bits 1..7 = zero
```

Rules:

- capability identifiers are unsigned 32-bit little-endian values;
- identifier zero is invalid;
- records are strictly increasing by identifier;
- duplicates are invalid;
- unknown flag bits are malformed;
- required/optional criticality belongs to each capability declaration.

Capability identifiers are epoch-local experimental identifiers until a future accepted registry policy says otherwise.

### Capability support outcome

An unknown **optional** capability does not invalidate structural integrity and may be reported/preserved without interpretation.

An unknown **required** capability also does not erase already-established structural/object/catalog integrity evidence, but the implementation must report that the file is **not fully interpretable** for operations requiring application semantics.

The result categories therefore remain distinct:

- structurally verified and fully interpretable for the requested operation;
- structurally verified but unsupported required capability prevents the requested interpretation;
- malformed/integrity failure;
- resource-policy refusal.

An implementation must not convert `unsupported required capability` into generic corruption.

## Extension block

The catalog always ends with exactly one canonical extension block, even when it has zero records.

Proposed extension-block magic: ASCII `UCEX0003`.

### Extension block header — 16 bytes

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCEX0003` |
| 8 | 4 | Extension-record count |
| 12 | 4 | Total extension-block byte length including this header |

`total_length` must equal the catalog header's `extension_block_length` and exactly consume the remaining catalog payload bytes.

### Extension record header — 12 bytes

| Offset within record | Size | Field |
|---:|---:|---|
| 0 | 4 | Non-zero extension tag |
| 4 | 2 | Flags |
| 6 | 2 | Zero reserved bytes |
| 8 | 4 | Payload length |
| 12 | variable | Opaque payload |
| after payload | 0..7 | Zero padding to 8-byte alignment |

Defined extension flags:

```text
bit 0 = REQUIRED
bits 1..15 = zero
```

Rules:

- extension tags are non-zero unsigned 32-bit identifiers;
- records are strictly increasing by tag;
- duplicates are invalid;
- unknown flag bits are malformed;
- payload length and padded length use checked arithmetic;
- padding bytes are zero;
- no bytes may trail the final declared record;
- the block may contain zero records, in which case its exact length is 16 bytes.

Tags are epoch-local experimental identifiers until a future accepted registry policy says otherwise.

### Extension support and preservation

Unknown optional extension records may remain opaque.

Unknown required extension records preserve structural integrity evidence but prevent any operation whose semantics require understanding that extension.

A rewrite that changes known catalog fields or known extension records and promises preservation must preserve every unknown optional extension record's exact header, payload, and zero-padding bytes unless an explicit higher-level policy authorizes dropping it.

A tool must not:

- reinterpret an unknown extension as a known tag;
- silently drop an unknown required extension;
- silently drop an unknown optional extension where its API claims preservation;
- execute code or fetch external resources merely because an extension payload requests it.

Canonical ordering and zero padding ensure that opaque optional records can be copied byte-for-byte while preserving deterministic catalog bytes.

## Catalog validation sequence

After the core has authenticated the active snapshot and primary directory, validation of the mandatory catalog proceeds conceptually as follows:

1. require non-zero snapshot catalog ID;
2. locate that ID through the authenticated active directory;
3. validate the complete catalog object record and digest using ordinary locator rules;
4. require object kind `CORE_KIND_CATALOG`;
5. parse the 32-byte catalog header under caller limits;
6. validate exact payload length from root/capability/extension section lengths;
7. validate sorted unique root IDs and require at least one root;
8. reject catalog ID as an application root;
9. validate sorted unique capability records and known flag bits;
10. parse the exact extension block, records, padding, and trailing-byte rule;
11. require every declared root ID to exist in the active directory;
12. classify unknown optional/required capabilities/extensions without erasing structural integrity evidence;
13. return catalog/root/capability/extension facts separately from the raw structural validity report.

A catalog with a missing root can be cryptographically self-consistent and still fail the core catalog semantic invariant. This is a semantic-invalid result, not a digest mismatch.

## Interaction with targeted lookup

Core targeted lookup can authenticate the requested object without first interpreting arbitrary application capabilities because page/object framing is fixed by EXP-0003 itself.

However, an application-level operation that claims profile semantics must also establish whatever catalog/capability support its profile requires.

Tools should expose this distinction rather than making every opaque-object lookup contingent on understanding all application capabilities.

## Interaction with history

Every historical snapshot names its own catalog object ID.

Verified-history validation therefore revalidates the exact catalog selected by each historical snapshot after that snapshot's structural prefix validates.

A historical catalog may reuse an older immutable object record/path or may have been replaced in that commit.

Unknown required capability in an ancestor may prevent full historical semantic interpretation while still permitting structural linked-history integrity to be reported distinctly.

## Interaction with recovery

Recovery candidate validation uses the exact snapshot-selected catalog of each candidate prefix.

A candidate is EXP-0003 structurally/catalog valid only when its catalog syntax, selected object, and root-existence invariants validate.

Recovery still does not choose which valid candidate is authorized or fresh.

## Interaction with rewrite and compaction

A canonical rewrite must emit exactly one catalog object and bind its new/reused `ObjectId` in the new snapshot.

If object IDs are preserved and the catalog bytes remain unchanged, the exact catalog object record/digest may be preserved where the rewrite's physical construction permits it. A fresh genesis rewrite will usually assign a new physical locator while preserving object ID/content identity if its policy allows.

Semantic compaction must treat every catalog-declared application root as protected unless the selected profile/rewrite policy explicitly and safely constructs a new catalog root set.

Unknown optional catalog extension preservation remains mandatory where the rewrite API promises unknown-data preservation.

## Why not put roots/capabilities directly in the snapshot

Rejected for the first Draft because:

- root/capability counts are variable;
- fixed snapshot growth would either impose permanent maxima or require another variable structure anyway;
- application roots/capabilities change more frequently and independently than structural snapshot framing;
- an ordinary authenticated catalog object reuses the existing object/page integrity machinery;
- catalog replacement can preserve unknown extension bytes without redesigning the commit footer.

The snapshot needs only the catalog object's identity key.

## Why not reserve a catalog ObjectId

Rejected for EXP-0003 because the 128-bit `ObjectId` is intended as an opaque application namespace. An explicit snapshot field provides stronger unambiguous selection without sacrificing a distinguished key value.

The catalog **kind** remains distinguished because an implementation must know which core payload codec to use after locating the snapshot-selected object.

## Why separate capabilities and extensions

Capabilities answer: **what behavior must an implementation understand to interpret this file/profile operation?**

Extensions answer: **what canonical length-delimited metadata bytes are attached to the catalog and may need opaque preservation?**

Keeping them separate avoids pretending every metadata record defines a support capability and avoids forcing opaque extension payloads into the capability registry.

A capability may define semantics for one or more known extension tags in a later accepted profile.

## Resource policy

The encoding deliberately avoids hard-coded research maxima such as 4,096 roots or 256 KiB catalog payloads.

Wire representability is bounded by field widths and total file/range validity. Operational acceptance is additionally bounded by caller policy.

Implementations must provide limits for at least:

- catalog payload bytes;
- root count;
- capability count;
- extension block bytes;
- extension record count;
- individual extension payload bytes;
- cumulative allocation/work while parsing and preserving records.

Exceeding caller policy is not proof of malformed bytes.

## Required authoritative vectors

If this proposal is accepted into EXP-0003, add at least:

1. minimal catalog with one root, zero capabilities, zero extensions;
2. multiple sorted roots;
3. known required capability;
4. unknown optional capability with structural validity preserved;
5. unknown required capability with structural integrity preserved but interpretability blocked;
6. known extension plus unknown optional extension preserved through catalog replacement;
7. unknown required extension support failure;
8. missing root with all outer object/page/snapshot/commit digests valid;
9. duplicate/unordered root;
10. duplicate/unordered/zero capability ID;
11. unknown capability flag bit;
12. duplicate/unordered/zero extension tag;
13. unknown extension flag bit;
14. non-zero extension padding;
15. truncated extension payload;
16. extension-length/catalog-length mismatch;
17. wrong catalog object kind;
18. missing snapshot-selected catalog object;
19. catalog selected as its own application root;
20. linked history where an unchanged catalog record/path is reused;
21. linked history where the catalog is replaced;
22. recovery candidate whose outer bytes verify but catalog root semantics fail.

Cross-language vectors must pin exact catalog bytes and classification outcomes, not only whole-file hashes.

## Proposed changes to the current EXP-0003 Draft

If accepted, fold this proposal into `spec/experimental/UCOF-EXP-0003.md` by:

- changing `SNAPSHOT_LEN` from 96 to 112;
- replacing the snapshot field table with the 112-byte table above;
- changing footer required `snapshot_length` from 96 to 112;
- assigning epoch-local object kind `1` to the core catalog;
- adding the catalog/capability/extension byte grammar and validation semantics above;
- removing catalog/capability/extension placement from the Draft open-decision list;
- adding the required vectors to the authoritative corpus gate.

## Review focus

Review should challenge especially:

- mandatory catalog versus optional/zero catalog;
- snapshot naming by `ObjectId` versus object digest or full locator;
- 112-byte snapshot size;
- root level narrowing from u64 to u8 plus reserved bytes;
- mandatory one-or-more application roots;
- u32 capability/tag namespaces;
- separate required flags on capabilities and extension records;
- whether unknown required extensions should block only extension-dependent operations or all profile interpretation;
- exact rewrite-preservation promise for opaque optional extension records;
- whether any core capability must be discoverable before directory traversal rather than through the catalog.
