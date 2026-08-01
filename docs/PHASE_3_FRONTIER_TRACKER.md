# Phase 3 Frontier Tracker

**Updated:** 2026-08-01  
**Purpose:** Keep verified implementation evidence, proposed policy, and external or production gates separate.

## Current verified implementation frontiers

| Frontier | PRs | Verified claim | Remaining gate |
|---|---:|---|---|
| Canonical occupancy | #17, #18 | Deterministic canonical grouping, strict validation, independent Python construction, boundary tables, and regenerated identities | FCP disposition and migration to the proposed identifier/locator geometry |
| Persistent authenticated writing | #21, #23, #27, #30, #34, #37 | Replacement, insertion, deletion, recursive repair, shared multi-`Put`, canonical mixed updates, exact page identity, and reuse accounting | Proposed-epoch migration and global minimality policy |
| Complete append-tail writer stack | #47, #54, #57, #58, #59, #60 | Byte-identical bounded base-plus-tail output for replacement, insertion, deletion, shared multi-`Put`, and canonical mixed batches behind one borrowed-payload dispatcher | Bounded source-backed mutation planning |
| Bounded and version-bound base copying | #61, #62 | Two-pass whole-file length/SHA-256 verification, cumulative read budgets, bounded I/O, tail withholding, and strong non-ABA checks around every length/range operation | Maintained provider adapters and source-backed tail planning |
| Private staging and Unix publication | #63, #64 | Complete staged validation, private-file sync, explicit no-overwrite outcomes, parent-directory durability, non-downgrading cleanup, and a concrete Unix hard-link backend | Descriptor-relative hardening, authenticated journal, encryption, power-loss/NFS policy, and platform qualification |
| Independent mixed transition evidence | #40 | Clean-room Python regeneration of stable-height, root-collapse, and root-growth transitions with pinned per-vector and aggregate SHA-256 identities | Complete proposed-epoch transition corpus and external interoperability review |
| Bounded canonical output | #35, #36 | Canonical genesis bytes through bounded sequential sinks from owned and strongly versioned payload sources, with incremental hashing and version-change failure | Integration with maintained provider adapters |
| Active and selected active rewrites | #38, #39 | Strict active-file validation, borrowed authenticated payload ranges, inactive-history skipping, caller-selected output, exact selected read accounting, and untouched-sink selection failures | Staged-publication composition and maintained provider integration |
| Dependency profile and rewrite bytes | #22, #28, #48 | Bounded graph policy, canonical reference-list payload coding, malformed-corpus rejection, canonical traversal, and exact Rust/Python rewrite identities | Application-profile adoption, root-selection authority, and preservation policy |
| Dependency-aware streaming compaction | #41, #45 | Selected authenticated streaming for active or exact historical states, exact reachable/orphan and reread accounting, and fail-before-source/output graph errors | Large-graph spill, per-snapshot semantic composition, and provenance/extension policy |
| Versioned source inventory and history output | #42, #43, #44, #45, #55 | Strict latest-state, exact-sequence, dependency-selected historical, and multi-selected-state chronological output over one non-ABA source version and cumulative budget | Maintained provider adapter, constant-memory multi-commit output, retry/auth integration, and staged publication composition |
| Source history and measurements | #14, #24, #32 | Bounded-source strict/history/recovery/rewrite APIs, selected historical reissuance, hostile-source fuzzing, and reproducible read/hash/allocation counters | Real provider measurements, extension policy, and provenance/signature policy |
| Retry, HTTP classification, waits, and freshness | #5, #19, #25, #31, #51, #56 | Strong-version operation binding, operation-wide retry budgets, independent traces, fail-closed HTTP classification, bounded jitter planning, cooperative waits, and freshness guidance | Maintained adapters, authentication refresh execution, native async cancellation, and durable checkpoint stores |
| Spill publication and restart | #20, #26, #29, #33, #53, #63, #64 | Publication authority model, independent traces, Unix no-overwrite publication, integrated version-bound private staging, directory synchronization, fault injection, and conservative authenticated fresh-process restart classification | Durable authenticated journal, encryption, descriptor-relative hardening, power-loss/NFS policy, and platform qualification |
| Proposal and external review packet | #8 | Draft FCP, Candidate 1 disposition, production spill requirements, freshness authorization, review manifest, and tracker | Maintainer decisions and external review findings |

All “verified” claims refer to the current research layout. They do not allocate an epoch, accept FCP-0003, establish provider behavior, or establish production suitability.

## Stack map

- PRs #5–#8 share PR #4 as their review baseline.
- Writer/source/publication chain: #7 → #15 → #17 → #21 → #23 → #27 → #30 → #34 → #37 → #47 → #54 → #57 → #58 → #59 → #60 → #61 → #62 → #63 → #64.
- Independent writer comparison and transition evidence: #40 and #50.
- Output/source chain: #23 → #35 → #36 → #38 → #39 → #41 → #42 → #43 → #44 → #45 → #55.
- Source/history chain: #6 → #14 → #24 → #32.
- Semantic-policy chain: #6 → #22 → #28 → #48.
- Retry chain: #5 → #19 → #25 → #31 → #51 → #56.
- Spill/restart chain: #8 → #20 → #26 → #29 → #33 → #53; persistent output joins it through #63 and #64.
- Occupancy policy #18 is refreshed on #8. Descendant evidence remains draft until landing order and policy disposition are selected.

## Frontier status

### 1. Successor epoch and normative policy

**Advanced:** FCP-0003 proposes immutable pages, 128-bit opaque identifiers, compact authenticated locators, fixed page geometry, canonical occupancy, deterministic split/deletion behavior, and batch semantics. Current Rust/Python evidence validates these policies against the existing 64-bit identifier and 88-byte locator research layout.

**Open:** The executable layout does not match the proposed identifier or locator widths. Epoch allocation, occupancy landing order, transition vectors, and every material proposal objection require maintainer disposition.

### 2. Persistent writer, tails, source copying, and publication

**Advanced:** Reusable authenticated byte paths cover replacement, insertion, deletion, split, borrow, redistribution, merge, recursive underflow, root growth/collapse, shared multi-`Put`, and canonical mixed batches. Exact reuse requires byte-identical locator or child-reference page bodies.

PRs #47, #54, #57, #58, and #59 construct absolute-offset append tails for every current persistent mode. They preserve owned-writer bytes, reports, page writes, and reuse accounting while copying the verified base and tail through bounded writes. PR #60 exposes one dispatcher that selects those five modes without cloning multi-`Put` payloads and includes sparse replacement footer accounting.

PR #61 copies an independently identified base from `ImmutableReadAt` without retaining the complete base. It precomputes a two-pass budget, hashes the whole source before output, rehashes while copying, and withholds the tail if the second pass changes. PR #62 brackets every source length and range operation with one strong non-ABA token; bytes from a changed range are rejected before reaching the sink.

PR #63 composes version-bound copying with private staging, complete staged length/SHA-256 validation, private synchronization, explicit no-overwrite link outcomes, parent synchronization, retained private state for indeterminate outcomes, and cleanup that cannot downgrade durable success. PR #64 implements that contract as a concrete path-based Unix harness with private mode-0600 files, owner/link-count checks, hard-link no-overwrite publication, and destination/staging directory synchronization.

**Open:** Tail construction still starts from in-memory validated mutation planning. Add bounded source-backed planning beginning with replacement-only path reads. The Unix backend remains path-based and plaintext; add descriptor-relative secure handles, effective-user policy, authenticated durable journaling, encryption, physical power-loss evidence, network-filesystem policy, and supported-platform qualification. Global minimality and proposed-epoch migration remain open.

### 3. Source history, rewrite, and transport

**Advanced:** Strict validation, lookup, history enumeration, report-only recovery, rewrite-all, selected rewrite, selected historical retention, and reproducible accounting exist for slices and bounded sources. Canonical output accepts strongly versioned payload sources. PRs #42–#45 bind inventories and selected output to one non-ABA source version. PR #55 reissues multiple selected linked-history states chronologically under one version and cumulative budget.

PR #51 classifies HTTP-style metadata and conditional ranges fail-closed. PR #56 executes retry delays with bounded caller-supplied jitter and cooperative cancellation/deadline checks around every wait chunk.

**Open:** The multi-state reissuer owns the complete output history before sink copying. Add constant-memory multi-commit output and semantic selection independently per retained state. Add maintained HTTP/cloud adapters, provider-specific request/body integration, authentication refresh, native asynchronous cancellation, staged-publication composition, and realistic latency/billing/cache/concurrency measurements.

### 4. Repair and semantic compaction

**Advanced:** Generic dependency traversal has bounded node, edge, and depth work. One reference-list profile has Rust and independent Python codecs. PR #48 binds it to exact cross-language rewrite recipes. PR #41 composes dependency closure with selected authenticated active-file streaming; PR #45 extends it to one authenticated historical prefix.

**Open:** Obtain profile adoption and root-selection authority. Define extension preservation, provenance/signature reissuance, large-graph spill, and semantic selection independently for every retained history state.

### 5. Vectors and fuzzing

**Advanced:** Evidence covers strict/recovery/history/source/rewrite paths; occupancy; persistent replacement/insertion/deletion/multi-`Put`/mixed writing; all five tail writers and dispatcher; unversioned and strong-version base copying; private staging/publication; compaction; retry and HTTP classification; cooperative waits; spill transitions/restart; and versioned historical output. PRs #40 and #48 provide independent byte or policy oracles. PR #63 fuzzes destination visibility under arbitrary staging/link/failure states, and #64 adds real Unix filesystem tests.

**Open:** Produce the complete cross-language corpus for the selected epoch geometry, concrete-provider transport traces, authenticated journal and physical power-loss corpora, descriptor-relative filesystem tests, and hostile maintained-adapter traces.

### 6. Spill and publication

**Advanced:** Models and Unix tests cover bounded resources, private staging, symlink refusal, ownership cleanup, complete artifact validation, no-overwrite publication, synchronization order, explicit indeterminate outcomes, and injected failures. PR #53 adds conservative authenticated fresh-process classification. PRs #63 and #64 integrate version-bound persistent source output through private staging and a concrete Unix hard-link backend.

**Open:** Implement and qualify the durable authenticated journal, descriptor-relative secure handles, effective-user ownership checks, encrypted framing and nonce management, physical power-loss behavior, network-filesystem policy, and supported platforms.

### 7. Independent review and freshness

**Advanced:** The review packet maps wire, writer, source/output, transport, spill, and semantic claims to evidence and open gates. Trusted checkpoint comparison distinguishes unpinned integrity, current state, authorized advance, rollback, and same-sequence fork.

**Open:** Obtain separately maintained parser/writer review or assigned external reviewers. Disposition every blocker and high-severity finding. Integrate freshness guidance into applications, durable checkpoint stores, and concrete adapters.

### 8. Candidate 1 disposition

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

Technical success in one branch does not allocate an epoch, accept an FCP, close a provider or production gate, or supersede external review.
