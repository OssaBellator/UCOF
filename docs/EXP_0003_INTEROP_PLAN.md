# EXP-0003 Interoperability Convergence Plan

**Status:** Active planning and convergence work  
**Started:** 2026-08-13  
**Baseline:** `main` after PR #75 (`27956368289b6ebf230ff2baf5fa5313d4f83806`)  
**Goal:** turn the consolidated Phase 3 research implementation into one independently implementable experimental successor epoch before Phase 4 feature work begins.

## Why this milestone exists

The repository now has substantial Phase 1–3 implementation and evidence on `main`, including bounded readers/writers, immutable-page successor research, copy-on-write mutation, history and recovery, source-backed operation planning, staged publication research, transport classification, and extensive vector/fuzz evidence.

The remaining risk is no longer lack of implementation volume. The main risk is specification drift: executable research, proposal text, old stacked-PR status documents, and open tracker issues no longer describe one single authoritative experimental format.

This milestone therefore prioritizes **convergence, independent implementability, and evidence quality** over new format features.

Phase 4 transform/compression work should not establish new byte dependencies until the EXP-0003 convergence gates below are satisfied.

## Architectural rule

UCOF remains a **universal container, not a universal representation**.

The universal core should remain small and stable enough to implement independently. Domain semantics belong in profiles and optional services. The core must not require every reader to implement compression families, schemas, signatures, encryption systems, or application-specific dependency graphs merely to enumerate and validate opaque objects safely.

## Target layering

### UCOF Core

- bootstrap/framing and experimental epoch identification;
- object records and opaque payload boundaries;
- immutable primary directory pages;
- snapshots, commits, active-root rules, history, and explicit recovery;
- capability declaration and unknown-feature handling;
- structural and content integrity scopes;
- bounded hostile-input validation;
- deterministic construction rules required for canonical identity.

### UCOF Services

- remote source adapters;
- repair and compaction executors;
- transforms and compression;
- schemas and text projection;
- signatures/provenance;
- encryption/key handling;
- durable publication/storage adapters.

### UCOF Profiles

- Archive;
- Table;
- later Media, Document, Scientific, Database, Package, and other domain profiles.

Profiles may define semantic dependencies and additional indexes. They must not silently change core validity rules.

---

# P0 — Select one experimental successor

## P0.1 Disposition Candidate 1 and FCP-0003

Primary tracker: issue #13 and PR #8.

Required decisions:

- explicitly retain `UCOF-EXP-0002` Candidate 1 as negative/security evidence rather than the reusable-page promotion candidate;
- decide whether the immutable-page successor should become the next disposable epoch;
- if approved for experimentation, allocate `UCOF-EXP-0003` only after the proposal package is internally consistent;
- transfer all material EXP-0002 objections into FCP-0003, rejected alternatives, or named later phases;
- record migration non-promises for EXP-0001/0002 research bytes.

### Candidate policy package to review

- 128-bit opaque object identifiers;
- exact minimal authenticated locator layout;
- 16 KiB immutable content-addressed pages unless contrary evidence wins review;
- exact leaf/internal capacities;
- half-full non-root occupancy, including final-two-page redistribution;
- deterministic split rule;
- deterministic deletion borrow/merge order;
- root exceptions and empty-tree policy;
- canonical one-operation-per-ID batch semantics;
- content/page/snapshot/commit identity scopes;
- unknown required/optional capability behavior;
- exact-end active-root selection;
- explicit separation of strict validity, linked-history validity, and recovery evidence.

**Gate:** no EXP-0003 vector is authoritative until this policy package is fixed for the experimental epoch.

## P0.2 Close the post-consolidation status gap

The older Phase 3 status and review packet predate the PR #75 consolidation and still describe the former stacked topology.

Required work:

- replace branch/PR topology descriptions with `main` as the authoritative implementation baseline;
- classify every open Phase 3 issue as `satisfied`, `partially satisfied`, `normative decision`, `production qualification`, or `external evidence`;
- close or rewrite trackers whose implementation requirements are already present on `main`;
- keep policy/evidence PRs open only when they preserve a distinct review boundary;
- preserve historical documents as evidence, but mark them as historical where appropriate.

### Immediate tracker audit

- #9: persistent insertion/deletion/mixed writer requirements — compare against consolidated `main` and close or reduce to remaining gaps;
- #16: occupancy convergence — separate already-implemented executable behavior from the remaining normative acceptance/vector migration decision;
- #10: maintained remote adapters and native async cancellation — remains a real implementation/qualification gate;
- #11: encrypted spill and durable publication qualification — remains a production gate;
- #12: independent successor implementation/external review — remains a hard interoperability gate;
- #13: FCP-0003/Candidate 1 disposition — remains the primary normative gate.

---

# P0 — Produce the EXP-0003 specification package

After the P0 policy decision, create one normative experimental package that an implementer can read without reference-source archaeology.

Required documents:

1. `spec/experimental/UCOF-EXP-0003.md`
   - binary grammar and byte order;
   - bootstrap/header/footer rules;
   - object records;
   - locator and page encodings;
   - snapshot/commit structures;
   - canonical padding and reserved-field requirements.
2. Canonical algorithms
   - genesis grouping;
   - insertion and split;
   - deletion, borrow, merge, recursive underflow, root collapse;
   - canonical mixed batch;
   - identity/digest calculation;
   - validation ordering where order is security-relevant.
3. Assurance semantics
   - strict exact-end validation;
   - authenticated lookup and absence;
   - linked-history verification;
   - report-only bounded recovery;
   - repair/rewrite identity rules.
4. Capability and preservation rules
   - required versus optional capabilities;
   - unknown optional byte preservation;
   - rewrite/compaction preservation policy.
5. Resource and conformance requirements
   - checked arithmetic;
   - caller-controlled work/allocation limits;
   - no malformed-input panics;
   - implementation-defined ceilings versus normative structural limits.

**Gate:** every byte-significant rule must be explicit enough for a clean-room implementation.

---

# P0 — Build a compact authoritative interoperability corpus

The research repository may retain broad experimental corpora, but EXP-0003 needs a smaller authoritative set.

Each valid vector should contain:

- recipe/source description;
- binary or deterministic generator;
- exact byte length;
- SHA-256 identity;
- object inventory;
- tree shape and root facts;
- expected assurance outcomes;
- annotated structure where useful.

Minimum valid vectors:

1. smallest valid file;
2. one full leaf boundary;
3. multi-leaf genesis;
4. multi-level tree;
5. replacement with historical page reuse;
6. insertion without split;
7. leaf split;
8. internal split/root growth;
9. deletion without underflow;
10. left/right borrow boundary cases;
11. merge;
12. recursive underflow/root collapse;
13. canonical mixed batch;
14. linked history;
15. interrupted append with older valid prefix;
16. bounded recovery candidates;
17. unknown optional capability;
18. unknown required capability;
19. selected rewrite;
20. semantic-compaction profile example.

Minimum invalid corpus:

- every fixed-field corruption class;
- integer/offset overflow attempts;
- object and structural overlap;
- page ordering and occupancy violations;
- locator/header contradictions;
- digest mismatches at each identity layer;
- broken parent/sequence history;
- forged recovery hints;
- duplicate batch/object identifiers;
- unknown-required interpretation attempts;
- truncation around every publication boundary.

**Gate:** Rust and at least one independent implementation must agree on all authoritative outcomes.

---

# P1 — Rebase the reference implementation onto EXP-0003

Do not preserve research byte compatibility for its own sake.

Required work:

- implement the accepted identifier/locator/page layout exactly;
- isolate old EXP-0002/Candidate 1 code and vectors as historical evidence;
- regenerate every authoritative EXP-0003 vector from the specification rules;
- keep parsing/writing layers separate from transport, filesystem, transform, schema, crypto, and profile code;
- remove magic constants from harnesses where format constants exist;
- preserve Rust 1.85, 32-bit, big-endian, docs, Clippy, fuzz, and property gates.

**Gate:** the reference implementation must be an implementation of the spec, not the source of undocumented rules.

---

# P1 — Independent implementation gate

Primary tracker: issue #12.

Preferred outcome: a separately maintained implementation in another language or repository.

Minimum reader coverage:

- exact-end bounded validation;
- directory traversal;
- authenticated lookup and absence;
- linked history;
- report-only recovery;
- invalid corpus classification.

Minimum writer coverage:

- genesis;
- replacement/page reuse;
- split;
- merge/root collapse;
- canonical mixed batch.

Rules for disagreements:

- record the mismatch before changing either implementation;
- classify the defect as specification, reference implementation, independent implementation, or vector error;
- do not change the independent implementation merely to match reference bytes;
- resolve every blocker/high-severity mismatch publicly.

**Gate:** Phase 3 does not exit until this evidence exists.

---

# P1 — Real remote-source qualification

Primary tracker: issue #10.

Implement at least:

- maintained HTTP range adapter using strong `ETag`/`If-Match` semantics;
- one versioned cloud-object adapter using provider-native immutable generation/version tokens;
- native asynchronous cancellation capable of aborting in-flight operations;
- operation-wide request, byte, retry, allocation, deadline, and cancellation budgets;
- fail-closed response/body/range/version validation;
- explicit TLS, credential, redirect, proxy, cache, and decompression policy.

Qualification workloads:

- metadata inspection;
- authenticated lookup/absence;
- full strict validation;
- linked history;
- recovery;
- selected rewrite.

Publish request/byte/latency/accounting evidence separately from in-memory synthetic profiles.

---

# P1 — Production-candidate writer/publication subsystem

Primary tracker: issue #11.

Required capabilities:

- descriptor-relative safe filesystem operations;
- private staging;
- optional/required encrypted spill policy;
- fresh operation key and AEAD nonce discipline;
- authenticated staged segments;
- bounded memory, bytes, files/inodes, descriptors, merge passes, and cleanup work;
- deterministic final UCOF bytes independent of spill ciphertext randomness;
- no-overwrite publication;
- authenticated durable journal/restart classification;
- explicit `not published`, `published durable`, and `publication indeterminate` results;
- platform/filesystem-specific durability qualification rather than generic claims.

This is primarily an implementation/storage contract. Avoid making filesystem mechanics normative UCOF bytes unless the format itself depends on them.

---

# P1 — Semantic compaction boundary

Consolidate the semantic/profile research into one explicit contract:

- core determines structural reachability only;
- a profile/application dependency resolver supplies semantic edges;
- unknown dependency semantics fail closed or conservatively retain according to explicit policy;
- history-retention policy is caller/profile controlled;
- optional unknown data has a preservation policy;
- byte-scoped signatures/provenance are explicitly invalidated or reissued;
- compaction produces new identity and never pretends rewritten bytes preserved old signatures.

Use the existing reference-list profile and independent vectors as evidence, not automatic normative adoption.

---

# Phase 3 exit / EXP-0003 interoperability release

Publish an experimental interoperability release only when all of the following are true:

- FCP-0003 disposition is recorded;
- one EXP-0003 experimental specification is internally consistent;
- authoritative valid/invalid vectors are published;
- consolidated Rust implementation matches those vectors;
- independent implementation/review gate passes;
- real remote-source adapters are qualified;
- large writer/publication behavior has a documented production-candidate contract;
- strict validation, history, recovery, repair, rewrite, and semantic-compaction boundaries are unambiguous;
- continuous fuzzing/property/portability/adversarial gates remain green;
- known rejected alternatives and compatibility non-promises are documented.

Suggested milestone name: **EXP-0003 Interoperability Candidate**.

This milestone remains disposable and must not claim UCOF 1.0 compatibility.

---

# Work after EXP-0003 convergence

Only after the interoperability candidate is coherent should feature phases resume:

- Phase 4 — transforms and compression;
- Phase 5 — schemas and lossless diagnostic text;
- Phase 6 — signatures, provenance, and trust/freshness scopes;
- Phase 7 — encryption and selective disclosure;
- Phase 8 — Archive and Table profiles as the universality proof;
- Phase 9 — broader independent implementations, conformance, benchmarks, and hostile-world hardening;
- Phase 10 — specification freeze and UCOF 1.0.

The key success criterion is not the number of supported features. It is whether a small core can safely and efficiently support materially different profiles without profile-specific semantics becoming mandatory core behavior.
