# Phase 3 Frontier Tracker

**Updated:** 2026-08-02 18:54 Australia/Melbourne  
**Purpose:** Separate repository-verified research evidence from review, policy, integration, external, and production gates.

## Evidence boundary

This tracker records repository evidence only. A green research branch does not allocate an epoch, accept FCP-0003, establish maintained-provider behavior, qualify filesystem durability, adopt an application dependency profile, or replace independent review.

Preserve every pull request's current base and review boundary. Independent frontiers may share ancestors without being flattened into one branch.

## Verified frontier snapshot

The following heads were checked remotely on 2026-08-02. Each listed implementation pull request is open, mergeable, ready for review, and green across its applicable Rust, Phase 3 integration, Phase 3 evidence, immutable-successor vector, and Fuzz workflows.

| Frontier | Pull request | Verified head | Repository evidence |
|---|---:|---|---|
| Source-backed canonical mixed planning | #69 | `48d8d110d7105d8e4c92c56ed6af6c9247f27f3f` | Full matrix green. Hostile smoke fuzzing exposed three minimized defects; all have deterministic fixes or regressions. PR #68 predecessor is recorded green at `090fa2b1ffbdd2b437f6c1a6b7ff357243ba5dc8`. |
| Conditional authentication refresh execution | #71 | `b510f89ea2f8840692579513181e96a52e5e6f34` | Full matrix green. Executes at most one explicitly authorized refresh and one replay above PR #56. |
| Unix publication directory identity pinning | #72 | `0d0ad79b9f9c3c471d7ef8006e67d54652724a18` | Full matrix green. Detects staging or destination-parent replacement during one path-based publication lifecycle above PR #64. |
| Selected-history owned-output cap | #73 | `c13f5d8e5d81f8e665269137db908d3622e47cab` | Full matrix green. Adds a caller-selected cap while preserving fail-before-output semantics above PR #55. |
| Per-state historical semantic selection planning | #74 | `c7c37bfb6588f7b1911a3e5d49daf66d40edbad0` | Full matrix green. Plans each retained state's graph and roots independently above PR #45. |
| Proposal and convergence packet | #8 | `8fc542dbb8e27af42d4c32e3786d9d1b9901a797` before this refresh | Prior matrix green; this tracker refresh is the next convergence commit. |

A green tip establishes only the checks encoded on that branch. Review, stack order, maintained integration, external interoperability, and production qualification remain separate.

## Established Phase 3 evidence

The current repository evidence includes:

- deterministic canonical occupancy, strict validation, regenerated identities, and boundary tables;
- owned persistent replacement, insertion, deletion, split, borrow, redistribution, merge, recursive underflow, root growth/collapse, shared multi-`Put`, and canonical mixed mutation;
- bounded base-plus-tail streaming for all current persistent mutation modes;
- strong non-ABA source binding, cumulative source budgets, whole-file identity verification, and bounded source copying;
- source-backed replacement, insertion, deletion, shared multi-`Put`, and canonical mixed tail planning;
- private staged validation, synchronization, no-replace publication outcomes, parent durability, conservative cleanup, and a Unix research backend;
- strict/history/recovery/rewrite APIs for slices and bounded sources, exact sequence selection, and chronological selected-history reissuance;
- fail-closed HTTP-style classification, operation-wide retry accounting, bounded jitter planning, cooperative waits, and one explicitly authorized authentication refresh;
- dependency-selected active and historical output, independent exact-byte policy evidence, and per-state semantic selection planning;
- independent transition oracles, hostile source/fuzz evidence, reproducible accounting, and proposal review material.

## Current frontier status

### 1. Source-backed persistent planning

**Advanced**

PR #69 completes the current source-backed mutation-planning modes with canonical deletion-plus-other-operation planning. It preserves exact owned-writer tail bytes, reports, current-page accounting, active-tree page accounting, and authenticated page reuse while retaining bounded decoded locator/page metadata rather than the complete base file.

The retained fuzz diagnostics minimized and resolved three findings:

1. `immutable_successor_persistent_batch`, `[10, 65]` (`CkE=`): a stale harness expected full rebuild instead of the established copy-on-write insertion batch mode.
2. `immutable_successor_persistent_mixed_streaming`, `[249, 245, 202]` (`+fXK`): streaming publication used complete active-tree pages for footer `page_count_current`.
3. `immutable_successor_persistent_source_mixed`, `[10]` (`Cg==`): source planning repeated that footer/report conflation.

The contract is now explicit: footer `page_count_current` counts pages written in the current commit; `ImmutableReport.page_count` counts the complete active tree, including reused pages.

**Open**

- compose verified source plans with private staged publication without collapsing the #62→#65→#69 and #62→#63→#64 review branches prematurely;
- retain version, budget, destination-visibility, and durability boundaries across that composition;
- add maintained provider adapters and prove their version tokens satisfy the non-ABA contract;
- source planning still performs complete strict validation and whole-file identity passes and retains active metadata, so it does not claim constant memory or minimal source traffic;
- proposed-epoch migration and global rewrite-minimality policy remain separate.

### 2. Transport, retry, and authentication

**Advanced**

PR #56 provides cooperative wait execution around the existing strong-version, retry-budget, HTTP-classification, and jitter-planning evidence. PR #71 adds an adapter-neutral state machine that:

- invokes an application-owned refresher only after an explicitly authorized 401 classification;
- permits at most one refresh and one replay;
- classifies a second 401 terminally;
- checks cancellation and the monotonic deadline around every exchange and refresh call;
- returns transport and refresh failures directly without hidden retries or backoff.

**Open**

- maintained HTTP/cloud adapters and provider-specific request, response-body, redirect, and authentication rules;
- credential acquisition, redaction, refresh synchronization, and production credential-runtime qualification;
- native asynchronous cancellation and runtime integration;
- durable checkpoint stores and application freshness authorization;
- realistic latency, billing, cache, and concurrency measurements.

### 3. Publication durability

**Advanced**

PRs #63 and #64 provide private staging, complete staged validation, private synchronization, explicit no-overwrite outcomes, destination-parent synchronization, non-downgrading cleanup, and a concrete Unix hard-link backend.

PR #72 records staging and destination-parent `(device, inode)` identities after private staging begins and fails closed if either observed path resolves to a different directory before a later path-dependent operation. Tests cover stable delegation, staging-directory replacement, and destination-parent replacement.

**Open**

- the wrapper remains path-based; identity checks and following filesystem operations are separate syscalls and can race;
- descriptor-relative secure handles and `openat`/`linkat`-style resolution;
- effective-user, namespace, symlink, mount, and supported-platform policy;
- authenticated durable journal, encryption and nonce management;
- physical power-loss evidence and network-filesystem policy;
- production durability qualification.

### 4. Versioned history output

**Advanced**

PR #55 reissues multiple selected linked-history states chronologically under one strong source version and cumulative budget, with complete preflight before bounded sink writes.

PR #73 adds an explicit caller-selected cap for the complete owned output. An exact cap preserves canonical bytes, retained source/output mappings, source statistics, version binding, and sink request bounds. An undersized cap fails before the first sink byte.

**Open**

- the complete reissued output chain is still owned before sink copying; the cap bounds that allocation but does not make it constant-memory;
- construct and validate chronological commit tails incrementally without retaining the complete output chain;
- integrate retry/authentication, maintained adapters, and private staged publication;
- preserve or explicitly reissue provenance, extensions, signatures, and historical policy.

### 5. Semantic compaction and retained states

**Advanced**

PR #45 streams dependency-selected active objects from one exact historical state under one strong source version. PR #74 adds a bounded graph-only plan for multiple retained states:

- requests are canonicalized chronologically;
- duplicate sequences are rejected;
- every sequence supplies its own graph and trusted roots;
- closure, orphan set, edge count, and maximum depth are computed independently per state;
- a cumulative reachable-object bound covers the aggregate plan.

**Open**

- compose per-state plans with source-history membership, object-presence validation, and multi-state output;
- adopt a normative application dependency profile and root-selection authority;
- define extension preservation, provenance/signature reissuance, and missing-profile behavior;
- add large-graph spill and bounded durable planning;
- caller-supplied graphs remain research inputs, not a normative application contract.

### 6. Proposal and external review

**Advanced**

The proposal packet records FCP-0003, Candidate 1 disposition material, occupancy evidence, production spill requirements, freshness authorization, review manifests, stack topology, and verified frontier tips.

**Open**

- maintainer decisions on epoch allocation, occupancy landing order, selected stack order, migration, and Candidate 1 disposition;
- disposition of every material proposal objection;
- a separately maintained parser/writer implementation or assigned external reviewers;
- complete cross-language valid, invalid, and transition vectors for the selected epoch;
- external interoperability findings and high-severity review closure.

## Stack map

- Source-backed mutation planning: #62 → #65 → #66 → #67 → #68 → #69.
- Base-copy/publication: #60 → #61 → #62 → #63 → #64 → #72.
- Retry/transport: #5 → #19 → #25 → #31 → #51 → #56 → #71.
- Source/output/history: #23 → #35 → #36 → #38 → #39 → #41 → #42 → #43 → #44 → #45 → #55 → #73.
- Historical semantic child: #45 → #74.
- Semantic-policy evidence: #6 → #22 → #28 → #48.
- Spill/restart evidence: #8 → #20 → #26 → #29 → #33 → #53; persistent publication joins through #63, #64, and #72.
- Independent transition and writer evidence remain in #40 and #50.
- Proposal convergence remains on #8; it records evidence but does not merge implementation stacks.

## Review and landing constraints

1. Keep #69, #71, #72, #73, and #74 on their current bases during review.
2. Do not infer that ready-for-review means ready to merge out of stack order.
3. Do not combine source planning and publication until their independent review boundaries are preserved and the composition contract is explicit.
4. Do not describe #73 as constant-memory history output.
5. Do not describe #72 as descriptor-relative or race-free filesystem hardening.
6. Do not describe #74 as source-validated multi-state semantic output or an adopted application profile.
7. Green experiments do not allocate an epoch or accept FCP-0003.

## Current execution order

1. Obtain review and stack-order decisions for green PRs #69 and #71–#74.
2. Add a dedicated composition layer joining a verified source-backed plan to private staged publication while preserving source-version, budget, preflight, no-replace, and durability semantics.
3. Advance selected-history output from a bounded complete allocation to incremental chronological tail construction with fail-before-publication behavior.
4. Compose per-state semantic plans with source-history validation and multi-state output without reusing one global closure.
5. Advance Unix publication from path-identity detection to descriptor-relative secure handles and authenticated restart evidence.
6. Add a maintained transport adapter and durable freshness/checkpoint integration without weakening the green adapter-neutral contracts.
7. Keep this tracker synchronized with every verified head, blocker, review decision, and assurance-boundary change.

## Phase 3 completion rule

Phase 3 remains incomplete until one selected experimental layout has:

- an accepted, independently implementable proposal;
- bounded lookup, strict validation, linked history, and report-only recovery;
- persistent replacement, insertion, deletion, split, merge, redistribution, mixed batching, and root-height behavior;
- deterministic large-writer and qualified production spill behavior;
- adopted semantic dependency and preservation contracts;
- complete cross-language valid, invalid, and transition vectors for the selected epoch;
- hostile source, operation, transport, and filesystem evidence;
- realistic range-I/O and maintained-provider measurements;
- independent implementation or external review;
- explicit freshness authorization and durable checkpoint guidance;
- maintainer disposition of Candidate 1 and proposal objections.

Technical success in any one branch does not allocate an epoch, accept an FCP, close a provider or production gate, or supersede independent review.
