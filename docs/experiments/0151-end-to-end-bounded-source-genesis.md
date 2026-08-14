# Experiment 0151 — end-to-end bounded source genesis

**Status:** non-normative Phase 3 implementation evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0148–0150

## Purpose

Experiments 0148–0150 proved the required pieces separately:

- bounded fixed-record external sorting;
- fail-before-output source metadata staging and source reborrow;
- bounded locator staging and level-by-level page-reference staging.

This experiment composes those pieces into one test-only source-backed genesis writer and asks the stronger question: can canonical source genesis avoid workload-wide metadata, locator, and page-frontier vectors while preserving the current writer's exact bytes, report, source freshness checks, occupancy behavior, and pre-output failure guarantees?

The answer is yes for the research candidate described here.

This is still implementation evidence, not a production writer API.

## End-to-end pipeline

The candidate performs the following phases.

### 1. Metadata preflight before output

Every source contributes one fixed 64-byte private descriptor containing:

- object ID (`u64`);
- source index (`u64`);
- kind (`u16`);
- six reserved zero bytes;
- logical length (`u64`);
- strong version (`[u8; 32]`).

Descriptors are externally sorted by object ID through the Experiment 0148 bounded spill sorter. Duplicate IDs, invalid source metadata, source-version acquisition failure, tree-shape limits, exact output-size limits, and file-size limits are resolved before the first canonical output byte is written.

The sorted descriptor file is retained as a private stage after sorter-owned runs have been retired.

### 2. Canonical object streaming

After preflight completes, the source-iteration borrow has ended. The writer reads sorted descriptors one at a time, reborrows the indexed source, confirms object ID/kind/logical length still match the staged metadata, and invokes the existing canonical `write_source_streaming_object` helper with the staged strong version.

Each returned locator is written to the Experiment 0150 fixed 72-byte private locator stage rather than retained in a workload-wide vector.

The source payload buffer remains bounded by `max_source_read_bytes`.

### 3. Bounded canonical tree construction

Leaf construction reads only one canonical group from the locator stage at a time. The candidate checks allocation for exactly `LEAF_CAPACITY` locator values rather than object count.

Internal construction alternates fixed 64-byte `PageRef` stages level by level. The candidate checks allocation for exactly `INTERNAL_FANOUT` page references rather than a level-wide frontier.

The existing leaf/internal encoders and streaming page writer are reused, so the experiment does not define replacement page bytes.

### 4. Existing canonical publication bytes

After the staged tree returns the final root reference, the candidate invokes the existing streaming publication helper and verifies that the final output length is exactly the preflighted canonical length.

No new UCOF wire bytes are introduced.

## Allocation boundary

For tree construction, the research candidate replaces workload-scaled allocation checks with fixed geometry checks:

- at most 185 resident `Locator` values;
- at most 255 resident `PageRef` values;
- one source read buffer bounded by caller configuration;
- the bounded spill sorter's initial-run buffer, independently bounded by its `run_records` configuration;
- existing fixed page/object encoding buffers.

This is a stronger executable claim than merely observing that a particular workload used little memory.

## Low-allocation regression

A 2,003-object regression computes a `max_allocation_bytes` value sufficient for the larger of:

- 185 `Locator` values; or
- 255 `PageRef` values.

That limit is intentionally too small for the current `write_genesis_sources_to` workload-wide locator allocation. The existing writer rejects before output under that limit.

The bounded candidate succeeds under the same `ImmutableLimits`, and its bytes and `ImmutableSourceStreamingWriteReport` exactly match a baseline produced under the normal default allocation limit.

This directly demonstrates that the candidate's tree allocation requirement is geometry-bounded rather than object-count-bounded.

## Full-fanout three-level equivalence case

The primary regression uses **70,671** reverse-ordered one-byte source objects.

This count is chosen deliberately. It produces:

- 383 canonical leaf pages;
- an internal level partitioned as 255 and 128 child references;
- 2 level-1 internal pages;
- 1 level-2 root page;
- 386 pages total;
- root level 2.

Therefore the test genuinely reaches both fixed in-memory maxima:

- peak locator batch: 185 entries;
- peak page-reference batch: 255 entries.

The candidate output is required to match the current `write_genesis_sources_to` baseline byte-for-byte and report-for-report. It does.

The descriptor sorter is configured to create multiple runs, so this regression also exercises sorted staged metadata rather than an accidental in-memory single-run path.

## Pre-output failure evidence

Three adversarial regressions require both an untouched output sink and an empty private-stage directory after failure:

1. a duplicate object ID split across separate spill runs;
2. a strong-version acquisition failure after an earlier spill run has already completed;
3. an output-size limit discovered after descriptor sorting has produced staged metadata.

All three pass.

These cases are important because bounded staging is not useful if preflight failures leak partial canonical output or abandoned private files.

## Retained private-stage accounting

The candidate reports:

- final descriptor-stage bytes;
- the bounded sorter's full `BoundedSpillSortReport`;
- peak resident locator entries;
- peak resident page-reference entries;
- peak live **retained-stage** bytes across descriptor→locator and locator/page-reference transitions.

That reporting closes an observability gap from Experiment 0149, but it is not yet a single enforced private-storage quota.

## Important storage-budget gap

The sorter and retained stages currently have separate limits.

A production writer must enforce one operation-wide private-storage budget that accounts for the actual overlap windows:

1. sorter-owned live runs **plus the growing final descriptor stage** during final output;
2. complete descriptor stage plus growing locator stage during object streaming;
3. locator stage plus growing leaf-reference stage during leaf emission;
4. current page-reference stage plus growing next-level stage during each internal level.

Bounding each subsystem independently is insufficient because their maxima can overlap.

A conservative next implementation can preflight the maximum of those phase sums from object count, tree geometry, fixed record widths, and the configured sorter live-spill cap.

## Publication boundary

The research candidate writes canonical output directly to the supplied sink after metadata preflight.

Therefore failures after output begins can still leave a partial artifact. This experiment does not replace the qualified private-staging/publication work already tracked under issue #11.

A production integration should feed the bounded canonical writer into the existing staged-publication contract so that partial construction remains private and publication outcomes remain explicit.

## Storage-security and restart boundaries

The descriptor, locator, and page-reference stages are:

- plaintext;
- non-durable;
- non-journaled;
- not authenticated restart authority.

This experiment does **not** close issue #11 requirements for:

- encrypted authenticated spill/staging;
- operation/key/nonce provenance and nonce uniqueness;
- authenticated durable restart journals;
- bounded stale-state cleanup and quarantine;
- descriptor-relative hardened storage operations;
- effective-user ownership policy;
- physical power-loss durability qualification;
- network-filesystem policy;
- supported-platform qualification.

The private fixed-record formats remain implementation details with no UCOF compatibility meaning.

## Verification

Implementation head `401c6acdc34b0b59443b8d911e5fc73f30c56b67` is green on the decisive implementation gates:

- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including the 70,671-object exact-byte/full-fanout regression, the low-allocation regression, and the three pre-output failure/cleanup regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The repository's longer HTTP, policy, parser, vector, and framing replay continues after those gates and provides broader regression confidence rather than changing the bounded-memory result above.

## Next executable slices

1. Add one preflighted operation-wide private-storage quota covering sorter/run overlap and every retained stage transition.
2. Add a payload-phase freshness failure after output has begun and require private metadata/tree stages to clean up while returning no report; document that direct sinks can contain a partial artifact.
3. Feed the bounded writer through the existing private staged-publication orchestration so construction failures never expose a destination artifact.
4. Consolidate the duplicated test-only descriptor/locator/page-reference stage helpers into one reusable private module with typed errors and explicit lifecycle states.
5. Replace plaintext private records with authenticated encrypted staging under the issue #11 policy, preserving deterministic final canonical bytes.
6. Add restart/discard authority, stale-operation cleanup bounds, and filesystem/platform qualification before proposing a production API.

## Governance boundary

This is implementation evidence only. It does not select EXP-0003 D1–D7, allocate EXP-0003, change FCP status, alter the immutable-successor wire proposal, or make any compatibility promise.