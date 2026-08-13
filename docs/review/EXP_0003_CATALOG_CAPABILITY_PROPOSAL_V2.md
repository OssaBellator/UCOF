# EXP-0003 Catalog, Roots, Capabilities, and Extension Binding — Review v2

**Status:** Draft amendment proposal for FCP-0003 review  
**Date:** 2026-08-13  
**Supersedes after merge:** the review shape in PR #79  
**Target:** `spec/experimental/UCOF-EXP-0003.md`  
**Evidence:** Experiments 0020, 0034, 0135–0137; identifier/geometry packet #113  
**Tracking:** issues #13, #16, #76

## Purpose

The first catalog proposal established the core architecture successfully:

> Every EXP-0003 snapshot names one ordinary immutable catalog object by `ObjectId`; the primary directory authenticates and locates that object; the catalog payload carries application roots, support requirements, and preservable extension metadata.

Review then exposed four issues that should be fixed before the proposal is folded into the epoch Draft:

1. the proposal hard-coded a 16-byte ObjectId while identifier width is still a separate Review decision;
2. requiring one-or-more application roots unnecessarily coupled application emptiness to the structurally non-empty primary tree;
3. catalog ObjectId lifecycle across linked snapshots was unspecified;
4. both capabilities and extensions had independent REQUIRED bits, creating two overlapping support-criticality mechanisms.

This v2 keeps the successful snapshot→ordinary-catalog-object architecture and resolves those four issues.

It does **not** accept FCP-0003, select ObjectId width, allocate EXP-0003, or edit the self-contained Draft yet.

## Design summary

For review, let:

```text
I = accepted EXP-0003 ObjectId width in bytes
```

The identifier/geometry decision packet currently recommends `I = 8`; its tight-128 alternative uses `I = 16`.

Catalog v2 is defined parametrically until that upstream decision is dispositioned:

- each snapshot names exactly one active catalog object by `I`-byte ObjectId;
- the catalog ID is stable across linked child snapshots;
- changing catalog metadata replaces the catalog object under the same active structural key;
- `root_count` may be zero;
- the mandatory catalog keeps the physical primary directory non-empty;
- profiles may independently require one-or-more application roots;
- capability records alone carry REQUIRED support-criticality;
- extension records are opaque canonical metadata with no independent REQUIRED bit;
- unknown extensions are preservable/ignorable at Core unless a required capability or profile semantics says understanding them is necessary;
- unknown required capabilities preserve structural integrity evidence but block the affected semantic interpretation.

## Why snapshot binding by ObjectId remains the preferred architecture

Binding by explicit ObjectId still has the advantages identified in v1:

- no globally reserved/magic ObjectId is stolen from the structural namespace;
- the catalog is authenticated through ordinary object/locator/page machinery;
- unchanged catalog bytes and paths may be reused across persistent snapshots;
- metadata replacement naturally becomes an ordinary immutable-object replacement;
- the snapshot identifies exactly which active object supplies Core catalog semantics;
- catalog physical offset stays out of the snapshot and remains authenticated by the primary locator;
- variable roots/capabilities/extensions stay out of fixed commit framing.

There is no circular parser dependency. Core can authenticate the snapshot and primary directory using fixed epoch framing before interpreting the catalog payload.

## Width-parametric snapshot amendment

Proposed snapshot length:

```text
SNAPSHOT_LEN = 96 + I
```

Proposed fields:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Snapshot magic |
| 8 | 8 | Snapshot sequence |
| 16 | 8 | Directory root page offset |
| 24 | 1 | Directory root level |
| 25 | 7 | Zero reserved/alignment bytes |
| 32 | 32 | Directory root page digest |
| 64 | I | Non-zero catalog `ObjectId` |
| 64 + I | 32 | Parent snapshot digest; zero for genesis |
| total | 96 + I | |

Concrete consequences:

```text
I = 8  -> SNAPSHOT_LEN = 104
I = 16 -> SNAPSHOT_LEN = 112
```

The accepted epoch must freeze one exact numeric length before allocation; the formula exists only so this Review proposal does not pre-empt the identifier-width decision.

The footer remains 128 bytes and must require its `snapshot_length` field to equal the accepted fixed `SNAPSHOT_LEN`.

The complete snapshot bytes participate in the snapshot digest.

## Catalog object kind

Retain the proposed epoch-local Core kind:

```text
CORE_KIND_CATALOG = 1
```

The selected catalog object must satisfy ordinary object rules plus:

```text
object_id == snapshot.catalog_object_id
kind == CORE_KIND_CATALOG
```

A snapshot is invalid if the selected catalog object is absent, duplicated, has the wrong kind, or fails normal locator/object authentication.

Kind `1` is scoped to this disposable epoch and does not allocate a permanent registry value.

## Stable catalog structural slot

### Genesis

A genesis snapshot chooses one non-zero catalog ObjectId that is unique in its active primary directory.

There is no fixed globally reserved value.

### Linked child snapshots

For every linked non-genesis child snapshot:

```text
child.catalog_object_id == parent.catalog_object_id
```

A normal catalog metadata update therefore uses replacement semantics under the same structural key.

If the catalog is unchanged, the child may reuse the existing immutable object record/page path where the tree transition permits it.

### Catalog deletion

A published child snapshot may never leave the current catalog ID absent.

A mutation operation that would delete the active catalog structural slot without atomically replacing it under the same ID is invalid.

This is stronger and clearer than permitting each commit to arbitrarily rename the catalog object.

### Independent rewrite/new lineage

A rewrite that starts a new independent genesis lineage may choose a new catalog ObjectId unless the rewrite API explicitly promises preservation of structural keys.

If a rewrite promises ObjectId preservation, preserving the catalog ID is part of that promise.

This distinction avoids turning the structural slot into global identity while still making linked-history catalog behavior deterministic.

## Catalog payload

Retain catalog magic:

```text
UCCAT003
```

Catalog version remains `1` unless Review decides the v2 grammar itself should consume a new catalog-version number before the experimental epoch is allocated.

Payload grammar:

```text
CatalogHeader
RootObjectId[root_count]
CapabilityRecord[capability_count]
ExtensionBlock
```

## Catalog header — 32 bytes

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCCAT003` |
| 8 | 2 | Catalog version |
| 10 | 2 | Flags; zero in this epoch |
| 12 | 4 | Root-object count |
| 16 | 4 | Capability-record count |
| 20 | 4 | Exact extension-block byte length |
| 24 | 8 | Zero reserved bytes |

All counts and derived lengths use checked arithmetic before allocation/range use.

Caller policy must bound catalog bytes, roots, capabilities, extensions, individual payload sizes, and cumulative parse/preservation work.

Exceeding policy is distinct from malformed bytes.

## Application root list

Each root entry is exactly one `I`-byte ObjectId.

Rules:

- `root_count` may be **zero**;
- every present root ID is non-zero;
- roots are strictly increasing in canonical ObjectId byte order;
- duplicates are invalid;
- the catalog's own ObjectId must not appear in the application root list;
- every declared root must exist in the same active primary directory;
- root existence is a Core catalog invariant even if the root object's application kind is unknown.

The root list identifies application/profile entry points. It does not prove that every active object is semantically reachable from those roots.

## Why zero roots should be valid in Core

A mandatory catalog means the structural primary tree can never be physically empty.

That lets Core separate two concepts cleanly:

```text
physical primary tree: always contains at least the catalog
application root set: may contain zero or more roots
```

Benefits:

- no special empty-B+tree encoding is required;
- deleting the last application object does not require a global structural prohibition;
- a catalog-only file is a well-defined empty-application structural state;
- profiles that require a meaningful entry point can impose `root_count >= 1` themselves;
- Core avoids hard-coding one application-policy assumption into the container structure.

A transition deleting the final application object is valid only if the resulting snapshot still has a valid active catalog and the replacement/current catalog's root list remains semantically consistent.

## Capability records — 8 bytes

Retain the v1 capability record:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | Non-zero capability identifier |
| 4 | 1 | Flags |
| 5 | 3 | Zero reserved bytes |

Defined flags:

```text
bit 0 = REQUIRED
bits 1..7 = zero
```

Rules:

- identifiers are unsigned 32-bit little-endian values;
- zero is invalid;
- records are strictly increasing by identifier;
- duplicates are invalid;
- unknown flag bits are malformed.

Capability identifiers remain epoch-local experimental identifiers until a future registry policy exists.

## Capability interpretation outcome

Unknown optional capability:

- does not invalidate structural/catalog integrity;
- may be reported and ignored for operations not requiring it.

Unknown required capability:

- does not erase already-established structural/object/catalog integrity evidence;
- blocks semantic operations whose claimed support requires complete interpretation of the catalog/profile requirements.

Implementations should keep result classes distinct:

- structurally/catalog valid and supported for requested interpretation;
- structurally/catalog valid but unsupported required capability;
- malformed/integrity failure;
- resource-policy refusal.

Unsupported semantics must not be reported as cryptographic corruption.

## Extension block

Retain one canonical extension block at the end of every catalog, including when it has zero records.

Magic:

```text
UCEX0003
```

### Extension-block header — 16 bytes

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic `UCEX0003` |
| 8 | 4 | Extension-record count |
| 12 | 4 | Total block byte length including this header |

`total_length` must equal the catalog header's `extension_block_length` and consume the exact remaining catalog payload.

## Extension record — 8-byte fixed header plus payload

V2 removes extension REQUIRED flags.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | Non-zero extension tag |
| 4 | 4 | Payload length |
| 8 | variable | Opaque payload |
| after payload | 0..7 | Zero padding to 8-byte alignment |

Rules:

- tag zero is invalid;
- records are strictly increasing by tag;
- duplicates are invalid;
- payload and padded length use checked arithmetic;
- padding bytes are zero;
- no bytes trail the final declared record;
- zero records is valid and yields a 16-byte block.

Extension tags remain epoch-local experimental values until a future registry policy exists.

## Why requiredness belongs only to capabilities

The v1 proposal allowed both:

```text
required capability
required extension
```

without defining a relationship between the two.

That creates ambiguity:

- does an unknown required extension block all interpretation?
- only one profile?
- only an operation that happens to use the tag?
- is it effectively another capability declaration?

V2 removes the duplicate mechanism.

Capabilities answer:

> What support must an implementation have to claim a semantic interpretation/operation is supported?

Extensions answer:

> What canonical length-delimited metadata bytes are carried and may need opaque preservation?

If understanding an extension is necessary for a semantic feature, the corresponding accepted capability/profile definition declares that requirement. Core does not need a second generic REQUIRED bit on the metadata record itself.

## Unknown extension behavior

At Core level, an unknown extension:

- is not structural corruption;
- is not automatically a required-support failure;
- remains opaque;
- must be preserved byte-for-byte when a rewrite API explicitly promises unknown-extension preservation.

A known required capability/profile may independently declare that a specific extension tag is necessary for one semantic operation. That higher-level relationship is outside the generic extension framing.

A tool must not execute code, fetch external resources, or reinterpret unknown payloads merely because an extension record exists.

## Catalog validation sequence

After authenticating the active snapshot and primary directory:

1. require non-zero snapshot catalog ObjectId;
2. for linked non-genesis history, require the catalog ID to match the validated parent snapshot's catalog ID;
3. locate the catalog ID through the authenticated active directory;
4. validate its complete object record/digest through ordinary locator rules;
5. require `CORE_KIND_CATALOG`;
6. parse the exact 32-byte catalog header under caller limits;
7. validate exact payload length from roots/capabilities/extension-block lengths;
8. validate zero-or-more sorted unique root IDs;
9. reject the catalog ID as an application root;
10. validate sorted unique capability records and known flag bits;
11. parse the exact extension block, sorted tags, lengths, padding, and no-trailing-byte rule;
12. require every declared application root to exist in the active directory;
13. classify unsupported capabilities separately from integrity/malformed results;
14. return catalog/root/capability/extension facts separately from raw structural validity.

A missing root can therefore fail the catalog semantic invariant even when every outer digest is cryptographically correct.

## Interaction with targeted lookup

Core structural targeted lookup does not need to interpret arbitrary application capabilities before finding an object because EXP-0003 primary-directory framing is fixed by the epoch.

A caller requesting profile/application semantics must additionally establish the catalog/capability support that operation requires.

The mandatory catalog does not turn every opaque structural lookup into a profile interpretation operation.

## Interaction with history

Every linked historical snapshot names the **same catalog ObjectId**, but may resolve it to a different immutable catalog record according to that snapshot's active primary directory.

Verified-history validation revalidates each historical snapshot's selected active catalog record after that structural prefix validates.

This gives a stable semantic slot with immutable historical versions:

```text
same catalog ObjectId
+ different snapshot
-> possibly different authenticated catalog object bytes
```

An unsupported required capability in an ancestor can block semantic interpretation of that historical state without erasing linked-history structural integrity evidence.

## Interaction with recovery

Every recovery candidate validates its own snapshot-selected catalog object and catalog semantic invariants.

A prefix whose structural digests verify but whose selected catalog is missing, wrong-kind, malformed, or names missing roots is not a valid EXP-0003 active-state candidate.

Recovery still does not decide freshness/authorization among multiple valid candidates.

## Interaction with deletion

The mandatory catalog replaces the current global rule:

```text
reject deletion of final active object
```

with a more precise structural invariant:

```text
every published snapshot must keep its selected catalog ObjectId active,
well-formed, and authenticated
```

Application objects may be deleted down to zero application roots/objects if the resulting catalog accurately represents that state.

The catalog slot itself is not deletable in a valid linked child state; it may only be replaced under the same ID.

This change is independent of the still-open deletion borrower preference decision.

## Interaction with rewrite and compaction

For a rewrite that starts a new independent genesis lineage:

- choose exactly one catalog object;
- choose a valid catalog ObjectId under the rewrite's key policy;
- emit the accepted root/capability/extension grammar;
- preserve unknown extensions when the API promises preservation.

For a linked/persistent transition:

- preserve the catalog ObjectId;
- reuse the current catalog record if bytes are unchanged;
- otherwise replace under the same ID.

Semantic compaction must treat catalog-declared application roots as protected inputs unless the selected profile/rewrite policy intentionally constructs a new root set.

## Width consequences after #113 disposition

Once identifier geometry is selected, replace `I` with one fixed number everywhere before the proposal enters accepted epoch bytes.

### If tight 64-bit is selected

```text
I = 8
SNAPSHOT_LEN = 104
root ObjectId entry = 8 bytes
catalog ObjectId field = 8 bytes
```

### If tight 128-bit is selected

```text
I = 16
SNAPSHOT_LEN = 112
root ObjectId entry = 16 bytes
catalog ObjectId field = 16 bytes
```

The rest of the catalog grammar is unchanged by key width.

## Why not store roots/capabilities directly in the snapshot

V2 retains the v1 rejection:

- counts are variable;
- fixed snapshot growth would impose arbitrary maxima or another variable structure;
- roots/capabilities/extensions change independently from commit framing;
- ordinary authenticated catalog objects reuse existing integrity machinery;
- unknown extension preservation is easier in a length-delimited object.

The snapshot needs only the catalog structural key.

## Why not reserve a magic catalog ObjectId

A stable catalog slot does **not** require one globally distinguished byte value.

Genesis chooses any valid unique non-zero ObjectId; linked children preserve that selected value.

This gives stable lineage behavior without shrinking the application structural namespace or creating a universal magic key.

## Required authoritative vectors if accepted

After identifier width and this catalog grammar are both accepted, the EXP-0003 corpus should include at least:

1. catalog-only genesis with `root_count = 0`;
2. minimal one-root catalog;
3. multiple sorted roots;
4. stable catalog ID across unchanged linked child;
5. stable catalog ID across catalog replacement;
6. child snapshot that changes catalog ID and must fail;
7. deletion of final application object producing catalog-only valid state;
8. attempted deletion of catalog slot and failure;
9. known required capability;
10. unknown optional capability;
11. unknown required capability with integrity preserved/support blocked;
12. unknown extension preserved through catalog replacement;
13. missing root while outer digests remain valid;
14. duplicate/unordered/zero root;
15. duplicate/unordered/zero capability ID;
16. unknown capability flag bit;
17. duplicate/unordered/zero extension tag;
18. non-zero extension padding;
19. truncated extension payload;
20. extension-length/catalog-length mismatch;
21. wrong catalog object kind;
22. missing snapshot-selected catalog object;
23. catalog selected as its own application root;
24. linked history with different authenticated catalog bytes under the same catalog ID;
25. recovery candidate whose outer bytes verify but catalog semantics fail.

Cross-language vectors must pin catalog bytes and classification results, not only whole-file hashes.

## Review decisions presented by v2

Review should now focus on these bounded questions:

1. mandatory snapshot-selected ordinary catalog object: retain or reject;
2. stable catalog ObjectId across linked child snapshots: retain or permit renaming;
3. zero application roots in Core: allow or require one-or-more roots globally;
4. capability REQUIRED bit as the single generic support-criticality mechanism: retain or justify a second extension-level requiredness mechanism;
5. `u32` capability/tag namespaces and 32-byte catalog header: retain or revise;
6. unknown-extension preservation promise for rewrite APIs: exact scope;
7. root-level u8 plus seven zero bytes in snapshot: retain or revise.

Identifier width itself remains owned by #113/#13 rather than this proposal.

## Proposed Draft changes if accepted

Only after maintainer dispositions for geometry and catalog semantics:

- freeze `I` to the accepted ObjectId width;
- set exact `SNAPSHOT_LEN = 96 + I`;
- update the snapshot field table and footer snapshot-length requirement;
- assign epoch-local catalog kind `1`;
- add stable linked-history catalog-ID rule;
- add zero-or-more application roots;
- replace the final-active-object deletion prohibition with mandatory catalog reachability;
- add capability grammar/support outcomes;
- add simplified extension block/record grammar;
- add rewrite/history/recovery catalog semantics;
- add authoritative valid/invalid vectors listed above;
- remove catalog/capability placement from the Draft open-decision list.

## Boundary

This is a rebased Review proposal only.

It does **not**:

- select 8- or 16-byte ObjectIds;
- edit the self-contained EXP-0003 Draft;
- merge/accept the identifier packet #113;
- accept the deletion-policy recommendation;
- accept FCP-0003;
- allocate EXP-0003;
- regenerate authoritative vectors;
- claim production compatibility.

If this v2 proposal lands on `main`, PR #79 can be closed as superseded by a safer current-baseline review document; that closure would still not constitute acceptance of the catalog design.
