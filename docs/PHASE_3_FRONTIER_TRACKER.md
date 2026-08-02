# Phase 3 Frontier Tracker

**Updated:** 2026-08-02  
**Purpose:** Keep verified implementation evidence, work still under verification, proposed policy, and external or production gates separate.

## Evidence boundary

This tracker records only repository evidence. A green research branch does not allocate an epoch, accept FCP-0003, establish provider or filesystem behavior, adopt an application profile, qualify production use, or substitute for independent review.

The Phase 3 work remains a set of stacked draft pull requests. Preserve each pull request's existing base and review boundary; do not flatten independent frontiers into one branch merely because they share research code.

## Current implementation frontiers

| Frontier | PRs | Current repository evidence | Remaining gate |
|---|---:|---|---|
| Canonical occupancy | #17, #18 | Deterministic grouping, strict validation, independent construction, boundary tables, and regenerated identities | FCP disposition and migration to the proposed identifier/locator geometry |
| Owned persistent writing | #21, #23, #27, #30, #34, #37 | Replacement, insertion, deletion, recursive repair, shared multi-`Put`, canonical mixed updates, exact page identity, and reuse accounting | Proposed-epoch migration and global minimality policy |
| Complete append-tail writer stack | #47, #54, #57, #58, #59, #60 | Byte-identical bounded base-plus-tail output for all five current mutation modes behind one borrowed-payload dispatcher | Composition with source-backed planners, maintained adapters, and publication |
| Bounded and version-bound base copying | #61, #62 | Two-pass whole-file identity verification, cumulative budgets, bounded I/O, tail withholding, and strong non-ABA checks around every source operation | Maintained provider adapters and proof that provider tokens satisfy the non-ABA contract |
| Source-backed persistent planning | #65, #66, #67, #68, #69 | #65–#68 are recorded green for replacement, insertion, deletion, and shared insertion/replacement planning. #69 adds canonical mixed deletion-plus-other-operation planning at head `a84c47472cb233a27b4dc795525ed5b468c0b51b` | Complete #69 CI and hostile mixed-source fuzzing; then compose planning with private staged publication. The planners retain bounded active metadata and do not claim constant memory or minimal source traffic |
| Private staging and Unix publication | #63, #64 | Complete staged validation, private-file sync, explicit no-overwrite outcomes, directory durability, non-downgrading cleanup, and a concrete Unix hard-link backend | Descriptor-relative hardening, authenticated journal, encryption, power-loss/NFS policy, and platform qualification |
| Independent mixed transition evidence | #40 | Clean-room regeneration of stable-height, root-collapse, and root-growth transitions with pinned identities | Complete proposed-epoch transition corpus and external interoperability review |
| Bounded canonical output | #35, #36 | Canonical genesis output through bounded sinks from owned and strongly versioned payload sources | Maintained provider integration and staged publication composition |
| Active and selected rewrites | #38, #39 | Strict active-file validation, borrowed authenticated ranges, inactive-history skipping, selected output, and exact read accounting | Maintained provider integration and staged publication composition |
| Dependency profile and exact rewrite bytes | #22, #28, #48 | Bounded graph policy, canonical reference-list coding, malformed-corpus rejection, canonical traversal, and exact Rust/Python identities | Application-profile adoption, root authority, and preservation policy |
| Dependency-aware semantic streaming | #41, #45 | Selected authenticated streaming for active or exact historical states with bounded dependency closure and exact accounting | Large-graph spill, per-retained-snapshot semantic selection, and provenance/extension policy |
| Versioned source inventory and history output | #42, #43, #44, #45, #55 | Latest-state, exact-sequence, dependency-selected historical, and chronological multi-selected-state output under one non-ABA version and cumulative budget | Constant-memory multi-commit output, retry/auth integration, maintained adapters, and staged publication |
| Source history and measurements | #14, #24, #32 | Bounded strict/history/recovery/rewrite APIs, selected historical reissuance, hostile-source fuzzing, and reproducible counters | Real provider measurements and provenance/signature/extension policy |
| Retry, HTTP classification, waits, and freshness | #5, #19, #25, #31, #51, #56 | Strong-version operation binding, operation-wide retry budgets, independent traces, fail-closed HTTP classification, bounded jitter planning, cooperative waits, and freshness guidance | Maintained adapters, authentication-refresh execution, native async cancellation, and durable checkpoint stores |
| Spill publication and restart | #20, #26, #29, #33, #53, #63, #64 | Publication authority model, independent traces, Unix no-overwrite publication, integrated private staging, directory synchronization, fault injection, and conservative authenticated restart classification | Authenticated journal, encryption, descriptor-relative hardening, power-loss/NFS policy, and platform qualification |
| Proposal and external review packet | #8 | Draft FCP, Candidate 1 disposition, production spill requirements, freshness authorization, review manifest, and this tracker | Maintainer decisions, selected landing order, and external review findings |

## Stack map

- PRs #5–#8 share PR #4 as their early review baseline.
- Owned writer and append-tail chain: #7 → #15 → #17 → #21 → #23 → #27 → #30 → #34 → #37 → #47 → #54 → #57 → #58 → #59 → #60.
- Base-copy and publication branch: #60 → #61 → #62 → #63 → #64.
- Source-backed mutation-planning branch: #62 → #65 → #66 → #67 → #68 → #69.
- Output/source/history chain: #23 → #35 → #36 → #38 → #39 → #41 → #42 → #43 → #44 → #45 → #55.
- Source/history measurement chain: #6 → #14 → #24 → #32.
- Semantic-policy chain: #6 → #22 → #28 → #48.
- Retry chain: #5 → #19 → #25 → #31 → #51 → #56.
- Spill/restart chain: #8 → #20 → #26 → #29 → #33 → #53; persistent output joins through #63 and #64.
- Independent writer comparison and transition evidence remain in #40 and #50.
- Occupancy policy #18 is refreshed on #8. Descendant evidence remains draft until landing order and policy disposition are selected.

## Frontier status

### 1. Successor epoch and normative policy

**Advanced:** FCP-0003 proposes immutable pages, opaque identifiers, compact authenticated locators, fixed geometry, canonical occupancy, deterministic split/deletion behavior, and batch semantics. Current executable Rust/Python evidence tests those policy ideas against the existing research layout.

**Open:** The executable layout still differs from the proposed identifier and locator widths. Epoch allocation, occupancy landing order, migration and transition vectors, Candidate 1 disposition, and every material proposal objection require maintainer action. Green experiments are not proposal acceptance.

### 2. Persistent writers and source-backed planning

**Advanced:** The owned writer paths cover replacement, insertion, deletion, split, borrow, redistribution, merge, recursive underflow, root growth/collapse, shared multi-`Put`, and canonical mixed batches. Exact reuse requires byte-identical locator or child-reference page bodies.

PRs #47, #54, #57, #58, and #59 construct absolute-offset append tails for every current persistent mode. PR #60 dispatches those modes without cloning multi-`Put` payloads. PRs #61 and #62 add bounded two-pass source copying and strong non-ABA source-version checks.

The source-planning stack now extends beyond replacement-only planning:

- #65 plans replacement tails from a strongly versioned bounded source;
- #66 adds insertion and split/root-growth planning;
- #67 adds deterministic deletion repair and root collapse;
- #68 adds shared insertion/replacement multi-`Put` planning and is recorded green at `090fa2b1ffbdd2b437f6c1a6b7ff357243ba5dc8`;
- #69 adds canonical mixed deletion-plus-other-operation planning and exact owned-writer equivalence.

**Open:** #69 must complete its own full CI and hostile mixed-source fuzz gate before being described as green. Source planning still performs complete strict validation and whole-file identity passes and retains decoded active locator/page metadata; it does not claim constant memory or minimal source traffic. The next integration layer should combine a verified source plan with #63/#64 private staging without weakening version, budget, destination-visibility, or durability boundaries. Proposed-epoch migration and global rewrite minimality remain separate.

### 3. Publication durability

**Advanced:** #63 composes version-bound copying with private staging, complete staged length/SHA-256 validation, private synchronization, explicit no-overwrite outcomes, parent synchronization, retained private state for indeterminate outcomes, and cleanup that cannot downgrade durable success. #64 implements that contract as a path-based Unix research backend using private mode-0600 files, owner/link-count checks, hard-link publication, and destination/staging directory synchronization.

**Open:** The Unix backend is path-based and plaintext. Production-oriented work still needs descriptor-relative secure handles, effective-user and namespace policy, an authenticated durable journal, encryption and nonce management, physical power-loss evidence, network-filesystem policy, and supported-platform qualification. Do not treat successful research filesystem tests as production durability qualification.

### 4. Source history, rewrite, and transport

**Advanced:** Strict validation, lookup, linked-history enumeration, report-only recovery, rewrite-all, selected rewrite, selected historical retention, and reproducible accounting exist for slices and bounded sources. #42–#45 bind inventories and selected output to one non-ABA version. #55 reissues multiple selected linked-history states chronologically under one version and cumulative budget.

#51 classifies HTTP-style metadata and conditional ranges fail-closed. #56 executes accepted retry delays with bounded caller-supplied jitter and cooperative cancellation/deadline checks around each wait chunk.

**Open:** #55 owns the complete output history before sink copying. Add constant-memory multi-commit output and semantic selection independently for each retained state. Add maintained HTTP/cloud adapters, provider-specific request/body rules, single-use authentication refresh execution, native asynchronous cancellation, staged publication composition, and realistic latency/billing/cache/concurrency measurements.

### 5. Repair and semantic compaction

**Advanced:** Generic dependency traversal bounds node, edge, and depth work. One reference-list profile has Rust and independent Python codecs; #48 binds it to exact cross-language rewrite recipes. #41 composes dependency closure with selected authenticated active-file streaming, and #45 extends it to an authenticated historical prefix.

**Open:** Obtain application-profile adoption and root-selection authority. Define extension preservation, provenance/signature reissuance, large-graph spill, and semantic selection independently for every retained history state. Caller-supplied dependency graphs are not a normative application contract.

### 6. Vectors and fuzzing

**Advanced:** Existing evidence covers strict/recovery/history/source/rewrite paths; occupancy; owned persistent mutation modes; all five append-tail writers and dispatch; source-backed replacement, insertion, deletion, and multi-`Put`; unversioned and strong-version base copying; private staging/publication; semantic compaction; retry and HTTP classification; cooperative waits; spill transitions/restart; and versioned historical output. #40 and #48 provide independent byte or policy oracles.

**In verification:** #69's mixed-source target is a distinct gate because it combines canonical operation ordering, deletion repair, insertion/replacement records, exact page reuse, root collapse/growth, source mutation, and cumulative budgets. Its failed fuzz job was re-run on 2026-08-02; do not infer a code defect or green result until that attempt concludes.

**Open:** Produce the complete cross-language corpus for selected epoch geometry, maintained-provider traces, authenticated journal and physical power-loss corpora, descriptor-relative filesystem tests, and hostile maintained-adapter traces.

### 7. Independent review and freshness

**Advanced:** The review packet maps wire, writer, source/output, transport, spill, and semantic claims to evidence and open gates. Trusted checkpoint comparison distinguishes unpinned integrity, current state, authorized advance, rollback, and same-sequence fork.

**Open:** Obtain a separately maintained parser/writer review or assigned external reviewers. Disposition every blocker and high-severity finding. Integrate freshness guidance into applications, durable checkpoint stores, and concrete adapters.

### 8. Candidate 1 disposition

**Advanced:** The disposition draft recommends superseding Candidate 1 as the reusable-page baseline while retaining it as executable negative/security evidence.

**Open:** A maintainer must decide and explicitly transfer every material FCP-0002 objection into FCP-0003, rejected alternatives, or named later work.

## Current execution order

1. Complete and inspect the PR #69 fuzz re-run; only patch a reproduced code or harness defect.
2. Compose the proven source-planning result with private staging/publication without collapsing the #62→#65→#69 and #62→#63→#64 review branches prematurely.
3. Advance the transport frontier with a maintained adapter or a clearly scoped adapter-neutral execution layer, not additional policy-only classification.
4. Advance history/compaction with constant-memory multi-commit output and per-retained-state semantic selection while preserving fail-before-output behavior.
5. Advance publication with descriptor-relative and authenticated-journal evidence before making stronger durability claims.
6. Keep proposal convergence synchronized with verified PR heads and preserve all external/maintainer gates.

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
