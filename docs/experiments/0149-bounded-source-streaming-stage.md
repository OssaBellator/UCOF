# Experiment 0149 — bounded source metadata staging for canonical streaming

**Status:** non-normative Phase 3 implementation evidence  
**Date:** 2026-08-13  
**Tracking:** issue #11  
**Depends on:** Experiment 0148 / PR #131 bounded deterministic spill-sort foundation

## Purpose

Issue #11 requires a production writer whose memory use does not scale with the complete logical workload. The existing source-backed genesis writer already streams payload bytes from strong-version random-access sources, but its preflight retains three workload-wide vectors: source order, strong versions, and logical lengths.

This experiment tests whether those three vectors can be replaced by a private fixed-size metadata stage without changing canonical UCOF bytes or the existing source-version safety checks.

The candidate is deliberately test-only. It reuses the current `StreamingSink`, source-object writer, canonical tree builder, and publication helpers after a new descriptor preflight has completed. No public API or wire-format change is made.

## Private source descriptor

Each source is represented by a fixed 64-byte private descriptor containing:

- object ID (`u64`);
- original source index (`u64`);
- object kind (`u16`);
- six reserved zero bytes;
- logical length (`u64`);
- strong version (`[u8; 32]`).

The object ID is also the external spill-sort key. Reserved bytes are required to remain zero when the prepared stage is read.

The descriptor framing is private writer state and has no UCOF compatibility meaning.

## Two-phase source borrowing

A source-descriptor iterator temporarily borrows the mutable source slice while metadata and strong versions are acquired. The bounded sorter consumes that iterator completely and writes the final sorted descriptor stream into a private stage.

Only after descriptor preparation returns does the writer reborrow the source slice by staged source index. This avoids unsafe code, interior mutability, or overlapping mutable borrows.

Before each payload is written, object ID, kind, and logical length are checked against the staged descriptor. The existing source-object helper then performs its normal strong-version check before and after bounded payload reads.

## Preflight-before-output property

Canonical output does not begin until descriptor sorting, cross-run duplicate detection, metadata acquisition, descriptor-stage validation, canonical tree-shape calculation, and exact output-size preflight have completed.

The tests require both of these failures to leave the output sink untouched and the private directory empty:

- duplicate object IDs split across spill runs;
- a strong-version metadata failure after a complete initial run has already been emitted.

This preserves the existing source writer's fail-before-output preflight intent while allowing metadata sorting to spill.

## Canonical byte equivalence

The primary regression uses 401 source-backed objects supplied in reverse object-ID order, each with a 257-byte payload. The existing `write_genesis_sources_to` implementation is the baseline.

The bounded candidate is executed with two materially different descriptor-sort geometries:

| configuration | initial-run records | merge fan-in |
|---|---:|---:|
| A | 17 | 3 |
| B | 53 | 7 |

Both candidates must produce bytes exactly equal to the baseline and must return the same normal source-streaming report as the baseline. The two candidates must also use different initial-run counts.

For 401 descriptors, the final private descriptor stage is exactly `25,664` bytes (`401 × 64`) in both configurations. The stage is removed before the candidate returns successfully.

## Resource effect

The candidate removes the writer-owned workload-wide order, strong-version, and logical-length vectors from the source preflight. Metadata ordering is instead bounded by the Experiment 0148 run/fan-in/disk/I/O limits.

This is not yet a constant-memory writer. The current canonical tree path still retains a `Vec<Locator>` proportional to object count, and that vector is the next explicit memory target.

The final 64-byte-per-object descriptor stage is also additional private storage outside the bounded sorter's internal `max_live_spill_bytes` accounting. Its byte count is reported separately by the candidate. A production resource model must bound the combined internal spill plus retained final-stage footprint explicitly.

## Storage and durability boundaries

The candidate stage is plaintext, non-durable, and non-journaled. It uses private exclusive creation and is retained only for the two-phase writer operation. It is not restart authority.

The experiment therefore does **not** close the issue #11 requirements for:

- authenticated encrypted-at-rest spill/staging;
- nonce/key provenance and restart-safe nonce management;
- authenticated durable restart journals;
- bounded stale-state cleanup;
- descriptor-relative hardened storage paths;
- physical power-loss or network-filesystem durability qualification.

The exploratory candidate uses explicit cleanup and handle-pinned reads, but production cleanup/error-path ownership still requires consolidation into the reusable storage layer rather than duplicated test-only staging code.

## Verification

The candidate head `ca1d2a0f4428353991e5d18fd436fbe54b38a974` is green on:

- workspace formatting and Clippy with warnings denied;
- the full Rust implementation test suite, including the byte-equivalence and preflight-failure regressions above;
- Rust 1.85.0 MSRV;
- i686 and powerpc64 portability checks;
- the complete long framing replay;
- Phase 3 Evidence;
- immutable-successor vector verification.

## Next executable slices

1. Consolidate the descriptor stage into one reusable private implementation rather than the exploratory test-only duplication.
2. Replace the remaining workload-wide locator vector with bounded staged/paged locator construction while preserving canonical page bytes.
3. Add authenticated encrypted-at-rest private framing once the dependency lockfile can be resolved reproducibly under `--locked`.
4. Add restart/discard semantics and ownership-bound authenticated journal state only after the encrypted staging format is fixed.

## Governance boundary

This is implementation evidence only. It does not select EXP-0003 D1–D7, change FCP status, allocate a new wire epoch, or make the private descriptor/spill framing part of UCOF compatibility.
