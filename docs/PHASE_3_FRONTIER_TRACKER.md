# Phase 3 Frontier Tracker

**Updated:** 2026-08-01  
**Purpose:** Keep verified implementation evidence, proposed policy, and external or production gates separate.

## Current verified implementation frontiers

| Frontier | PRs | Verified claim | Remaining gate |
|---|---:|---|---|
| Canonical occupancy | #17, #18 | Deterministic canonical grouping, strict validation, independent Python construction, boundary tables, and regenerated identities | FCP disposition and migration to the proposed identifier/locator geometry |
| Persistent single-operation and multi-`Put` writing | #21, #23 | Authenticated deletion, borrow, merge, recursive repair, root collapse, shared insertion/replacement paths, split propagation, and exact reuse accounting | Landing order, append-tail generalization, and wider-layout migration |
| Mixed and replacement append-tail output | #27, #30, #34, #37, #47, #50, #54 | Simultaneous mixed repair, exact page identity and reuse, authenticated canonical bytes, bounded base-plus-tail sink output, path-local rewrite comparison, and byte-identical replacement-only tail output | Insertion-only, deletion-only, and shared multi-`Put` tails; bounded source base; atomic staging; global minimality policy |
| Independent mixed transition evidence | #40 | Clean-room Python regeneration of stable-height, root-collapse, and root-growth transitions with pinned per-vector and aggregate SHA-256 identities | Complete proposed-epoch transition corpus and external interoperability review |
| Bounded canonical output | #35, #36 | Canonical genesis bytes through bounded sequential sinks from owned and strongly versioned payload sources, with incremental hashing and version-change failure | Atomic staging integration |
| Active and selected active rewrites | #38, #39 | Strict active-file validation, borrowed authenticated payload ranges, inactive-history skipping, caller-selected output, exact selected read accounting, and untouched-sink selection failures | Atomic staging integration |
| Dependency profile and rewrite bytes | #22, #28, #48 | Bounded graph policy, canonical reference-list payload coding, malformed-corpus rejection, canonical root/dependency traversal, and exact Rust/Python profile-defined rewrite byte identities | Application-profile adoption, root-selection authority, and preservation policy |
| Dependency-aware streaming compaction | #41, #45 | Selected authenticated streaming for active or exact historical states, exact reachable/orphan and reread accounting, and fail-before-source/output graph errors | Large-graph spill, per-snapshot semantic composition, and provenance/extension policy |
| Versioned bounded-source inventory and output | #42, #43, #44, #45, #55 | Strict latest-state, exact-sequence, dependency-selected historical, and multi-selected-state chronological output over one non-ABA source version and cumulative budget | Maintained provider adapter, constant-memory multi-commit output, per-snapshot semantic selection, retry/auth integration, and atomic publication staging |
| Source history and measurements | #14, #24, #32 | Bounded-source strict/history/recovery/rewrite APIs, selected historical reissuance, hostile-source fuzzing, and reproducible read/hash/allocation counters | Real provider measurements, extension policy, and provenance/signature policy |
| Retry, HTTP classification, waits, and freshness | #5, #19, #25, #31, #51, #56 | Strong-version operation binding, operation-wide attempt budgets, independent traces, fail-closed HTTP response classification, bounded delay/jitter planning, cooperative real waits, and explicit freshness authorization guidance | Maintained provider adapters, jitter distribution/seed policy, authentication refresh execution, native async cancellation, and checkpoint stores |
| Spill publication and restart | #20, #26, #29, #33, #53 | Publication authority model, independent traces, real Unix no-overwrite publication, directory synchronization, authority-boundary fault injection, and conservative authenticated fresh-process restart classification | Durable authenticated journal implementation, encryption, descriptor-relative hardening, effective-user ownership, physical power-loss, network-filesystem policy, and platform qualification |
| Proposal and external review packet | #8 | Draft FCP, Candidate 1 disposition recommendation, production spill requirements, freshness authorization, independent-review packet, manifest, and tracker | Maintainer decisions and external review findings |

All “verified” claims above refer to the current research layout. They do not allocate an epoch, accept FCP-0003, or establish production suitability.

## Stack map

- PRs #5–#8 share PR #4 as their review baseline.
- Writer chain: #7 → #15 → #17 → #21 → #23 → #27 → #30 → #34 → #37 → #47, with rewrite comparison in #50, replacement-tail generalization in #54, and independent transitions in #40.
- Output/source chain: #23 → #35 → #36 → #38 → #39 → #41 → #42 → #43 → #44 → #45 → #55.
- Source/history chain: #6 → #14 → #24 → #32.
- Semantic-policy chain: #6 → #22 → #28 → #48.
- Retry chain: #5 → #19 → #25 → #31 → #51 → #56.
- Spill chain: #8 → #20 → #26 → #29 → #33 → #53.
- Occupancy policy #18 is refreshed on #8. Descendant evidence remains draft until landing order and policy disposition are selected.

## Frontier status

### 1. Successor epoch and normative policy

**Advanced:** FCP-0003 proposes the immutable-page successor, 128-bit opaque identifiers, compact authenticated locators, fixed page geometry, canonical occupancy, deterministic split/deletion behavior, and batch semantics. Current Rust/Python evidence validates the policies against the existing 64-bit identifier and 88-byte locator research layout.

**Open:** The proposed byte layout and policies remain reviewable drafts. The executable layout does not yet match the proposed identifier or locator widths. Occupancy landing order, epoch allocation, and every material proposal objection still require maintainer disposition.

### 2. Persistent writer and append tails

**Advanced:** Replacement, insertion, shared multi-`Put`, deletion, split, borrow, redistribution, merge, recursive underflow, root growth, and root collapse all have reusable authenticated byte paths. Mixed deletion-plus-other-operation batches use canonical persistent regrouping instead of the full object/page rebuild fallback. Exact reuse requires complete byte-identical locator or child-reference page bodies.

PR #47 constructs only the mixed append tail, computes all new offsets from the verified base length, preserves exact page reuse, hashes the same linked commit, and copies the base plus tail through bounded writes. Its streamed bytes and report are identical to the owned writer, while avoiding a second complete successor-file allocation. Invalid operations fail before output; sink failure after output begins is terminal.

PR #54 reuses the same absolute-offset tail discipline for replacement-only batches. Multiple leaf-to-root replacement paths are rewritten into one tail, untouched references remain exact, caller order is canonicalized, and bytes, reports, page writes, and reuse accounting match the owned persistent writer.

PR #50 compares canonical authenticated page writes with the path-local planner. The pinned stable-height, root-collapse, and root-growth recipes choose identical final leaf partitions and equal write counts of 2, 1, and 3 pages. Broader fuzzing found no equal-final-partition case where canonical regrouping wrote more pages than the conservative path-local estimate.

PR #40 independently reproduces three byte-significant transitions. Its pinned aggregate is `8470bdff6c3a12cc8a01382c61cb9fad35fc9656a82a33288f29c0f9807cb79b`.

**Open:** Global minimality is not proven, and divergent valid final partitions remain a policy question. Generalize the base-offset tail abstraction to insertion-only, deletion-only, and shared multi-`Put` modes. Add bounded-source base copying, private staging, and publication. Migrate and regenerate evidence for the proposed epoch geometry.

### 3. Source history, rewrite, and transport

**Advanced:** Strict validation, lookup, history enumeration, report-only recovery, rewrite-all, selected rewrite, selected historical-state retention, and reproducible operation accounting exist for slices and bounded sources. Canonical output accepts strongly versioned payload sources. Strict active-file adapters borrow authenticated payload ranges and skip inactive history; selected adapters read only retained active payloads.

PRs #42 and #43 close the latest-active bounded-source gap: one non-ABA versioned `ReadAt` source can be strictly inventoried and streamed to canonical output under one cumulative budget without materializing the complete input or output. All active descriptors are authenticated, payload digests are rechecked, and version changes are terminal.

PR #44 selects one exact linked-history state. PR #45 composes exact historical selection with selected-object output and bounded dependency closure. PR #55 reissues multiple caller-selected linked-history states as one chronological output history under the same strong version and cumulative source budget. It preserves explicitly selected unchanged-state boundaries, validates the complete output history before writing, and matches the owned selected-history rewriter exactly.

PR #51 classifies HTTP-style metadata and conditional range responses fail-closed. It accepts only exact valid metadata and partial-range shapes, treats version changes and malformed responses as terminal, and retries only a small explicit status allowlist. PR #56 executes accepted delays with caller-supplied bounded jitter and cooperative cancellation/deadline checks before and after every wait chunk.

**Open:** The multi-state reissuer still owns the complete output history before sink copying. Add constant-memory multi-commit tail output and semantic selection independently per retained state. Add maintained authenticated HTTP/cloud adapters, provider-specific request/body integration, authentication refresh execution, native asynchronous cancellation, and realistic latency/billing/cache/concurrency measurements. Integrate private staging and publication for atomic visibility.

### 4. Repair and semantic compaction

**Advanced:** Generic graph traversal has bounded node, edge, and depth work. One reference-list profile has Rust and independent Python codecs. PR #48 binds that profile to three exact cross-language rewrite-byte recipes covering a dependency chain, two caller-unsorted roots sharing a dependency, and an empty reference root. Rust and Python agree on canonical roots, retained/discarded IDs, edges, depth, byte length, SHA-256, root level, page count, object count, payload semantics, and aggregate `fe20a891b04b90b6df1870e6652eec5d6ddfa91ebc8370f4d6dfa70881a27c84`.

PR #41 composes dependency closure with selected authenticated active-file streaming. PR #45 extends the same fail-before-source graph policy to one exact authenticated historical prefix under one version and cumulative budget. Compaction failures remain distinct from history/source/output failures.

**Open:** Obtain application-profile adoption and root-selection authority. Define extension preservation, provenance and signature reissuance, large-graph spill, and semantic selection independently for every retained state in a reissued history.

### 5. Vectors and fuzzing

**Advanced:** Valid, invalid, interrupted, fork, recovery, source, history, rewrite, occupancy, persistent replacement/insertion/deletion/multi-`Put`/mixed, mixed and replacement sink output, rewrite comparison, compaction, profile-defined rewrite bytes, retry, HTTP classification, cooperative waits, spill transitions and restart, active streaming, selected streaming, semantic streaming, versioned inventory, direct source-to-sink, selected-history source-to-sink, historical-semantic source-to-sink, and multi-selected-state history evidence exists. PR #40 and PR #48 provide independently generated byte or policy oracles rather than treating Rust as the sole source of truth.

**Open:** Produce a complete cross-language corpus for the eventually selected epoch geometry, including all structural transition boundaries. Add concrete-provider retry/source traces, authenticated journal/power-loss corpora, and hostile maintained-adapter traces.

### 6. Spill and publication

**Advanced:** Models and Unix tests cover bounded resources, private staging, symlink refusal, ownership cleanup, no-overwrite publication, synchronization ordering, explicit indeterminate outcomes, and injected failures before link, after link, after destination sync, and during private-name retirement.

PR #53 adds conservative fresh-process classification. A destination name alone is never durable authority. Durable classification requires an authenticated ownership-bound journal at or beyond destination-directory synchronization plus a matching regular destination. Foreign private state is preserved, valid owned staging is retained for retry, invalid owned staging may be removed only before publication, and contradictory durable records require manual intervention. Fresh-process tests, independent traces, and fuzzing cover those rules.

**Open:** Implement and qualify the authenticated durable journal itself, descriptor-relative secure handles, effective-user ownership checks, encrypted spill framing and nonce management, physical power-loss behavior, network-filesystem policy, and supported-platform qualification under `docs/security/PHASE_3_PRODUCTION_SPILL_REQUIREMENTS.md`.

### 7. Independent review

**Advanced:** The review packet and JSON manifest map wire, writer, source/output, transport, spill, and semantic claims to their evidence and open gates. Independent Python work covers occupancy, graph policy, profile coding and rewrite bytes, retry and HTTP traces, spill transitions and restart, and mixed transition bytes.

**Open:** Obtain separately maintained parser/writer review or assigned external reviewers. Disposition every blocker and high-severity finding. Independent evidence produced in this repository is not a substitute for external review.

### 8. Freshness and rollback

**Advanced:** Trusted checkpoint comparison distinguishes unpinned integrity, current state, authorized advance, rollback, and same-sequence fork. Guidance separates file integrity from authority to pin or advance a checkpoint and requires crash-consistent checkpoint persistence.

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
- hostile source, operation, transport, and filesystem evidence;
- realistic range-I/O measurements;
- independent implementation or external review;
- explicit freshness authorization and durable checkpoint guidance;
- maintainer disposition of Candidate 1 and proposal objections.

Technical success in one branch does not allocate an epoch, accept an FCP, close a production gate, or supersede external review.
