# Phase 3 Frontier Tracker

**Updated:** 2026-08-01  
**Purpose:** Keep implementation evidence, proposal decisions, and external dependencies separate and reviewable.

## Active stacked drafts

| Frontier | Branch / PR | Current claim | Remaining gate |
|---|---|---|---|
| Mixed byte-writer baseline | `phase-3/reusable-mixed-batch-baseline` / #4 | Deterministic insert, replace, and delete batch with strict revalidation | Shared persistent batches containing deletion plus another operation |
| Conditional source and freshness | `phase-3/conditional-source-freshness` / #5 | One strong source version per synchronous assurance operation; explicit trusted freshness checkpoint | Maintained HTTP/cloud adapters and native async cancellation |
| Semantic compaction | `phase-3/semantic-compaction` / #6 | Dependency-complete active rewrite relative to caller resolver and unknown policy | Adopted profile contracts, extension/provenance policy |
| Persistent replacement writer | `phase-3/persistent-batch-writer` / #7 | Arbitrary-depth copy-on-write replacement paths with exact page reuse | Superseded by broader stacked writer paths for shape-changing operations |
| Successor convergence packet | `phase-3/successor-convergence-packet` / #8 | Draft epoch proposal, disposition, spill requirements, independent review handoff, and this tracker | Maintainer and external review disposition |
| Source rewrite and compaction | `phase-3/source-rewrite-compaction` / #14 | Bounded-source all/selected rewrite and dependency compaction without a whole-file input buffer | Constant-memory output, spill integration, and concrete versioned adapters |
| Persistent insertion writer | `phase-3/persistent-insertion-writer` / #15 | One absent object through a persistent arbitrary-depth path with split propagation and root growth | Superseded by shared multi-`Put` planning for larger insertion batches |
| Canonical occupancy writer | `phase-3/canonical-occupancy-writer` / #17 | Deterministic final-two-page redistribution, strict canonical occupancy, independent Python construction, and regenerated identity | Review, landing order, and broader epoch migration |
| Occupancy policy algorithm | `phase-3/occupancy-policy-algorithm` / #18 | Exact grouping pseudocode, boundary tables, insertion preservation, deletion precondition, and vector requirements | Proposal disposition and branch refresh against #8 |
| Conditional retry budget | `phase-3/conditional-retry-budget` / #19 | Operation-wide bounded metadata/range attempts with retryable/terminal separation | Concrete provider classification and native async cancellation |
| Spill publication state | `phase-3/spill-publication-state-machine` / #20 | Ownership, resource, cleanup, no-overwrite, and indeterminate-outcome policy model | Branch refresh against #8 and production filesystem implementation |
| Persistent deletion writer | `phase-3/persistent-deletion-writer` / #21 | Green single-object borrow, merge, recursive internal repair, root collapse, and dedicated fuzzing | Shared batches containing deletion plus another operation |
| Semantic compaction recipes | `phase-3/semantic-compaction-vectors` / #22 | Independent Python graph-policy verifier with ten pinned cases and aggregate identity | Profile-specific byte-rewrite identities |
| Shared persistent multi-`Put` writer | `phase-3/persistent-multi-put-writer` / #23 | Green shared-path insertion/replacement batches with canonical overflow and dedicated fuzzing | Mixed deletion integration |
| Selected source history rewrite | `phase-3/source-selected-history-rewrite` / #24 | Green selected verified-state retention, chronological reissuance, cumulative budgets, and hostile-source fuzzing | Constant-memory output and provenance/extension policy |
| Conditional retry traces | `phase-3/conditional-retry-traces` / #25 | Independent retry outcome/accounting corpus with pinned aggregate | Provider-specific traces and maintained adapter integration |
| Spill transition traces | `phase-3/spill-transition-traces` / #26 | Independent publication transition corpus with pinned aggregate | Real fault injection, restart, and power-loss evidence |
| Mixed deletion leaf plan | `phase-3/mixed-deletion-plan` / #27 | Green order-independent simultaneous leaf insert/replace/delete planning with deterministic split, borrow, and merge | Locator/page emission and exact reusable-reference integration |
| Reference-list dependency profile | `phase-3/reference-list-profile` / #28 | Green bounded canonical dependency payload, Rust resolver, independent Python codec, and pinned aggregate | Application-profile adoption and rewrite-byte vectors |
| Unix spill publication | `phase-3/unix-spill-publication` / #29 | Green private staging, exclusive file creation, real no-overwrite hard-link publication, directory sync, and cleanup tests | Descriptor-relative hardening, encryption, fault injection, and platform qualification |
| Recursive mixed tree plan | `phase-3/mixed-tree-plan` / #30 | Canonical internal grouping, root growth/collapse, and conservative ancestor rewrite planning | Complete behavioral matrix, exact reusable references, and byte emission |
| Conditional retry delays | `phase-3/conditional-backoff-plan` / #31 | Green capped exponential delay, bounded server minimum, cumulative budget, and deadline planning | Real wait integration, provider policy, jitter policy, and async cancellation |

PRs #5–#8 intentionally share PR #4 as their review baseline. PR #14 stacks on #6. PR #15 stacks on #7. PR #17 stacks on #15. PR #21 stacks on #17. PR #23 stacks on #21. PR #27 stacks on #23 and #30 stacks on #27. PRs #18 and #20 stack on #8. PR #19 stacks on #5; PR #25 stacks on #19 and #31 stacks on #25. PR #22 stacks on #6 and #28 stacks on #22. PR #24 stacks on #14. PR #26 stacks on #20 and #29 stacks on #26. They should be reviewed and landed independently where possible, then refreshed only after their assurance boundaries remain clear.

## Frontier status

### 1. Successor epoch and normative policy

**Advanced:** FCP-0003 Draft proposes an immutable-page successor, 128-bit opaque identifiers, compact authenticated locators, fixed page geometry, deterministic occupancy/split/deletion rules, batch semantics, and acceptance evidence. PR #18 specifies the exact final-two-page occupancy algorithm. PR #17 implements that algorithm for the current research layout in Rust and independent Python and pins the changed 400-object identity.

**Open:** Review may revise every proposed byte and policy. The executable microformat still uses 64-bit identifiers and 88-byte locators rather than the proposed epoch widths. Issue #16 remains open until occupancy policy, landing order, vector migration, and epoch disposition are accepted. PRs #18 and #20 require a clean refresh against the advanced #8 branch before landing.

### 2. Persistent mixed writer

**Advanced:** Reusable writer paths cover deterministic full mixed rebuilds; arbitrary-depth replacement batches; one insertion; shared multi-`Put` insertion/replacement batches; split propagation and root growth; one deletion; left/right borrow; merge; recursive internal repair; and root collapse. PR #27 now applies complete mixed operations simultaneously at leaf level against original ranges. PR #30 propagates the result into canonical internal shape and root-height decisions without false reuse claims.

**Open:** Integrate locators and payload records, derive exact reusable page references through structural shifts, emit authenticated leaf/internal pages, publish one commit, and move deletion-plus-other-operation batches off the full-rebuild byte path.

### 3. Source history, recovery, rewrite, and transport

**Advanced:** Slice and bounded-source strict validation, lookup, history, and recovery exist. Conditional adapters bind one operation to one strong version and validate returned range metadata. PR #19 adds one operation-wide attempt budget; PR #25 independently verifies retry outcome/accounting traces; PR #31 adds bounded delay and deadline planning. Source rewrite and semantic compaction operate under cumulative budgets without a whole-file input buffer. PR #24 retains selected verified historical active states and now has dedicated hostile-source fuzzing.

**Open:** Constant-memory output streaming, maintained authenticated HTTP/cloud adapters, provider-specific retry classification, real backoff/wait integration, native asynchronous cancellation, and preservation/reissuance policy for extensions and provenance.

### 4. Repair and semantic compaction

**Advanced:** Strict rewrite-all, selected rewrite, policy-driven dependency traversal, and source-backed equivalents exist. Unknown semantics either abort or retain the complete active set. PR #22 independently verifies graph selection and limits. PR #28 adds one concrete canonical reference-list profile with a Rust resolver and independent Python codec. PR #24 advances selected historical state retention separately from dependency-based active compaction.

**Open:** Adoption by an actual application profile, extension preservation, provenance reissuance, signatures, constant-memory output, large-graph spill, and pinned cross-language rewrite-byte identities for profile-defined cases.

### 5. Vectors and fuzzing

**Advanced:** Valid, invalid, interrupted, fork, recovery, source, history, rewrite, persistent replacement/insertion/deletion, shared multi-`Put`, occupancy, and compaction evidence exists across Rust and Python layers. Dedicated fuzz targets cover writer and assurance APIs. PR #24 adds selected-history hostile-source mutation evidence. PRs #25 and #26 add independent retry and spill transition corpora. PR #28 adds an independent profile codec.

**Open:** Cross-language deletion and mixed-operation transition bytes, provider-specific retry traces, Unix publication fault injection, restart/power-loss corpora, and independently generated complete epoch vectors.

### 6. Spill and publication

**Advanced:** Models cover bounded sort/merge, descriptor limits, private staging, ownership cleanup, symlink refusal, no-overwrite publication, synchronization ordering, and explicit indeterminate outcomes. PR #26 independently verifies the publication state transitions. PR #29 exercises real Unix exclusive creation, staged-file validation, hard-link no-overwrite publication, directory synchronization, destination preservation, and private-name retirement.

**Open:** Descriptor-relative secure handles, effective-user ownership checks, encrypted spill framing and nonce management, injected transition failures, power-loss and restart testing, network-filesystem policy, and platform qualification under `docs/security/PHASE_3_PRODUCTION_SPILL_REQUIREMENTS.md`.

### 7. Independent review

**Advanced:** A clean-room packet defines wire, parser, writer, security, and transport tracks plus vector and finding requirements. Independent Python work covers canonical occupancy, semantic-compaction graph policy, retry traces, spill transitions, and the reference-list dependency codec.

**Open:** Assign external reviewers or obtain a separately maintained complete parser/writer implementation; disposition every blocker and high-severity finding.

### 8. Freshness and rollback

**Advanced:** Trusted checkpoint comparison distinguishes unpinned integrity, current state, advancing state, rollback, and same-sequence fork. Retry policy preserves strong-version binding and does not retry version/protocol failures. Delay planning refuses waits that reach the remaining deadline.

**Open:** Integrate crash-consistent trusted checkpoint storage in applications and document authorization rules for accepting an advance.

### 9. Candidate 1 disposition

**Advanced:** The disposition draft recommends superseding Candidate 1 as the reusable-page baseline while retaining it as executable negative/security evidence.

**Open:** Maintainer decision and transfer of every material FCP-0002 objection into FCP-0003, rejected-alternative findings, or a named later phase.

## Phase 3 completion rule

Phase 3 remains incomplete until one selected experimental layout has:

- an accepted experimental proposal and self-contained independently implementable specification;
- bounded lookup, strict validation, linked history, and report-only recovery;
- persistent replacement, insertion, deletion, split, merge, redistribution, and root-height behavior in the reusable writer;
- deterministic large-writer and production spill policy implementation;
- repair and semantic compaction with adopted dependency and preservation contracts;
- cross-language valid, invalid, and transition vectors;
- hostile source, operation, and filesystem evidence;
- realistic range-I/O measurements;
- independent implementation or external review;
- explicit freshness guidance;
- maintainer disposition of Candidate 1 and proposal objections.

Technical success in one branch does not allocate an epoch, accept an FCP, or close another frontier automatically.
