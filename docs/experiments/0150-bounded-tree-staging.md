# Experiment 0150 — bounded canonical tree staging

**Status:** non-normative Phase 3 implementation evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiment 0149 bounded source metadata staging and the canonical immutable-successor tree writer

## Purpose

Experiment 0149 removed three workload-wide source-preflight vectors, but the canonical tree path still retained one `Vec<Locator>` proportional to object count and then level-wide `Vec<PageRef>` frontiers while emitting immutable pages.

This experiment tests whether canonical tree construction can replace both workload-scaled tree vectors with private fixed-size stages while preserving the current writer's exact page partitioning, page order, file bytes, publication report, and occupancy behavior.

The candidate is deliberately test-only. It reuses the current `StreamingSink`, `encode_leaf`, `encode_internal`, and publication helpers. No public API or wire-format change is made.

## Canonical geometry

The current immutable-successor research format uses 16 KiB pages with:

- leaf capacity: 185 locator entries;
- minimum non-root leaf occupancy: 93;
- internal fan-out: 255 page references;
- minimum non-root internal occupancy: 128.

An allocation-free canonical group-size iterator reproduces the existing final-two-group redistribution rule. Infeasible generic geometries are rejected rather than silently producing an underfull group.

## Private locator stage

Each emitted object locator is written to a fixed 72-byte private record containing:

- object ID (`u64`);
- object kind (`u16`);
- six reserved zero bytes;
- record offset (`u64`);
- record length (`u64`);
- logical length (`u64`);
- object digest (`[u8; 32]`).

Leaf emission reads only one canonical group at a time. At most 185 `Locator` values are resident while one leaf page is encoded and written.

The locator framing is private writer state and has no UCOF compatibility meaning.

## Level-by-level page-reference staging

Each emitted page reference is written to a fixed 64-byte private record containing:

- minimum object ID (`u64`);
- maximum object ID (`u64`);
- page offset (`u64`);
- page level (`u8`);
- seven reserved zero bytes;
- page digest (`[u8; 32]`).

After all leaves have been emitted, internal construction alternates between two private stage files:

1. read one current-level stage in canonical order;
2. consume at most 255 `PageRef` values for one canonical internal group;
3. emit the normal canonical internal page through the existing writer;
4. append the resulting parent `PageRef` to the next-level stage;
5. retire the consumed stage and repeat until one root reference remains.

This preserves the existing level-by-level page order without retaining an entire level in memory.

## File-handle and cleanup discipline

Private stages use exclusive creation. On Unix the experimental stage files are mode `0600`.

Stage readers and writers clone the already-open file handle instead of reopening the pathname for normal reads. A stage closes its retained handle before unlinking its private name. Successful test completion requires the private stage directory to be empty after all stages are dropped.

These mechanics are research evidence only; they are not the production spill-security contract required by issue #11.

## Three-level canonical equivalence case

The primary regression uses 74,003 one-byte source-backed objects. This is intentionally above the two-level threshold and produces:

- 401 leaf pages;
- 2 level-1 internal pages;
- 1 level-2 root page;
- 404 current pages total;
- root level 2.

The existing `write_genesis_sources_to` implementation is the baseline.

The staged candidate writes the same canonical file header and source objects, stores returned locators in the 72-byte stage, builds the tree from bounded locator and page-reference batches, and then invokes the existing publication helper.

The regression requires:

- exact byte-for-byte equality with the current writer;
- exact publication-report equality;
- peak in-memory locator batch exactly 185 entries;
- peak in-memory page-reference batch exactly 255 entries;
- no retained private stage names after successful completion.

The candidate satisfies all of those assertions.

## Resource effect

For tree construction, workload-scaled locator and page-frontier vectors are no longer required.

The in-memory tree batch bound is determined by format geometry rather than object count:

- at most 185 `Locator` values while building a leaf;
- at most 255 `PageRef` values while building an internal page;
- one fixed page buffer inside the existing page encoder/writer path.

The tradeoff is private temporary storage proportional to tree metadata: 72 bytes per locator plus 64 bytes per staged page reference at each active level.

This experiment does **not** establish one unified disk budget covering descriptor-sort runs, the final descriptor stage, the locator stage, and the current/next page-reference stages. Production qualification must account for their combined live footprint rather than bounding each subsystem independently.

## End-to-end scope boundary

This experiment proves bounded-memory **tree construction**, not yet an end-to-end bounded-memory source-genesis writer.

The test intentionally still enters through the existing `preflight_source_streaming` path. That preflight retains workload-wide source order, strong-version, and logical-length vectors. Experiment 0149 separately demonstrated that those vectors can be replaced by a sorted fixed 64-byte descriptor stage, but the two experiments have not yet been consolidated into one reusable writer path.

Therefore this experiment must not be used to claim that the current public `write_genesis_sources_to` implementation is constant-memory.

## Storage and durability boundaries

The locator and page-reference stages are plaintext, non-durable, and non-journaled. They are not restart authority.

The experiment does **not** close issue #11 requirements for:

- authenticated encrypted-at-rest spill/staging;
- nonce/key provenance and restart-safe nonce management;
- authenticated durable restart journals;
- bounded stale-state cleanup;
- descriptor-relative hardened storage paths;
- a single combined live-stage/spill quota;
- physical power-loss or network-filesystem durability qualification;
- production publication semantics for a partially written output artifact.

## Verification

The implementation head `8a9ee411a4cf9c0bc7a69a74ac06e3f0ddb46884` is green on:

- workspace formatting and Clippy with warnings denied;
- the full Rust implementation test step, including the 74,003-object three-level byte/report equivalence regression;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks;
- concrete Reqwest conditional HTTP tests;
- async targeted lookup, full validation, linked history, and recovery tests;
- the versioned S3 source-adapter tests.

The repository's longer evidence replay continues after those decisive implementation gates and is not required to establish the bounded-tree equivalence claim above.

## Next executable slices

1. Consolidate Experiment 0149's sorted 64-byte source descriptor stage directly into this staged tree path so no workload-wide metadata vectors remain.
2. Replace workload-scaled allocation checks in the bounded candidate with explicit fixed-batch checks for the descriptor-sort run buffer, source read buffer, 185-entry leaf batch, and 255-entry internal batch.
3. Define and test one combined live private-storage quota covering sorter runs, retained descriptor output, locator staging, and both page-reference level stages.
4. Add adversarial cleanup tests for source/version failure after locator staging has begun, page-stage I/O failure, output-limit preflight, and publication failure.
5. Consolidate duplicate test-only staging implementations into one reusable private module only after end-to-end equivalence and cleanup behavior are green.
6. Add authenticated encrypted-at-rest private framing and restart/discard semantics before treating the staged writer as production-candidate evidence.

## Governance boundary

This is implementation evidence only. It does not select EXP-0003 D1–D7, change FCP status, allocate a new wire epoch, or make the private locator/page-reference staging formats part of UCOF compatibility.