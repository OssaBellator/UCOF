# Phase 3 Status — EXP-0003 Successor Convergence

**Status:** In progress; implementation consolidated on `main`, normative successor choices remain pending  
**Started:** 2026-07-30  
**Authoritative implementation baseline:** `main` after PR #75 (`27956368289b6ebf230ff2baf5fa5313d4f83806`)  
**Current convergence milestone:** #76 — EXP-0003 Interoperability Candidate  
**Current Draft→Review decision surface:** `docs/review/FCP_0003_DRAFT_TO_REVIEW_LEDGER.md`

## Current objective

Phase 3 is no longer primarily an implementation-volume phase.

The repository contains substantial executable evidence for bounded hostile-input parsing, immutable primary-directory pages, snapshots, persistent copy-on-write mutation, linked history, explicit recovery, rewrite/compaction inputs, source-backed operation planning, transport policy, and publication research.

The remaining P0 objective is to turn that research into **one internally consistent, independently implementable experimental successor epoch** without treating green reference code or merged recommendation packets as normative acceptance.

Until P0 convergence is complete, Phase 4 transform/compression work should not establish new byte-level dependencies on unresolved successor choices.

## Authority split

The current repository authority is:

- **`main`** — authoritative implementation/research baseline;
- **FCP-0003 Draft + review packets/ledger** — normative proposals awaiting maintainer disposition;
- **current research implementation/vectors** — executable evidence, not accepted EXP-0003 bytes;
- **EXP-0001 / EXP-0002 Candidate 1** — disposable historical experimental evidence with no compatibility promise;
- **future candidate corpus** — must be generated only after selected EXP-0003 bytes are merged normatively.

## Experimental-epoch status

### UCOF-EXP-0001

EXP-0001 remains the disposable minimal framing/Phase 2 safety-first codec experiment. It is historical interoperability evidence, not a promotion candidate and not a compatibility promise.

### UCOF-EXP-0002 Candidate 1

Candidate 1 remains executable historical evidence for authenticated paged directories, snapshots, exact-end publication, bounded source access, history, recovery, repair, and rewrite.

The current **recommendation** is to supersede Candidate 1 as the reusable-page Phase 3 baseline because its page identity binds active snapshot sequence into page authentication and therefore prevents byte-for-byte reuse of unchanged historical pages.

That recommendation is **not yet the formal maintainer disposition**. Issue #13 and D1 of the Draft→Review ledger own the actual decision.

Candidate 1 should remain buildable/testable while its corpora and negative/security findings remain useful. No migration or compatibility promise is proposed.

### Immutable-page successor / FCP-0003

The immutable-page successor removes active snapshot sequence from page identity and is the current research direction represented most fully on `main`.

FCP-0003 remains **Draft**. `UCOF-EXP-0003` remains **unallocated**. The first self-contained Draft contains provisional first-Draft bytes that are intentionally allowed to differ from later Review recommendations.

No recommendation packet on `main` by itself changes those statuses.

## What `main` demonstrates

### Bounded hostile-input behavior

Research evidence includes:

- checked arithmetic and bounded ranges;
- caller-controlled read/byte/allocation/object/page/depth/diagnostic budgets;
- strict exact-end validation;
- explicit recovery/salvage modes kept separate from strict validity;
- bounded slice, seekable, and random-access sources;
- Rust 1.85 MSRV;
- i686 and powerpc64 portability checks;
- continuous fuzz/property/adversarial evidence.

### Immutable primary directory and identity scopes

Research evidence includes:

- immutable content-addressed pages;
- authenticated object records and primary locators;
- separate object/page/snapshot/commit identity scopes;
- authenticated lookup and absence;
- overlap/range/ordering validation;
- exact page reuse where unchanged bytes remain valid;
- distinct structural integrity, semantic support, freshness, and authorization claims.

### Persistent copy-on-write mutation

The consolidated implementation includes:

- replacement-only batches;
- insertion through authenticated paths;
- leaf/internal split propagation and root growth;
- deletion, borrow, merge, recursive internal repair, and root collapse;
- canonical mixed insertion/replacement/deletion batches;
- exact page reuse accounting;
- bounded append-tail streaming;
- source-backed planning variants.

Issue #9 is therefore closed as implementation-complete.

### Source-backed mutation/output

Research implementations cover verified and strongly-versioned source copying, source-backed replacement/insertion/deletion/multi-put/mixed planning, bounded output, selected history, and historical semantic-selection planning.

These demonstrate architectural feasibility without complete-file materialization. They do **not** qualify real provider behavior; #10 remains the maintained HTTP/cloud/native-async gate.

### History and recovery

The repository keeps these assurance modes separate:

- **strict active validation** — exact-end, no recovery fallback;
- **linked-history verification** — explicitly requested ancestry validation;
- **recovery** — explicitly requested, bounded, report-only candidate-prefix discovery.

Recovery never silently replaces the active state.

### Rewrite/compaction boundary

Core can determine structural validity/reachability but cannot infer arbitrary application dependency semantics from opaque payloads.

Profiles/applications supply semantic dependency edges; unknown dependency semantics must fail closed or use an explicit conservative-retention policy. Rewrite/compaction creates new identity and cannot falsely preserve byte-scoped signatures/provenance across changed bytes.

## P0 — current Draft→Review decision ledger

PR #117 consolidated the remaining byte-significant policy choices into seven explicit maintainer dispositions. **Every checkbox remains unselected.**

### D1 — Candidate 1 / FCP-0002 disposition

Recommended: supersede Candidate 1 as reusable-page baseline, retain historical/security/regression evidence and explicit non-promises.

Primary tracker: #13.

### D2 — ObjectId and primary-directory geometry

Recommended Review candidate:

```text
ObjectId                  8 opaque bytes, unsigned lexicographic
scope                     container-context structural lookup key
Core no-remap merge       no generic guarantee
page size                 16,384
object header             40
page header               40
leaf locator              56
internal reference        56, explicit child min + max
leaf/internal C           291
leaf/internal M           146
leaf/internal overflow    292 -> 146,146
```

Tight 128-bit remains the explicit alternative if Core intentionally adopts a stronger uncoordinated/no-remap identifier contract.

Evidence: Experiments 0107, 0108, 0135–0137 and `EXP_0003_IDENTIFIER_GEOMETRY_DECISION_PACKET.md`.

### D3 — occupancy and split policy

Recommended: half-full non-root occupancy, deterministic final-two redistribution, root exceptions, and deterministic split arithmetic derived from selected geometry.

Primary tracker: #16.

### D4 — deletion borrower policy

Recommended: borrow from the fuller eligible sibling, exact left tie-break, preserve deterministic merge fallback order.

Current research bytes still use LeftFirst; no normative/default-byte change occurs until disposition.

Evidence: Experiments 0110–0134 and `EXP_0003_DELETE_POLICY_DECISION_PACKET.md`.

### D5 — catalog/root/capability/extension binding

Recommended catalog-v2 architecture:

- one ordinary snapshot-selected authenticated catalog object;
- stable catalog structural slot across linked snapshots;
- zero-or-more application roots;
- catalog-only valid application-empty state;
- capability records carry REQUIRED support semantics;
- extension records are sorted opaque length-delimited metadata with explicit preservation behavior.

Current proposal: `EXP_0003_CATALOG_CAPABILITY_PROPOSAL_V2.md`.

### D6 — hash/domain/magic/kind package

Recommended:

- SHA-256 only for this disposable epoch;
- exact current epoch domains/magics;
- page kinds `1=leaf`, `2=internal`, others invalid;
- object kind `0` invalid;
- kind `1` catalog if D5 is accepted, otherwise Core-reserved;
- kinds `2..65535` structurally opaque application/profile tags.

Current packet: `EXP_0003_HASH_MAGIC_KIND_DECISION_PACKET.md`.

### D7 — scoped determinism

Recommended:

- fresh canonical rewrite is the normalized current-set structural form;
- persistent mutation is deterministic from exact prior validated bytes + canonicalized batch;
- equal logical active states reached through different histories need not have equal persistent root/snapshot digests.

Experiment 0138 updates the rationale: history-independent half-full B-tree partitioning is possible, but EXP-0003 authenticates physical object/page/root offsets and deliberately reuses immutable physical bytes. Canonical partition boundaries alone therefore do not make persistent structural identity placement-independent.

Current packet: `EXP_0003_SCOPED_DETERMINISM_DECISION_PACKET.md`.

## P0 — what happens after D1–D7

Do not apply packet recommendations piecemeal while the maintainer ballot is unresolved.

After dispositions are committed, make **one coordinated normative amendment** so these artifacts agree:

1. `docs/proposals/0003-immutable-page-successor.md`
2. `spec/experimental/UCOF-EXP-0003.md`
3. `docs/spec/IMMUTABLE_SUCCESSOR_OCCUPANCY_POLICY.md`
4. Candidate 1/FCP-0002 disposition/status records
5. this status document
6. objection-transfer blocker status
7. #13, #16, #76

Earlier first-Draft/research numeric identities must be marked historical/non-authoritative.

## P0 — candidate interoperability corpus

After the coordinated amendment, generate a **new** EXP-0003 candidate valid/invalid corpus from those exact selected bytes. Existing research identities must not be relabeled as authoritative.

The corpus must pin framing/digest domains, object/locator contradictions, occupancy/split boundaries, insertion/deletion/mixed mutation, history/recovery, selected catalog semantics, and canonical-rewrite versus persistent-history identity behavior.

Rust/in-repository checks must reproduce it while existing safety, portability, fuzz, and adversarial gates remain green.

## Draft → Review gate

FCP-0003 should move Draft→Review only after:

- D1–D7 actual maintainer dispositions are committed;
- the coordinated normative amendment is merged and internally consistent;
- all lengths/capacities derive from selected byte tables rather than stale Draft constants;
- Candidate 1/FCP-0002 disposition is recorded consistently;
- the new candidate corpus is generated from selected bytes;
- in-repository reproduction remains green;
- rejected alternatives/non-promises remain explicit;
- Phase 4 has introduced no dependency on unresolved successor bytes.

## Review → experimental allocation remains separate

FCP Review status does not allocate `UCOF-EXP-0003`.

Before allocation, require a meaningful clean-room interpretation/reproduction of the normative byte tables and candidate corpus. Material mismatches must be classified as spec/reference/independent/vector defects before implementations are changed merely to agree.

Allocation remains an explicit maintainer decision.

## P1 — reference implementation migration

Only after normative bytes are selected:

- migrate Rust to the exact accepted EXP-0003 grammar;
- keep current research/Candidate 1 bytes historical;
- regenerate candidate/authoritative vectors from specification rules rather than implementation inertia;
- preserve Rust 1.85, portability, docs, Clippy, fuzz, property, and adversarial gates.

## P1 — broader Phase 3 exit gates

### #10 — maintained remote adapters

Still required:

- maintained real HTTP range adapter with strong conditional semantics;
- one versioned cloud-object source;
- native async cancellation;
- provider/TLS/redirect/cache/decompression/request-budget qualification.

### #11 — production-candidate publication subsystem

Still required:

- encrypted spill when policy requires it;
- descriptor-relative hardening;
- authenticated restart/journal semantics;
- bounded cleanup;
- platform/filesystem durability qualification.

### #12 — independent implementation / external clean-room evidence

A meaningfully independent implementation or documented external clean-room review remains a hard Phase 3 exit gate. In-repository Rust/Python agreement can share the same misunderstanding.

## What `main` does not establish

Current evidence does **not** establish:

- a stable wire format;
- accepted FCP-0003 policy choices;
- formal Candidate 1 disposition;
- allocated EXP-0003;
- migration/compatibility guarantees;
- production-qualified durable publication;
- maintained provider adapters/native async cancellation;
- independent implementation agreement;
- signatures/provenance semantics;
- encryption/selective disclosure semantics;
- Archive/Table profile conformance;
- UCOF 1.0 readiness.

Integrity also does not establish authenticity, freshness, authorization, provenance, confidentiality, or rollback resistance by itself.

## Phase 3 exit rule

Phase 3 completes only when one disposable successor epoch is coherently selected/specifiable, its authoritative bytes/corpus and reference implementation agree, independent evidence exists, real remote-source behavior and publication claims are qualified at the level asserted, semantic/profile boundaries are explicit, continuous safety evidence remains green, and rejected alternatives/non-promises are recorded.

The intended milestone remains **EXP-0003 Interoperability Candidate**.

## Next phases

Only after the successor interoperability candidate is coherent should feature phases resume:

- Phase 4 — transforms and compression;
- Phase 5 — schemas and lossless diagnostic text projection;
- Phase 6 — signatures, provenance, and trust/freshness scopes;
- Phase 7 — encryption and selective disclosure;
- Phase 8 — Archive and Table profiles;
- Phase 9 — broader interoperability/conformance/benchmarks/hardening;
- Phase 10 — specification freeze and UCOF 1.0.
