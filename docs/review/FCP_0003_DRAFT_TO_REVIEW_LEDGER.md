# FCP-0003 Draft → Review Decision Ledger

**Status:** governance consolidation; no maintainer decisions selected  
**Date:** 2026-08-13  
**Tracking:** issues #13, #16, #76; Phase 3 exit gates #10–#12

## Purpose

Phase 3 research is now ahead of the first self-contained EXP-0003 Draft. The repository has accumulated enough focused evidence that continuing to add byte-layout variants would increase drift rather than reduce uncertainty.

This ledger replaces the old mental model of “many unresolved Phase 3 questions” with a bounded sequence:

1. record a small set of maintainer policy dispositions;
2. apply those dispositions in one coordinated normative amendment;
3. generate the authoritative EXP-0003 candidate corpus from those exact bytes;
4. perform clean-room interpretation/reproduction before experimental allocation;
5. migrate the reference implementation to the accepted candidate bytes;
6. continue the broader Phase 3 implementation-qualification gates.

This document does **not** select any maintainer checkbox, accept FCP-0003, allocate `UCOF-EXP-0003`, or make current research bytes authoritative.

## Current evidence package

The focused normative evidence sequence is complete enough for maintainer disposition.

### Candidate 1 / objection transfer

- `docs/PHASE_3_DISPOSITION_DRAFT.md`
- `docs/review/FCP_0002_TO_0003_OBJECTION_TRANSFER.md`

Recommendation:

> Supersede EXP-0002 Candidate 1 as the reusable-page baseline while retaining its implementation, corpora, negative findings, security evidence, and regression value. Preserve compatibility/migration non-promises.

The transfer record classifies maintained remote adapters, production spill/publication, and full independent implementation as Phase 3 implementation/exit gates rather than missing EXP-0003 wire fields.

### Identifier and geometry

- Experiments 0107, 0108, 0135–0137
- `docs/review/EXP_0003_IDENTIFIER_GEOMETRY_DECISION_PACKET.md`

Recommendation:

```text
ObjectId                   8 opaque bytes
ordering                   unsigned lexicographic byte order
scope                      container-context structural lookup key
all-zero ObjectId          invalid
Core no-remap merge        no generic guarantee
page size                  16,384 bytes
object header              40 bytes
page header                40 bytes
leaf locator               56 bytes
internal reference         56 bytes, explicit child min + max
leaf capacity/min          291 / 146
internal fanout/min        291 / 146
leaf overflow              292 -> 146,146
internal overflow          292 -> 146,146
```

Tight 128-bit remains the explicit alternative if Core intentionally adopts a stronger uncoordinated/no-remap identifier contract.

### Deletion borrower policy

- Experiments 0110–0134
- `docs/review/EXP_0003_DELETE_POLICY_DECISION_PACKET.md`

Recommendation:

```text
if both siblings can lend:
    borrow from the fuller sibling
    exact occupancy tie -> left
otherwise:
    borrow from the one eligible sibling
merge fallback:
    preserve existing deterministic merge order
```

The recommendation is based on exact local balancing, real persistent Rust traces, policy-reward decomposition, recursive/internal evidence, and measured source-information cost. Current research default remains LeftFirst until disposition.

### Catalog, roots, capabilities, extensions

- `docs/review/EXP_0003_CATALOG_CAPABILITY_PROPOSAL_V2.md`

Recommendation:

- every snapshot selects one ordinary authenticated catalog object by ObjectId;
- ObjectId width is parameterized over the selected geometry until frozen;
- linked child snapshots preserve one stable catalog structural slot;
- catalog changes replace bytes under that same active ObjectId;
- `root_count == 0` is valid in Core;
- a mandatory catalog keeps the physical tree non-empty while the application-root set may be empty;
- capability records carry required-support semantics;
- extension records are sorted opaque length-delimited metadata without a second REQUIRED bit;
- unknown extensions are preserved where a rewrite API claims preservation;
- missing/wrong-kind/malformed catalog and missing declared roots are semantic-validity failures even when outer integrity verifies.

If the recommended 8-byte ObjectId geometry is selected, the proposed catalog-bearing snapshot becomes 104 bytes. If the tight 128-bit alternative is selected, it becomes 112 bytes.

### Hash domains, magics, and kind semantics

- `docs/review/EXP_0003_HASH_MAGIC_KIND_DECISION_PACKET.md`

Recommendation:

- SHA-256 only for EXP-0003;
- raw 32-byte digest fields;
- no algorithm identifier/agility field in this disposable epoch;
- retain exact current object/page/snapshot/commit NUL-terminated hash domains;
- retain exact current structural magics;
- page kinds `1=leaf`, `2=internal`, all others invalid;
- object kind `0` invalid;
- object kind `1` catalog if catalog is accepted, otherwise reserved to Core;
- object kinds `2..65535` structurally opaque application/profile tags, not a permanent universal type registry.

### Scoped determinism

- Experiments 0111 and 0138
- `docs/review/EXP_0003_SCOPED_DETERMINISM_DECISION_PACKET.md`

Recommendation:

- retain current-set canonical fresh/rewrite form;
- retain deterministic persistent transition from exact prior valid state + canonicalized batch;
- do **not** require equal persistent logical active states reached through different histories to have equal root/snapshot digests;
- revise the rationale to physical placement: locators/references/snapshots authenticate absolute offsets and persistent mutation deliberately reuses old physical bytes;
- explicitly acknowledge that history-independent `(B,2)` dynamic partitioning now exists and can match half-full occupancy, but canonical partitioning alone cannot remove EXP-0003's placement-sensitive byte identity.

## Maintainer ballot

The following are the remaining byte-significant policy choices that should be dispositioned before one coordinated normative rewrite.

No option is selected by this ledger.

### D1 — Candidate 1 / FCP-0002 reusable-page disposition

- [ ] **Recommended:** Candidate 1 is superseded as the reusable-page baseline and retained as disposable historical/security/regression evidence; no migration or compatibility promise.
- [ ] Revise disposition: ________________________________________________
- [ ] Defer for one named blocker: ______________________________________

### D2 — ObjectId and primary-directory geometry

- [ ] **Recommended:** tight 64-bit local structural geometry: 8-byte opaque key; 40-byte object/page headers; 56-byte leaf/internal entries; full child ranges; `C=291`, `M=146` for both page kinds.
- [ ] Tight 128-bit alternative: 16-byte opaque key; 48/56-byte object/page headers; 64/72-byte leaf/internal entries; leaf `255/128`, internal `226/113`.
- [ ] Revise/defer for one named requirement: _____________________________

### D3 — Occupancy and bulk/split policy

This decision is intentionally separated from field width even though the recommended numeric constants derive from D2.

- [ ] **Recommended:** half-full non-root occupancy; canonical final-two-group redistribution; root exceptions; lower/left-biased deterministic odd split rule where counts are odd; exact even split where the accepted capacity produces one.
- [ ] Revise occupancy/split policy: _____________________________________
- [ ] Defer for one named blocker: ______________________________________

If D2 tight-64 is selected, D3 derives:

```text
C = 291
M = 146
292 -> 146,146
```

for both leaf and internal pages.

### D4 — Deletion borrower policy

- [ ] **Recommended:** fuller eligible sibling, exact left tie-break, existing deterministic merge fallback order.
- [ ] Retain current LeftFirst borrower preference.
- [ ] Revise/defer for one named blocker: ________________________________

### D5 — Catalog/root/capability/extension binding

- [ ] **Recommended:** adopt the v2 snapshot-selected ordinary catalog architecture, stable linked-history catalog slot, zero-or-more application roots, capability-only REQUIRED semantics, simplified opaque extension records.
- [ ] Remove catalog/capability/extension bytes from EXP-0003 scope and name the later owner: ____________________.
- [ ] Revise/defer with specific blocking semantics: ______________________

### D6 — Hash/domain/magic/kind package

- [ ] **Recommended:** fixed SHA-256, exact current epoch domains/magics, fail-closed page kinds, object kind `0` invalid, `1` Core catalog/reserved, `2..65535` opaque application/profile tags.
- [ ] Revise package: ____________________________________________________
- [ ] Defer for one named blocker: ______________________________________

### D7 — Scoped determinism

- [ ] **Recommended:** retain scoped determinism with the revised authenticated-physical-offset rationale; canonical rewrite is normalization, persistent structural identity may remain history-sensitive.
- [ ] Replace with a complete history-independent persistent placement/reference/randomness proposal: ____________________.
- [ ] Defer for one named blocker: ______________________________________

## Decisions already sufficiently specified unless Review reopens them

The following are not presented as separate ballots because the objection-transfer/research package already converges on one precise direction and no current evidence packet identifies a competing Review candidate.

### Minimal authenticated primary locator

Keep:

```text
ObjectId
record offset
record length
SHA-256 object-record digest
```

Do not mirror kind/logical length/reserve into every primary entry.

Broad inventory acceleration remains an optional authenticated index/profile/service concern.

### Fixed-size primary entries

Keep fixed-width leaf/internal entries for the first interoperability epoch. Variable-length primary entries remain a rejected/deferred alternative.

### Complete active snapshots and strict validation/recovery separation

Keep:

- complete independently valid published snapshot/commit states;
- exact-end strict validity;
- explicit bounded backward recovery as a separate report-only operation;
- linked-history validation distinct from active validity and recovery.

### Canonical semantic batch normalization

Keep one deterministic operation ordering by ObjectId and explicit duplicate/conflict handling before byte emission. Persistent byte determinism is defined from the canonicalized batch, not caller iteration order.

### Strong source view is not freshness

Keep source-version stability, retry/cancellation, and trusted freshness/authorization as distinct assurance layers. Maintained HTTP/cloud implementations remain #10 work, not a wire-field ballot.

### Profile semantic dependencies remain above Core

Core understands structural reachability. Application/profile dependency semantics used by semantic compaction remain explicit resolver/profile inputs and must fail closed when unknown.

## Coordinated normative amendment after the ballot

Do **not** edit the Draft piecemeal as individual recommendations are selected.

Once D1–D7 are dispositioned, one normative amendment should make all affected documents agree in one reviewable change set.

Minimum affected artifacts:

1. `docs/proposals/0003-immutable-page-successor.md`
2. `spec/experimental/UCOF-EXP-0003.md`
3. `docs/spec/IMMUTABLE_SUCCESSOR_OCCUPANCY_POLICY.md`
4. `docs/PHASE_3_DISPOSITION_DRAFT.md` → actual decision record or successor record
5. `docs/PHASE_3_STATUS.md`
6. `docs/review/FCP_0002_TO_0003_OBJECTION_TRANSFER.md` blocker-status update
7. issue #13 status/body
8. issue #16 status/body
9. issue #76 P0 status

The amendment should explicitly mark earlier first-Draft/research numeric identities as historical/non-authoritative.

## Exact byte consequences under the packet recommendations

This section is **illustrative pending maintainer selection**, not normative.

If D2, D3, D5, and D6 take their recommended options:

```text
ObjectId                       8 bytes, opaque lexicographic
object header                 40
page size                 16,384
page header                   40
leaf locator                  56
internal reference            56
leaf/internal C              291
leaf/internal M              146
snapshot                     104
footer                       128
object/page/snapshot/commit digest  SHA-256 / 32 bytes
```

Catalog-bearing structural-empty/application-empty state:

```text
primary tree contains catalog object
catalog.root_count == 0
application root set empty
```

This replaces the current broad "deleting the final active object is invalid" concept with "the selected catalog structural slot must remain active and valid."

## Authoritative corpus gate

After the normative amendment, generate a **new** EXP-0003 candidate corpus from those exact bytes. Do not promote research vectors by relabeling them.

The corpus should be compact enough for an independent implementation but broad enough to pin every byte-significant boundary.

### Framing / identity

Include at least:

- minimal genesis;
- one append;
- linked-history chain;
- exact-end strict-valid file;
- torn/trailing commit invalid under strict mode;
- recoverable earlier valid prefix;
- exact object/page/snapshot/commit domain vectors;
- one-byte corruption of every structural magic;
- non-zero reserved/unknown flag failures.

### Object / locator

Include:

- minimum object payload;
- maximum policy-bounded test payload;
- wrong object digest;
- wrong locator record offset;
- wrong record length;
- object header ID mismatch;
- kind zero invalid;
- ordinary opaque object kinds at low/high accepted values.

### Occupancy / shape

For selected `C` and `M`, pin at least:

```text
1
C-1
C
C+1
2C-1
2C
2C+1
final group M-1 / M / M+1
C*internal_fanout - 1 / exact / +1
final internal group M-1 / M / M+1
```

Also pin:

- fresh canonical final-two redistribution;
- leaf split;
- internal split;
- root growth;
- root collapse;
- non-root under-minimum invalid cases.

### Persistent insertion/deletion/mixed

Include:

- one local insertion without split;
- leaf split propagation;
- internal split propagation;
- deletion without repair;
- single-sibling borrow cases;
- two-donor policy-significant deletion;
- merge;
- recursive internal repair;
- canonical mixed batch;
- equivalent caller operation order producing identical canonical batch bytes;
- byte reuse accounting where unchanged pages are expected to survive.

### Catalog / capabilities / extensions

If D5 accepts catalog v2, include at least:

- catalog-only zero-root genesis;
- one/multiple roots;
- stable catalog ID across unchanged child;
- stable catalog ID across catalog replacement;
- changed linked-child catalog ID invalid;
- deletion down to catalog-only state;
- attempted catalog-slot deletion invalid;
- known required capability;
- unknown optional capability;
- unknown required capability → structural integrity preserved but semantic support blocked;
- unknown extension preserved through claimed-preserving rewrite;
- missing root with outer digests otherwise valid;
- malformed root/capability/extension ordering/length/padding cases.

### Determinism / rewrite

Include:

- fresh canonical construction from different caller input order producing identical bytes;
- canonical rewrite normalization;
- persistent-history pair with equal logical active set but allowed byte/root divergence under scoped determinism;
- proof that rewrite identity is new byte/commit identity even when semantic active facts match.

## Candidate corpus status labels

Before experimental allocation, corpus labels should distinguish:

```text
candidate / review corpus
historical research corpus
Candidate 1 historical corpus
```

Do not call the new corpus “stable” or imply migration compatibility.

## Draft → Review gate

Recommended process gate:

FCP-0003 may move from Draft to Review only after all of these are true:

- [ ] D1–D7 have explicit maintainer dispositions committed.
- [ ] The coordinated normative amendment is merged and internally consistent.
- [ ] Exact byte tables derive all capacities/lengths without stale first-Draft constants.
- [ ] Candidate 1/FCP-0002 disposition is recorded consistently.
- [ ] The authoritative **candidate** valid/invalid corpus is generated from the selected bytes.
- [ ] In-repository Rust/Python/spec checks reproduce that corpus and all existing safety gates remain green.
- [ ] Rejected alternatives and compatibility non-promises remain explicit.
- [ ] No Phase 4 transform/compression byte dependency has been introduced.

This gate does **not** require production HTTP/cloud/spill qualification; those remain Phase 3 implementation/exit work.

## Review → experimental-allocation gate

Do not equate FCP Review status with allocation of `UCOF-EXP-0003`.

Before allocating the disposable interoperability epoch, require at least:

- Review corrections resolved;
- one clean-room interpretation/reproduction of the normative byte tables/candidate corpus that is meaningfully independent from the reference implementation source;
- mismatches classified as spec/reference/independent/vector defects rather than “fixed until both implementations agree”;
- allocation decision recorded explicitly by maintainers.

The exact governance label may be adjusted by maintainers, but the independence event must remain a separate explicit gate.

## Phase 3 exit remains broader

Even after experimental allocation and reference-byte convergence, Phase 3 still owns:

- #10 maintained real HTTP + one versioned cloud source + native async cancellation/qualification;
- #11 production-candidate spill confidentiality/durable publication/restart semantics;
- #12 independently maintained implementation or documented external clean-room review as a hard exit gate;
- semantic/profile compaction boundary convergence;
- ongoing fuzz/property/portability/adversarial evidence.

These gates should not be smuggled into EXP-0003 byte validity, and wire convergence should not be used to claim they are complete.

## Recommended maintainer review order

To minimize dependent byte churn:

1. D1 Candidate 1 disposition;
2. D2 geometry;
3. D3 occupancy/split;
4. D4 deletion borrower;
5. D5 catalog binding;
6. D6 hash/magic/kind;
7. D7 scoped determinism;
8. approve one coordinated normative amendment plan;
9. generate/review candidate corpus;
10. move FCP-0003 to Review if the Draft→Review gate is satisfied.

The order is a workflow recommendation, not a voting dependency; maintainers may review packets in parallel.

## What happens next if no maintainer policy is selected yet

Engineering work should **not** silently choose the packet recommendations by changing normative bytes.

The productive work that can continue without those choices is limited to:

- keeping evidence/CI green;
- preparing mechanical amendment/corpus tooling that is parameterized by the unresolved choices;
- updating stale trackers to point at this ledger;
- avoiding new Phase 4 byte dependencies.

Do not regenerate authoritative identities against bytes already known to be decision-pending.

## Boundary

This ledger does **not**:

- select D1–D7;
- edit FCP-0003 or the EXP-0003 Draft;
- mark FCP-0003 Review/Accepted;
- formally supersede FCP-0002 by itself;
- allocate `UCOF-EXP-0003`;
- migrate Rust research bytes;
- promote authoritative vectors;
- close #10–#12;
- start Phase 4.

Its purpose is to make the remaining governance path finite and auditable.