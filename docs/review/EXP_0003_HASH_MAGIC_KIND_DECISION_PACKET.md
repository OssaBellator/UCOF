# EXP-0003 Hash Domains, Magic Values, and Kind Namespace Decision Packet

**Status:** maintainer-review packet; recommendation only  
**Date:** 2026-08-13  
**Target:** FCP-0003 Draft and `spec/experimental/UCOF-EXP-0003.md`  
**Related:** identifier/geometry packet #113; catalog v2 proposal #114; issues #13 and #76

## Purpose

The first self-contained EXP-0003 Draft already proposes one coherent cryptographic framing package:

- SHA-256;
- separate object/page/snapshot/commit domain prefixes;
- fixed 8-byte epoch-specific record magics;
- leaf/internal page kinds;
- a non-zero `u16` object-kind field.

The package is technically adequate, but two Review questions remain underspecified:

1. whether EXP-0003 should add digest-algorithm agility before allocation;
2. what the object-kind number actually means outside the proposed catalog kind.

This packet recommends freezing the simple existing hash/magic design and making the kind semantics explicit.

It does **not** select identifier geometry, accept the catalog proposal, accept FCP-0003, or allocate the epoch.

## Recommendation in one block

If accepted for Review:

```text
digest algorithm                 SHA-256 only
digest length                    32 raw bytes
algorithm identifier             none in EXP-0003
object domain                    ASCII "UCOF-EXP-0003-OBJECT\0"
page domain                      ASCII "UCOF-EXP-0003-PAGE\0"
snapshot domain                  ASCII "UCOF-EXP-0003-SNAPSHOT\0"
commit domain                    ASCII "UCOF-EXP-0003-COMMIT\0"
bootstrap magic                  ASCII "UCOFIM03"
object magic                     ASCII "UCOBOBJ3"
page magic                       ASCII "UCPGIM03"
snapshot magic                   ASCII "UCSNIM03"
footer magic                     ASCII "UCFTIM03"
catalog payload magic            ASCII "UCCAT003" if catalog accepted
extension block magic            ASCII "UCEX0003" if catalog accepted
page kind 1                      leaf
page kind 2                      internal
other page kinds                 invalid in this epoch
object kind 0                    invalid
object kind 1                    Core catalog if catalog proposal accepted;
                                 otherwise reserved to Core/unusable
object kinds 2..65535            structurally opaque application/profile tags
```

No value in this packet is a permanent UCOF registry allocation. Everything is scoped to the disposable EXP-0003 epoch.

## Decision 1 — SHA-256 baseline

Retain SHA-256 as the only EXP-0003 digest algorithm.

Every object/page/snapshot/commit digest field is exactly 32 bytes containing the raw SHA-256 output.

Do not add:

- algorithm identifiers;
- variable digest lengths;
- negotiated hash suites;
- per-object/per-page algorithm choice;
- implicit fallback algorithms.

### Why fixed SHA-256 is the correct experiment boundary

EXP-0003 exists to test the structural container contract, not cryptographic agility.

Adding algorithm choice now would affect:

- locator width;
- internal-reference width;
- snapshot/footer grammar;
- parser state and error classes;
- canonical vector identity;
- cross-implementation negotiation behavior;
- downgrade/fallback policy.

No current EXP-0003 requirement needs those dimensions.

A later incompatible epoch can replace the baseline algorithm or define an explicit cryptographic-service layer once UCOF's stable architecture has a real agility requirement.

For this disposable epoch, one mandatory algorithm makes independent reproduction easier and removes downgrade ambiguity.

## Decision 2 — exact domain prefixes

Retain the current exact domain prefixes:

| Scope | Exact prefix bytes |
|---|---|
| Object | `UCOF-EXP-0003-OBJECT\0` |
| Page | `UCOF-EXP-0003-PAGE\0` |
| Snapshot | `UCOF-EXP-0003-SNAPSHOT\0` |
| Commit | `UCOF-EXP-0003-COMMIT\0` |

Rules:

- the text is exact 7-bit ASCII;
- the final NUL byte is part of the domain;
- no Unicode normalization/case folding/terminator omission is permitted;
- implementations hash exactly those bytes before the scope-specific preimage;
- the prefixes are constants of the epoch, not file fields;
- domain prefixes are not stored or negotiated in the file.

### Why the NUL terminator is useful

The final NUL makes the textual domain a visibly terminated byte label rather than relying on an implementation's string representation.

The four labels are already distinct and epoch-specific. There is no need to add another length field or nested hash merely to separate these scopes.

## Decision 3 — object identity

Retain:

```text
object_digest = SHA-256(
    OBJECT_DOMAIN || exact_object_record_bytes
)
```

The object record includes:

- exact fixed object header bytes;
- exact stored payload bytes.

Therefore ObjectId, kind, flags, stored/logical lengths, and payload are all covered by object identity.

Physical offset is not included in object identity because it is carried/authenticated by the directory locator rather than the object record itself.

## Decision 4 — page identity

Retain:

```text
page_digest = SHA-256(
    PAGE_DOMAIN || exact_PAGE_SIZE_page_bytes
)
```

The complete fixed page includes zero tail padding.

Page identity intentionally excludes:

- physical offset;
- active snapshot sequence;
- commit sequence;
- file-instance publication identity.

Those fields do not appear in page bytes.

This is the property that allows an unchanged immutable page to be reused byte-for-byte across snapshots.

## Decision 5 — snapshot identity

Retain:

```text
snapshot_digest = SHA-256(
    SNAPSHOT_DOMAIN || exact_snapshot_bytes
)
```

The exact snapshot length is owned by the geometry/catalog decisions. This packet does not choose 96, 104, or 112 bytes.

Whatever fixed snapshot grammar is accepted, every byte participates in the snapshot digest.

## Decision 6 — commit identity

Retain the current construction:

```text
commit_digest = SHA-256(
    COMMIT_DOMAIN ||
    exact_current_commit_bytes_before_footer ||
    footer_semantics
)
```

where `footer_semantics` remains one fixed-width byte sequence defined by the accepted footer table.

The verifier knows the exact boundary between current-commit bytes and footer semantics because footer position and footer-semantics length are fixed by the epoch grammar.

No generic tuple-encoding layer is needed for this experiment.

## Decision 7 — exact record magics

Retain the current 8-byte structural magics:

| Structure | Exact bytes |
|---|---|
| Bootstrap | `UCOFIM03` |
| Object record | `UCOBOBJ3` |
| Directory page | `UCPGIM03` |
| Snapshot | `UCSNIM03` |
| Commit footer | `UCFTIM03` |

If catalog v2 is accepted, also retain:

| Structure | Exact bytes |
|---|---|
| Catalog payload | `UCCAT003` |
| Extension block | `UCEX0003` |

All are exact ASCII byte sequences with no terminator.

### Magic values are framing, not authentication

A matching magic does not establish integrity or authenticity.

A parser uses magic to classify fixed grammar. Authentication comes from the relevant digest/reference chain.

Conversely, a record with incorrect magic is malformed even if some surrounding digest has been recomputed to cover those malformed bytes.

## Decision 8 — page-kind namespace

Retain:

```text
page kind 1 = leaf
page kind 2 = internal
```

Every other page-kind value is malformed in EXP-0003.

Reason: directory page kinds define mandatory structural grammar. An implementation cannot safely skip an unknown page kind and still claim primary-tree validity.

New mandatory structural page kinds therefore require a new incompatible epoch unless an explicit extension mechanism is designed before allocation.

This is intentionally fail-closed.

## Decision 9 — object-kind namespace

The current Draft only says object kind is a non-zero `u16`. That is not enough to guide independent implementations.

Recommended semantics:

### Kind zero

```text
kind == 0 -> invalid object record
```

Zero remains a useful uninitialized/absent sentinel.

### Kind one

If catalog v2 is accepted:

```text
kind == 1 -> CORE_KIND_CATALOG
```

Only the snapshot-selected active catalog object may claim Core catalog semantics.

An arbitrary application object with kind `1` does not become the selected catalog merely because of the tag; the snapshot's catalog ObjectId is authoritative.

If the catalog proposal is ultimately rejected, kind `1` remains reserved/unusable in EXP-0003 rather than silently becoming an application assignment.

### Kinds 2 through 65535

```text
2..=65535 -> structurally opaque application/profile kind tags
```

Core structural validation:

- accepts these non-zero values in ordinary application objects;
- authenticates them as part of the object record;
- does not infer parser/schema/codec behavior from the number;
- does not claim the number is globally unique across profiles;
- does not execute code or fetch resources because of the number;
- leaves semantic interpretation to accepted profile/application context.

This lets the epoch test opaque object tagging without pretending to create a stable universal type registry.

## Why not reserve a large Core kind range

A partition such as `1..255 = Core` would create policy without a concrete use.

EXP-0003 currently needs at most one Core object grammar: the catalog.

If the experiment later discovers that another mandatory Core object kind is required before allocation, Review can assign it explicitly. After allocation, adding a new mandatory Core grammar should be treated as an incompatible-epoch change rather than relying on speculative reserved numbers.

The container's universal property should come from stable framing/capabilities/profiles, not from a permanently expanding mandatory object-kind registry.

## Kind values are not semantic identities

Even for application objects, the `u16 kind` is only an authenticated local tag.

It is not sufficient by itself to establish:

- globally unique media/schema type;
- profile identity;
- versioned schema identity;
- transform stack;
- external MIME identity;
- safe executable codec selection.

Profiles needing those concepts must define them explicitly through profile/capability/schema mechanisms rather than overloading this field.

## Decision 10 — capability/tag numeric scopes

If catalog v2 is accepted:

- capability identifiers remain non-zero `u32` epoch-local values;
- extension tags remain non-zero `u32` epoch-local values;
- neither namespace is a permanent stable UCOF registry;
- required capability support and unknown-extension preservation follow the catalog proposal.

This packet does not assign new capability/tag numbers.

## Reserved bytes and flags

Retain the existing strict rule:

- every reserved byte is zero;
- every unassigned flag bit is zero;
- non-zero reserved/unassigned bits are malformed for the allocated epoch.

This is compatible with EXP-0003 being a fixed disposable epoch.

Forward compatibility is exercised through explicit capabilities/extensions, not by silently accepting unknown bits whose semantics are undefined.

## No implicit algorithm or magic fallback

A conforming EXP-0003 reader must not:

- infer another digest algorithm from digest length;
- retry validation under multiple hash algorithms;
- accept case-insensitive/partial magic matches;
- guess an epoch from record magics when bootstrap framing identifies another epoch;
- reinterpret unknown page kinds;
- reinterpret unknown Core-reserved object kind `1` as application data.

Unknown/incompatible epoch input must fail as unsupported/malformed according to the top-level parser contract rather than through format guessing.

## Security considerations

### Domain separation

Separate object/page/snapshot/commit prefixes prevent the same byte string from being intentionally treated as the same identity across structural scopes.

### SHA-256 is not a trust statement

These digests provide integrity relative to authenticated references. They do not establish:

- signer identity;
- provenance;
- authorization;
- confidentiality;
- freshness;
- rollback resistance.

Those claims remain separate services/policies.

### Magic is not a security boundary

Magic is parser framing only. Digest/reference validation and structural cross-checks establish integrity.

### Opaque object kinds must remain opaque

A parser that dispatches arbitrary kind values to unsafe codecs/executables expands the trust boundary beyond EXP-0003. Core must not do that.

## Required vectors after final byte decisions

Once geometry/catalog/deletion decisions are accepted, authoritative vectors should pin at least:

1. object digest with exact OBJECT domain;
2. page digest with exact PAGE domain;
3. snapshot digest with exact SNAPSHOT domain;
4. commit digest with exact COMMIT domain;
5. same payload bytes hashed under two domains yielding different expected digests;
6. omitted/changed domain NUL producing different digest and therefore failing the pinned identity;
7. one-byte corruption in each structural magic;
8. object kind zero invalid;
9. ordinary opaque object kind `2` structurally valid without Core interpretation;
10. highest object kind `65535` structurally valid opaque;
11. unknown page kind invalid;
12. catalog selected with wrong object kind invalid if catalog is accepted;
13. non-zero reserved/unknown flag bits invalid.

Do not pin whole-file digest identities from the current first-Draft geometry merely to test domains; generate these vectors after accepted byte tables are frozen.

## Proposed maintainer disposition

Select exactly one:

- [ ] **Adopt fixed SHA-256, current exact domains/magics, fail-closed page kinds, and the object-kind semantics in this packet.**
- [ ] Revise the package with these specific changes: ____________________.
- [ ] Defer pending one named blocker: ____________________.

**Packet recommendation:** first option.

No checkbox is selected by this packet.

## If adopted

Update the normative package in one coordinated edit:

1. keep SHA-256 and 32-byte digest fields;
2. freeze the four exact NUL-terminated domain constants;
3. freeze exact structural magics;
4. explicitly define page kinds `1/2` and reject all others;
5. explicitly define object kind `0` invalid;
6. reserve/assign object kind `1` to catalog according to the catalog disposition;
7. define `2..65535` as structurally opaque application/profile tags;
8. state that capability/tag values are epoch-local experimental identifiers;
9. add the final domain/magic/kind valid/invalid vectors after geometry is frozen.

## Boundary

This packet does **not**:

- choose 8- versus 16-byte ObjectIds;
- change object/page field widths;
- accept catalog v2;
- select deletion borrower policy;
- edit the Draft itself;
- accept FCP-0003;
- allocate `UCOF-EXP-0003`;
- create permanent UCOF registry values;
- claim SHA-256 provides authenticity/freshness;
- regenerate authoritative file identities.

Its purpose is to turn the remaining hash/magic/kind ambiguity into one bounded Review decision rather than adding algorithm agility or registry machinery without evidence.
