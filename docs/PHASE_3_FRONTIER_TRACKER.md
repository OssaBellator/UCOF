# Phase 3 Frontier Tracker

**Updated:** 2026-07-31  
**Purpose:** Keep implementation evidence, proposal decisions, and external dependencies separate and reviewable.

## Active stacked drafts

| Frontier | Branch / PR | Current claim | Remaining gate |
|---|---|---|---|
| Mixed byte-writer baseline | `phase-3/reusable-mixed-batch-baseline` / #4 | Deterministic insert, replace, and delete batch with strict revalidation | Shared persistent multi-operation planning |
| Conditional source and freshness | `phase-3/conditional-source-freshness` / #5 | One strong source version per synchronous assurance operation; explicit trusted freshness checkpoint | Maintained HTTP/cloud adapters and native async cancellation |
| Semantic compaction | `phase-3/semantic-compaction` / #6 | Dependency-complete active rewrite relative to caller resolver and unknown policy | Profile resolver contracts, history retention, extension/provenance policy |
| Persistent replacement writer | `phase-3/persistent-batch-writer` / #7 | Arbitrary-depth copy-on-write replacement paths with exact page reuse | Shared shape-changing planner |
| Successor convergence packet | `phase-3/successor-convergence-packet` / #8 | Draft epoch proposal, disposition, spill requirements, and independent review handoff | Maintainer and external review disposition |
| Source rewrite and compaction | `phase-3/source-rewrite-compaction` / #14 | Bounded-source all/selected rewrite and dependency compaction without a whole-file input buffer | Constant-memory output, spill integration, historical retention, and concrete versioned adapters |
| Persistent insertion writer | `phase-3/persistent-insertion-writer` / #15 | One absent object through a persistent arbitrary-depth path with leaf/internal split propagation and root-height increase | Shared multi-operation insertion planning |
| Canonical occupancy writer | `phase-3/canonical-occupancy-writer` / #17 | Deterministic final-two-page redistribution, strict canonical occupancy, independent Python construction, and regenerated 400-object identity | Review, landing order, and broader epoch migration |
| Occupancy policy algorithm | `phase-3/occupancy-policy-algorithm` / #18 | Exact grouping pseudocode, boundary tables, insertion preservation, deletion precondition, and vector requirements | Proposal disposition |
| Conditional retry budget | `phase-3/conditional-retry-budget` / #19 | Operation-wide bounded metadata/range attempts with retryable/terminal separation | Concrete adapter classification, backoff, and native async cancellation |
| Spill publication state | `phase-3/spill-publication-state-machine` / #20 | Ownership, resource, cleanup, no-overwrite, and indeterminate-outcome policy model | Filesystem adapters, encrypted framing, fault injection, and platform qualification |
| Persistent deletion writer | `phase-3/persistent-deletion-writer` / #21 | Draft single-object left-first borrow, merge, recursive internal repair, and root collapse | Behavioral CI, dedicated fuzz campaign, and shared multi-operation planning |
| Semantic compaction recipes | `phase-3/semantic-compaction-vectors` / #22 | Independent Python graph-policy verifier with ten pinned cases and aggregate identity | Profile-specific graph contracts and byte-rewrite identities |

PRs #5–#8 intentionally share PR #4 as their review baseline. PR #14 and #22 stack on PR #6. PR #15 stacks on PR #7. PR #17 stacks on PR #15. PR #21 stacks on PR #17. PRs #18 and #20 stack on PR #8. PR #19 stacks on PR #5. They should be reviewed and landed independently where possible, then rebased or combined only after their assurance boundaries remain clear.

## Frontier status

### 1. Successor epoch and normative policy

**Advanced:** FCP-0003 Draft proposes an immutable-page successor, 128-bit opaque identifiers, compact authenticated locators, fixed page geometry, deterministic occupancy/split/deletion rules, batch semantics, and acceptance evidence. PR #18 specifies the exact final-two-page occupancy algorithm. PR #17 implements that algorithm for the current research layout in Rust and independent Python and pins the changed 400-object identity.

**Open:** Review may revise every proposed byte and policy. The current executable microformat still uses 64-bit identifiers and 88-byte locators rather than the proposed epoch widths. Issue #16 remains open until occupancy policy, landing order, vector migration, and epoch disposition are accepted.

### 2. Persistent mixed writer

**Advanced:** Reusable writer supports deterministic complete mixed batches, arbitrary-depth copy-on-write replacement batches, one persistent absent-object insertion with split propagation, and a draft persistent deletion path against canonical occupancy.

**Open:** Complete PR #21 verification, then integrate a shared planner for multiple insertions, replacements, and deletions without order-dependent intermediate trees or repeated shared-ancestor writes.

### 3. Source history, recovery, rewrite, and transport

**Advanced:** Slice and bounded-source strict validation, lookup, history, and recovery exist. Conditional source adapters bind one operation to one strong version and validate returned range metadata. PR #19 adds one operation-wide bounded attempt budget across metadata and ranges and retries only explicitly transient failures. Source rewrite and semantic compaction strictly validate, inventory, and reread selected records under one cumulative budget without copying the complete source into one contiguous buffer.

**Open:** Constant-memory output streaming, maintained authenticated HTTP/cloud adapters, provider-specific safe retry classification, native asynchronous cancellation, and source-based selected historical retention.

### 4. Repair and semantic compaction

**Advanced:** Strict verified-source rewrite-all, caller-selected rewrite, policy-driven dependency traversal, and source-backed equivalents exist. Unknown dependency semantics either abort or retain the full active set. PR #22 independently reproduces graph selection, cycle handling, failures, and limits without invoking Rust.

**Open:** Profile-specific resolver conformance, selected historical snapshot retention, extension preservation, provenance reissuance, signatures, constant-memory output, large-graph spill, and pinned byte-rewrite identities.

### 5. Vectors and fuzzing

**Advanced:** Valid, invalid, interrupted, fork, recovery, support-profile, source, history, rewrite, persistent-replacement, persistent-insertion, canonical-occupancy, and semantic-compaction policy evidence exists across Rust and Python research layers. Dedicated cargo-fuzz targets exercise successor assurance and writer APIs; PR #21 adds a deletion-specific target.

**Open:** Complete deletion fuzz verification, cross-language deletion transition bytes, hostile conditional-source retry traces, source-rewrite mutation/transport traces, spill fault corpora, and independently generated full epoch vectors.

### 6. Spill and publication

**Advanced:** Models cover bounded sort/merge, descriptor limits, private staging, ownership cleanup, symlink refusal, no-overwrite publication, synchronization ordering, and now an explicit publication outcome state machine that cannot collapse ambiguity into ordinary failure.

**Open:** Implement the production requirements in `docs/security/PHASE_3_PRODUCTION_SPILL_REQUIREMENTS.md`, including concrete secure filesystem handles, encrypted spill framing, nonce management, platform qualification, transition fault injection, power-loss tests, and restart policy.

### 7. Independent review

**Advanced:** A clean-room packet defines wire, parser, writer, security, and transport tracks plus vector and finding requirements. Independent Python construction now covers canonical occupancy and semantic-compaction graph policy.

**Open:** Assign external reviewers or obtain a separately maintained complete parser/writer implementation; disposition all blocker and high-severity findings.

### 8. Freshness and rollback

**Advanced:** Trusted checkpoint comparison explicitly distinguishes unpinned integrity, current state, advancing state, rollback, and same-sequence fork. Retry policy preserves strong-version binding and does not retry version/protocol failures.

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
- repair and semantic compaction with profile dependency and preservation contracts;
- cross-language valid/invalid/transition vectors;
- hostile source, operation, and filesystem evidence;
- realistic range-I/O measurements;
- independent implementation or external review;
- explicit freshness guidance;
- maintainer disposition of Candidate 1 and proposal objections.

Technical success in one branch does not allocate an epoch, accept an FCP, or close another frontier automatically.
