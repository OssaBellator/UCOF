# Phase 3 Frontier Tracker

**Updated:** 2026-08-01  
**Purpose:** Keep verified implementation evidence, proposed policy, and external or production gates separate.

## Current verified implementation frontiers

| Frontier | PRs | Verified claim | Remaining gate |
|---|---:|---|---|
| Canonical occupancy | #17, #18 | Deterministic canonical grouping, strict validation, independent Python construction, boundary tables, and regenerated identities | FCP disposition and migration to the proposed identifier/locator geometry |
| Persistent single-operation and multi-`Put` writing | #21, #23 | Authenticated deletion, borrow, merge, recursive repair, root collapse, shared insertion/replacement paths, split propagation, and exact reuse accounting | Landing order and wider-layout migration |
| Mixed planning and authenticated bytes | #27, #30, #34, #37 | Simultaneous mixed leaf repair, recursive shape, exact page identity, canonical final locator regrouping, exact current-page-body reuse, changed-page authentication, and one linked commit | Minimal-rewrite comparison and bounded streaming output for persistent updates |
| Independent mixed transition evidence | #40 | Clean-room Python regeneration of stable-height, root-collapse, and root-growth transitions with pinned per-vector and aggregate SHA-256 identities | Complete proposed-epoch transition corpus and external interoperability review |
| Bounded canonical output | #35, #36 | Canonical genesis bytes through bounded sequential sinks from owned and strongly versioned payload sources, with incremental hashing and version-change failure | Atomic staging integration |
| Active and selected active rewrites | #38, #39 | Strict active-file validation, borrowed authenticated payload ranges, inactive-history skipping, caller-selected output, exact selected read accounting, and untouched-sink selection failures | Selected historical-state streaming |
| Dependency-aware streaming compaction | #22, #28, #41 | Bounded graph closure, independent graph/profile codecs, selected authenticated streaming, exact reachable/orphan accounting, and fail-before-output graph errors | Application-profile adoption, profile rewrite-byte vectors, large-graph spill, and provenance/extension policy |
| Versioned bounded-source inventory and output | #42, #43 | Strict exact-end active inventory and canonical source-to-sink output over one non-ABA versioned source handle, one cumulative budget, authenticated descriptors, per-object version checks, digest equality, and inactive-history skipping | Maintained provider adapter, selected-history source-to-sink, retry/auth integration, and atomic publication staging |
| Source history and measurements | #14, #24, #32 | Bounded-source strict/history/recovery/rewrite APIs, selected historical reissuance, hostile-source fuzzing, and reproducible read/hash/allocation counters | Canonical selected-history sink integration and real provider measurements |
| Retry and freshness | #5, #19, #25, #31 | Strong-version operation binding, operation-wide attempt budgets, independent retry traces, bounded delay/deadline planning, and explicit freshness authorization guidance | Maintained provider adapters, real waits, jitter/authentication policy, native async cancellation, and application checkpoint stores |
| Spill publication | #20, #26, #29, #33 | Publication authority model, independent traces, real Unix no-overwrite hard-link publication, directory synchronization, and authority-boundary fault injection | Encryption, descriptor-relative hardening, restart/power-loss evidence, network-filesystem policy, and platform qualification |
| Proposal and external review packet | #8 | Draft FCP, Candidate 1 disposition recommendation, production spill requirements, freshness authorization, independent-review packet, manifest, and tracker | Maintainer decisions and external review findings |

All “verified” claims above refer to the current research layout. They do not allocate an epoch, accept FCP-0003, or establish production suitability.

## Stack map

- PRs #5–#8 share PR #4 as their review baseline.
- Writer chain: #7 → #15 → #17 → #21 → #23 → #27 → #30 → #34 → #37 → #40.
- Output/source chain: #23 → #35 → #36 → #38 → #39 → #41 → #42 → #43.
- Source/history chain: #6 → #14 → #24 → #32.
- Semantic-policy chain: #6 → #22 → #28.
- Retry chain: #5 → #19 → #25 → #31.
- Spill chain: #8 → #20 → #26 → #29 → #33.
- Occupancy policy #18 is refreshed on #8. The #20 descendant chain should only be refreshed atomically.

## Frontier status

### 1. Successor epoch and normative policy

**Advanced:** FCP-0003 proposes the immutable-page successor, 128-bit opaque identifiers, compact authenticated locators, fixed page geometry, canonical occupancy, deterministic split/deletion behavior, and batch semantics. Current Rust/Python evidence validates the policies against the existing 64-bit identifier and 88-byte locator research layout.

**Open:** The proposed byte layout and policies remain reviewable drafts. The executable layout does not yet match the proposed identifier or locator widths. Occupancy landing order, epoch allocation, and every material proposal objection still require maintainer disposition.

### 2. Persistent mixed writer

**Advanced:** Replacement, insertion, shared multi-`Put`, deletion, split, borrow, redistribution, merge, recursive underflow, root growth, and root collapse all have reusable authenticated byte paths. Mixed deletion-plus-other-operation batches now use canonical persistent regrouping instead of the full object/page rebuild fallback. Exact reuse requires complete byte-identical locator or child-reference page bodies. Dedicated fuzzing covers bounded mixed batches and caller-order determinism.

PR #40 independently reproduces three byte-significant transitions. Its pinned aggregate is `8470bdff6c3a12cc8a01382c61cb9fad35fc9656a82a33288f29c0f9807cb79b`.

**Open:** Compare canonical regrouping with the path-local repair planner to quantify avoidable rewrites. Add bounded sink/private-staging output for persistent commits without materializing the whole successor file. Migrate and regenerate evidence for the proposed epoch geometry.

### 3. Source history, rewrite, and transport

**Advanced:** Strict validation, lookup, history enumeration, report-only recovery, rewrite-all, selected rewrite, selected historical-state retention, and reproducible operation accounting exist for slices and bounded sources. Canonical output accepts strongly versioned payload sources. Strict active-file adapters borrow authenticated payload ranges and skip inactive history; selected adapters read only retained active payloads.

PRs #42 and #43 close the latest-active bounded-source gap: one non-ABA versioned `ReadAt` source can be strictly inventoried and streamed to canonical output under one cumulative budget without materializing the complete input or output. All active descriptors are authenticated, payload digests are rechecked, and version changes are terminal.

**Open:** Stream selected historical states from the versioned bounded source directly to canonical output. Add maintained authenticated HTTP/cloud adapters, provider-specific retry classification, real waits, jitter/authentication policy, native asynchronous cancellation, and realistic provider latency/billing/cache/concurrency measurements. Integrate private staging and publication for atomic visibility.

### 4. Repair and semantic compaction

**Advanced:** Generic graph traversal has bounded node, edge, and depth work. One reference-list profile has Rust and independent Python codecs. PR #41 composes dependency closure with selected authenticated streaming so graph planning finishes before output and only reachable active payloads are read. Compaction failures remain distinct from source/output failures.

**Open:** Obtain application-profile adoption and root-selection policy. Pin cross-language rewrite bytes for profile-defined cases. Define extension preservation, provenance and signature reissuance, large-graph spill, versioned bounded-source semantic streaming, and historical-state semantic compaction.

### 5. Vectors and fuzzing

**Advanced:** Valid, invalid, interrupted, fork, recovery, source, history, rewrite, occupancy, persistent replacement/insertion/deletion/multi-`Put`/mixed, compaction, retry, spill, active streaming, selected streaming, semantic-streaming, versioned inventory, and direct source-to-sink evidence exists. PR #40 adds independently generated mixed transition bytes rather than treating Rust output as the oracle.

**Open:** Produce a complete cross-language corpus for the eventually selected epoch geometry, including all structural transition boundaries. Add provider-specific retry/source traces, fresh-process restart and physical power-loss corpora, selected historical source-to-sink traces, and hostile concrete-provider traces.

### 6. Spill and publication

**Advanced:** Models and Unix tests cover bounded resources, private staging, symlink refusal, ownership cleanup, no-overwrite publication, synchronization ordering, explicit indeterminate outcomes, and injected failures before link, after link, after destination sync, and during private-name retirement.

**Open:** Descriptor-relative secure handles, effective-user ownership checks, encrypted spill framing and nonce management, fresh-process restart testing, physical power-loss qualification, network-filesystem policy, and platform qualification under `docs/security/PHASE_3_PRODUCTION_SPILL_REQUIREMENTS.md`.

### 7. Independent review

**Advanced:** The review packet and JSON manifest map wire, writer, source/output, transport, spill, and semantic claims to their evidence and open gates. Independent Python work covers occupancy, graph policy, profile coding, retry traces, spill transitions, and mixed transition bytes.

**Open:** Obtain separately maintained parser/writer review or assigned external reviewers. Disposition every blocker and high-severity finding. Independent evidence produced in this repository is not a substitute for external review.

### 8. Freshness and rollback

**Advanced:** Trusted checkpoint comparison distinguishes unpinned integrity, current state, authorized advance, rollback, and same-sequence fork. Guidance now separates file integrity from authority to pin or advance a checkpoint and requires crash-consistent checkpoint persistence.

**Open:** Integrate the guidance into applications, user interfaces, durable checkpoint stores, and concrete transport adapters. Define application-specific authorization and trust-reset procedures.

### 9. Candidate 1 disposition

**Advanced:** The disposition draft recommends superseding Candidate 1 as the reusable-page baseline while retaining it as executable negative/security evidence.

**Open:** Maintainer decision and explicit transfer of every material FCP-0002 objection into FCP-0003, rejected alternatives, or a named later phase.

## Phase 3 completion rule

Phase 3 remains incomplete until one selected experimental layout has:

- an accepted, independently implementable proposal;
- bounded lookup, strict validation, linked history, and report-only recovery;
- persistent replacement, insertion, deletion, split, merge, redistribution, mixed batching, and root-height behavior;
- deterministic large-writer and qualified production spill behavior;
- adopted semantic dependency and preservation contracts;
- complete cross-language valid, invalid, and transition vectors for the selected epoch;
- hostile source, operation, and filesystem evidence;
- realistic range-I/O measurements;
- independent implementation or external review;
- explicit freshness authorization and durable checkpoint guidance;
- maintainer disposition of Candidate 1 and proposal objections.

Technical success in one branch does not allocate an epoch, accept an FCP, close a production gate, or supersede external review.
