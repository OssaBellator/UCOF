# Phase 3 Frontier Tracker

**Updated:** 2026-08-01  
**Purpose:** Keep implementation evidence, proposal decisions, and external dependencies separate and reviewable.

## Active stacked drafts

| Frontier | Branch / PR | Current claim | Remaining gate |
|---|---|---|---|
| Mixed byte-writer baseline | `phase-3/reusable-mixed-batch-baseline` / #4 | Deterministic insert, replace, and delete batch with strict revalidation | Shared persistent batches containing deletion plus another operation |
| Conditional source and freshness | `phase-3/conditional-source-freshness` / #5 | One strong source version per synchronous assurance operation; explicit trusted freshness checkpoint | Maintained HTTP/cloud adapters and native async cancellation |
| Semantic compaction | `phase-3/semantic-compaction` / #6 | Dependency-complete active rewrite relative to caller resolver and unknown policy | Adopted profile contracts and extension/provenance policy |
| Persistent replacement writer | `phase-3/persistent-batch-writer` / #7 | Arbitrary-depth copy-on-write replacement paths with exact page reuse | Superseded by broader stacked writer paths for shape-changing operations |
| Successor convergence packet | `phase-3/successor-convergence-packet` / #8 | Draft epoch proposal, disposition, spill requirements, independent review handoff, and this tracker | Maintainer and external review disposition |
| Source rewrite and compaction | `phase-3/source-rewrite-compaction` / #14 | Bounded-source all/selected rewrite and dependency compaction without a whole-file input buffer | Canonical streaming output integration, spill integration, and concrete versioned adapters |
| Persistent insertion writer | `phase-3/persistent-insertion-writer` / #15 | One absent object through a persistent arbitrary-depth path with split propagation and root growth | Superseded by shared multi-`Put` planning for larger insertion batches |
| Canonical occupancy writer | `phase-3/canonical-occupancy-writer` / #17 | Deterministic final-two-page redistribution, strict canonical occupancy, independent Python construction, and regenerated identity | Review, landing order, and broader epoch migration |
| Occupancy policy algorithm | `phase-3/occupancy-policy-algorithm` / #18 | Refreshed exact grouping pseudocode, boundary tables, insertion preservation, deletion precondition, and vector requirements | Proposal disposition |
| Conditional retry budget | `phase-3/conditional-retry-budget` / #19 | Operation-wide bounded metadata/range attempts with retryable/terminal separation | Concrete provider classification and native async cancellation |
| Spill publication state | `phase-3/spill-publication-state-machine` / #20 | Ownership, resource, cleanup, no-overwrite, and indeterminate-outcome policy model | Atomic descendant-chain refresh and production filesystem completion |
| Persistent deletion writer | `phase-3/persistent-deletion-writer` / #21 | Green single-object borrow, merge, recursive internal repair, root collapse, and dedicated fuzzing | Shared batches containing deletion plus another operation |
| Semantic compaction recipes | `phase-3/semantic-compaction-vectors` / #22 | Independent Python graph-policy verifier with ten pinned cases and aggregate identity | Profile-specific rewrite-byte identities |
| Shared persistent multi-`Put` writer | `phase-3/persistent-multi-put-writer` / #23 | Green shared-path insertion/replacement batches with canonical overflow and dedicated fuzzing | Mixed deletion integration |
| Selected source history rewrite | `phase-3/source-selected-history-rewrite` / #24 | Green selected verified-state retention, chronological reissuance, cumulative budgets, and hostile-source fuzzing | Canonical streaming sink integration and provenance/extension policy |
| Conditional retry traces | `phase-3/conditional-retry-traces` / #25 | Independent retry outcome/accounting corpus with pinned aggregate | Provider-specific traces and maintained adapter integration |
| Spill transition traces | `phase-3/spill-transition-traces` / #26 | Independent publication transition corpus with pinned aggregate | Restart and power-loss evidence |
| Mixed deletion leaf plan | `phase-3/mixed-deletion-plan` / #27 | Green simultaneous leaf insert/replace/delete planning with deterministic split, borrow, and merge | Locator/page emission integration |
| Reference-list dependency profile | `phase-3/reference-list-profile` / #28 | Green bounded dependency payload, Rust resolver, independent Python codec, and pinned aggregate | Application-profile adoption and rewrite-byte vectors |
| Unix spill publication | `phase-3/unix-spill-publication` / #29 | Green private staging, exclusive creation, real no-overwrite hard-link publication, directory sync, and cleanup | Descriptor-relative hardening, encryption, restart/power-loss, and platform qualification |
| Recursive mixed tree plan | `phase-3/mixed-tree-plan` / #30 | Green canonical internal grouping, root growth/collapse, and conservative ancestor planning | Exact reusable references and byte emission |
| Conditional retry delays | `phase-3/conditional-backoff-plan` / #31 | Green capped exponential delay, bounded server minimum, cumulative budget, and deadline planning | Real wait integration, provider policy, jitter policy, and async cancellation |
| Source I/O profile | `phase-3/source-io-profile` / #32 | Green reproducible strict/lookup/rewrite read, hash, and allocation counters through 1,000 objects | Real HTTP/cloud latency, billing, caching, and concurrency measurements |
| Unix publication fault injection | `phase-3/unix-spill-fault-injection` / #33 | Green real side-effect injection before link, after link, after directory sync, and during retirement | Fresh-process restart and physical power-loss testing |
| Exact mixed page-reference plan | `phase-3/mixed-reference-plan` / #34 | Green exact original-page reuse from unchanged leaf contents and child-reference sequences | Locator/digest integration and authenticated byte emission |
| Streaming canonical genesis output | `phase-3/streaming-genesis-output` / #35 | Bounded sequential sink, preflighted canonical shape, incremental hashing, and byte-equivalence tests | Full matrix, source-backed payload streaming, and rewrite integration |

PRs #5–#8 intentionally share PR #4 as their review baseline. PR #14 stacks on #6. PR #15 stacks on #7. PR #17 stacks on #15. PR #21 stacks on #17. PR #23 stacks on #21. PR #27 stacks on #23, #30 stacks on #27, and #34 stacks on #30. PR #35 also stacks on #23 as an independently reviewable output path. PRs #18 and #20 stack on #8; #18 has been refreshed, while #20 and its descendants remain one chain. PR #19 stacks on #5; PR #25 stacks on #19 and #31 stacks on #25. PR #22 stacks on #6 and #28 stacks on #22. PR #24 stacks on #14 and #32 stacks on #24. PR #26 stacks on #20; #29 stacks on #26 and #33 stacks on #29. Branches should be refreshed only when descendants can remain coherent and assurance boundaries stay explicit.

## Frontier status

### 1. Successor epoch and normative policy

**Advanced:** FCP-0003 Draft proposes an immutable-page successor, 128-bit opaque identifiers, compact authenticated locators, fixed page geometry, deterministic occupancy/split/deletion rules, batch semantics, and acceptance evidence. PR #18 specifies the exact final-two-page occupancy algorithm and is refreshed on the current convergence packet. PR #17 implements that algorithm for the current research layout in Rust and independent Python and pins the changed 400-object identity.

**Open:** Review may revise every proposed byte and policy. The executable microformat still uses 64-bit identifiers and 88-byte locators rather than the proposed epoch widths. Issue #16 remains open until occupancy policy, landing order, vector migration, and epoch disposition are accepted. PR #20's descendant chain requires a coordinated refresh rather than an isolated base rewrite.

### 2. Persistent mixed writer

**Advanced:** Reusable writer paths cover deterministic full mixed rebuilds; arbitrary-depth replacement batches; one insertion; shared multi-`Put` insertion/replacement batches; split propagation and root growth; one deletion; left/right borrow; merge; recursive internal repair; and root collapse. PR #27 applies complete mixed operations simultaneously at leaf level. PR #30 propagates the result through canonical internal shape and root-height decisions. PR #34 identifies exactly reusable original abstract pages by unchanged leaf contents and unchanged child-reference sequences. PR #35 begins canonical sequential byte emission without a whole-output buffer.

**Open:** Carry real locators and payload records through the mixed planner, assign offsets and digests, prove abstract identity equality implies byte equality, emit authenticated changed pages, publish one mixed commit, and move deletion-plus-other-operation batches off the full-rebuild path. Integrate source-backed payload streaming with the canonical sink.

### 3. Source history, recovery, rewrite, and transport

**Advanced:** Slice and bounded-source strict validation, lookup, history, and recovery exist. Conditional adapters bind one operation to one strong version and validate returned range metadata. PR #19 adds one operation-wide attempt budget; PR #25 independently verifies retry traces; PR #31 adds bounded delay/deadline planning. Source rewrite and semantic compaction operate under cumulative budgets without a whole-file input buffer. PR #24 retains selected verified historical states with dedicated hostile-source fuzzing. PR #32 records reproducible strict, lookup, and selected-rewrite counters through 1,000 objects with a 16 KiB maximum temporary allocation.

**Open:** Canonical source-to-sink output integration, maintained authenticated HTTP/cloud adapters, provider-specific retry classification, real waits and jitter policy, native asynchronous cancellation, and preservation/reissuance policy for extensions and provenance. Real provider latency, billing, cache, and concurrency measurements remain required.

### 4. Repair and semantic compaction

**Advanced:** Strict rewrite-all, selected rewrite, policy-driven dependency traversal, and source-backed equivalents exist. Unknown semantics either abort or retain the complete active set. PR #22 independently verifies graph selection and limits. PR #28 adds a concrete canonical reference-list profile with a Rust resolver and independent Python codec. PR #24 advances selected historical state retention separately from dependency-based active compaction.

**Open:** Adoption by an application profile, extension preservation, provenance reissuance, signatures, canonical streaming rewrite output, large-graph spill, and pinned cross-language rewrite-byte identities for profile-defined cases.

### 5. Vectors and fuzzing

**Advanced:** Valid, invalid, interrupted, fork, recovery, source, history, rewrite, persistent replacement/insertion/deletion, shared multi-`Put`, occupancy, and compaction evidence exists across Rust and Python. Dedicated fuzz targets cover writer and assurance APIs. PR #24 adds selected-history mutation evidence. PRs #25 and #26 add independent retry and spill corpora. PR #28 adds an independent profile codec. PR #33 validates publication outcomes against real filesystem side effects. PRs #30 and #34 pin recursive shape and exact reuse decisions.

**Open:** Cross-language deletion and mixed-operation transition bytes, provider-specific retry traces, fresh-process restart/power-loss corpora, source-to-sink hostile I/O traces, and independently generated complete epoch vectors.

### 6. Spill and publication

**Advanced:** Models cover bounded sort/merge, descriptor limits, private staging, ownership cleanup, symlink refusal, no-overwrite publication, synchronization ordering, and explicit indeterminate outcomes. PR #26 independently verifies transition traces. PR #29 exercises real Unix exclusive creation, validation, hard-link publication, directory synchronization, destination preservation, and retirement. PR #33 injects failures at each authority boundary and confirms that an observable post-link state is indeterminate until destination-directory synchronization, while durable success survives later cleanup failure.

**Open:** Descriptor-relative secure handles, effective-user ownership checks, encrypted spill framing and nonce management, fresh-process restart testing, physical power-loss qualification, network-filesystem policy, and platform qualification under `docs/security/PHASE_3_PRODUCTION_SPILL_REQUIREMENTS.md`.

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
