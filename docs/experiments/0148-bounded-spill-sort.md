# Experiment 0148 — bounded deterministic spill sort foundation

**Status:** non-normative Phase 3 implementation evidence  
**Date:** 2026-08-13  
**Tracking:** issue #11  
**Depends on:** bounded writer research; no dependency on the old stacked spill/restart branches

## Purpose

Issue #11 requires a production writer that does not require the complete logical workload in memory. This experiment ports the useful external-sort invariants from the earlier EXP-0002 Python research into a clean Rust primitive on current `main`.

The primitive is intentionally narrower than the final writer. It sorts opaque fixed-size payload records by a separate `u64` key, using bounded initial runs and bounded-fan-in staged merges, then streams only the payload bytes in strict key order. It does not define UCOF wire bytes, publication policy, encryption, or restart semantics.

## Bounded resource model

`BoundedSpillSortLimits` makes the following limits explicit:

- exact payload bytes per logical record;
- records buffered per initial run;
- total logical records;
- initial run count;
- merge fan-in / simultaneously opened input runs;
- merge pass count;
- live spill bytes;
- cumulative spill bytes written;
- cumulative intermediate-merge bytes read;
- cumulative intermediate-merge bytes written.

Intermediate output bytes are reserved against the live-disk limit before the new merge file is created. This prevents the merge phase from silently requiring roughly twice the live input footprint without accounting for that amplification.

Spill files use exclusive creation. On Unix the experiment requests mode `0600`. The workspace tracks every file it creates and removes tracked state on success or best-effort on failure.

The caller still supplies the spill directory. This experiment does not claim descriptor-relative path hardening, same-UID isolation, encrypted-at-rest storage, or secure deletion.

## Ordering and duplicate semantics

Every initial run is sorted by the external key and rejects adjacent duplicates before being emitted.

Every k-way merge keeps one current record per input run in a min-heap. A key equal to the previously emitted key is rejected, so duplicates split across independent initial runs cannot survive the merge boundary.

The final run is checked for strict ordering again while payload bytes are streamed to the caller.

These checks make sorted output independent of initial-run size and merge fan-in for a given unique logical record set.

## Deterministic workload

The primary regression uses `20,003` unique records with an `88`-byte payload. Input order is an affine permutation of the key range; output is compared with the direct key-sorted byte sequence.

Two materially different external-sort configurations are required to converge:

| configuration | initial run records | merge fan-in | initial runs | merge passes |
|---|---:|---:|---:|---:|
| A | 127 | 4 | 158 | 4 |
| B | 509 | 16 | 40 | 2 |

Both configurations produce exactly `1,760,264` payload bytes with SHA-256:

```text
b9502518e37076aa6abd57756695f34ba2b8003f0b218a3fda7527c221d6322b
```

With the private eight-byte key frame, the initial spill representation is `1,920,288` bytes in either configuration. Different run geometry changes spill I/O and pass counts but not the emitted payload bytes.

## Adversarial checks

The Rust tests also require:

- a duplicate split across initial runs to fail during staged merge;
- merge-time live-disk amplification to be rejected before an intermediate run can exceed the configured live-spill budget;
- all tracked spill files to be gone after those failures;
- empty input to write no bytes and return the SHA-256 digest of the empty byte string.

The public API explicitly documents that the caller's output writer can receive a valid prefix before a later I/O error. Atomic visibility therefore remains the responsibility of the private publication staging layer rather than the sorting primitive.

## Reproduction

```console
cargo test --locked -p ucof-experiments bounded_spill_sort_tests
```

The complete repository Rust test, lint, Rust 1.85.0 MSRV, i686, powerpc64, Fuzz, Phase 3 Evidence, Phase 3 Integration, and immutable-successor-vector workflows are also required for the PR head.

## Remaining issue #11 work

This experiment establishes a clean bounded external-sort foundation only. It does not complete issue #11.

The next executable slices are:

1. authenticated encrypted-at-rest spill framing using externally supplied key material and an established AEAD implementation;
2. deterministic discard/restart semantics for unauthenticated, truncated, or key-mismatched spill state;
3. durable ownership-bound restart journal/checkpoint state if resumable spills are retained;
4. integration of the bounded sorter with canonical writer emission without changing deterministic UCOF bytes;
5. stale-state cleanup, cancellation/crash injection, filesystem durability qualification, and supported-platform hardening.

## Governance boundary

This is implementation evidence only. It does not select EXP-0003 D1–D7, change FCP status, allocate a wire epoch, or make the private spill framing part of UCOF compatibility.
