# Phase 3 Frontier Tracker

**Updated:** 2026-07-31  
**Purpose:** Keep implementation evidence, proposal decisions, and external dependencies separate and reviewable.

## Active stacked drafts

| Frontier | Branch / PR | Current claim | Remaining gate |
|---|---|---|---|
| Mixed byte-writer baseline | `phase-3/reusable-mixed-batch-baseline` / #4 | Deterministic insert, replace, and delete batch with strict revalidation | Persistent shape-changing updates |
| Conditional source and freshness | `phase-3/conditional-source-freshness` / #5 | One strong source version per synchronous assurance operation; explicit trusted freshness checkpoint | Maintained HTTP/cloud adapters and native async cancellation |
| Semantic compaction | `phase-3/semantic-compaction` / #6 | Dependency-complete active rewrite relative to caller resolver and unknown policy | Profile resolver contracts, history retention, extension/provenance policy |
| Persistent replacement writer | `phase-3/persistent-batch-writer` / #7 | Arbitrary-depth copy-on-write replacement paths with exact page reuse | Persistent insertion, split, redistribution, merge, underflow, and root collapse |
| Successor convergence packet | `phase-3/successor-convergence-packet` / #8 | Draft epoch proposal, disposition, spill requirements, occupancy blocker, and independent review handoff | Maintainer and external review disposition |
| Source rewrite and compaction | `phase-3/source-rewrite-compaction` / #14 | Bounded-source all/selected rewrite and dependency compaction without a whole-file input buffer | Constant-memory output, spill integration, historical retention, and concrete versioned adapters |
| Persistent insertion writer | `phase-3/persistent-insertion-writer` / #15 | One absent object through a persistent arbitrary-depth path with leaf/internal split propagation and root-height increase | Canonical multi-insertion planner and persistent deletion |

PRs #5–#8 intentionally share PR #4 as their review baseline. PR #14 stacks on PR #6. PR #15 stacks on PR #7. They should be reviewed and landed independently where possible, then rebased or combined only after their assurance boundaries remain clear.

## Frontier status

### 1. Successor epoch and normative policy

**Advanced:** FCP-0003 Draft proposes an immutable-page successor, 128-bit opaque identifiers, compact authenticated locators, fixed page geometry, deterministic occupancy/split/deletion rules, batch semantics, and acceptance evidence. Experiment 0054 records that the current maximum-packing writer and pinned 400-object vector do not satisfy the proposed half-full non-root occupancy rule.

**Open:** Issue #16 tracks canonical occupancy convergence. Selecting half-full occupancy requires final-two-page redistribution, new Rust/Python construction, regenerated vectors, and deletion integration against the accepted invariant. Review may revise every proposed byte and policy; the current executable microformat is not yet the proposed epoch specification.

### 2. Persistent mixed writer

**Advanced:** Reusable writer supports deterministic complete mixed batches, copy-on-write arbitrary-depth replacement batches, and one persistent absent-object insertion with leaf/internal split propagation and root-height increase.

**Open:** Integrate a shared-path planner for multiple insertions and replacements. Persistent deletion must wait for, or explicitly isolate itself from, the occupancy-policy convergence because current research genesis may contain a sparse final page below the proposed minimum.

### 3. Source history, recovery, rewrite, and transport

**Advanced:** Slice and bounded-source strict validation, lookup, history, and recovery exist. Conditional source adapter binds one operation to one strong version and checks cancellation/deadline before accepting returned bytes. Source rewrite and semantic compaction strictly validate, inventory, and reread selected records under one cumulative budget without copying the complete source into one contiguous buffer.

**Open:** Constant-memory output streaming, concrete authenticated HTTP/cloud adapters, operation-wide retry budgets, native asynchronous cancellation, and source-based selected historical retention.

### 4. Repair and semantic compaction

**Advanced:** Strict verified-source rewrite-all, caller-selected rewrite, policy-driven dependency traversal, and source-backed equivalents exist. Unknown dependency semantics either abort or retain the full active set.

**Open:** Profile-specific resolver conformance, selected historical snapshot retention, extension preservation, provenance reissuance, signatures, constant-memory output, and large-graph spill.

### 5. Vectors and fuzzing

**Advanced:** Valid, invalid, interrupted, fork, recovery, support-profile, source, history, rewrite, persistent-replacement, and persistent-insertion evidence exists across Rust and Python research layers. Dedicated cargo-fuzz targets exercise successor assurance and writer APIs.

**Open:** Cross-language canonical shape-transition vectors under the selected occupancy policy, semantic-compaction vector generator with pinned identities, hostile conditional-source and source-rewrite traces, spill fault corpora, and independently generated vectors.

### 6. Spill and publication

**Advanced:** Models cover bounded sort/merge, descriptor limits, private staging, ownership cleanup, symlink refusal, no-overwrite publication, and synchronization ordering.

**Open:** Implement the production requirements in `docs/security/PHASE_3_PRODUCTION_SPILL_REQUIREMENTS.md`, including encrypted spill, platform qualification, fault injection, indeterminate publication reporting, and restart policy.

### 7. Independent review

**Advanced:** A clean-room packet now defines wire, parser, writer, security, and transport tracks plus vector and finding requirements.

**Open:** Assign external reviewers or obtain a separately maintained implementation; disposition all blocker and high-severity findings.

### 8. Freshness and rollback

**Advanced:** Trusted checkpoint comparison explicitly distinguishes unpinned integrity, current state, advancing state, rollback, and same-sequence fork.

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
