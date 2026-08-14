# Experiment 0153 — post-preflight freshness failure cleanup

**Status:** non-normative Phase 3 implementation evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0151–0152

## Purpose

Experiments 0151 and 0152 establish bounded source-genesis construction, pre-output metadata failure handling, and an operation-wide quota for private working stages. One important lifecycle case remained: a source can pass metadata preflight and the pre-read freshness check, then change after payload bytes have already been emitted.

This experiment verifies the exact terminal behavior of that case and distinguishes two independent cleanup obligations:

1. private descriptor/locator/page-reference working state must still be retired;
2. a direct caller-provided output sink may legitimately contain a partial canonical artifact because output visibility is outside the working-stage cleanup contract.

That distinction is the reason the next integration step must place bounded source genesis behind private staged publication.

## Freshness schedule

The regression uses one one-byte source whose strong version changes on the third version query.

The expected sequence is:

1. **metadata preflight** — version `[7; 32]` is staged in the sorted source descriptor;
2. **immediately before payload streaming** — version remains `[7; 32]` and the object may proceed;
3. the canonical object header and one payload byte are written;
4. **immediately after payload streaming** — version changes to `[8; 32]`;
5. the existing canonical source-object helper returns a terminal version-change error and no publication report is produced.

The test relies on the existing source-writer freshness contract rather than introducing a new version-check sequence.

## Partial direct-sink artifact

The direct output sink is required to contain exactly:

- the 64-byte immutable file header;
- the 48-byte object header;
- the one-byte payload.

Total expected partial output: **113 bytes**.

The regression also checks the file and object magic prefixes in those bytes.

No page, snapshot, footer, or publication report is produced.

This partial output is expected behavior for a direct sink. The public source-streaming writer already documents that failures after output begins are terminal and that callers must use private staging when partial bytes must remain invisible.

## Private working-state cleanup

The same operation uses the exact Experiment 0152 private working-stage quota.

After the terminal post-payload freshness failure, the test requires the private working directory to be empty. This proves that the retained sorted descriptor stage and the partially accumulated locator stage are retired by normal ownership/drop cleanup even though canonical output has already begun.

No page-reference stage is created for this one-object failure because tree construction is never reached.

## What this closes

The experiment establishes that bounded source-genesis working-stage cleanup is not limited to pre-output failures. A terminal source freshness failure after canonical bytes begin does not leak the private descriptor or locator state used by the bounded writer.

It also makes the direct-sink boundary concrete rather than merely documented: private working-state cleanup and output visibility are separate concerns.

## What it does not close

This experiment does not make a direct sink transactional. The 113-byte partial artifact is intentionally observable in the test.

It therefore does not close issue #11 requirements for:

- private construction of the final output artifact;
- no-overwrite publication and explicit durable/indeterminate outcomes;
- including the private output artifact in the operation-wide storage quota;
- encrypted authenticated spill/staging;
- restart authority and authenticated durable journals;
- hardened storage paths and stale-operation cleanup;
- filesystem/platform durability qualification.

## Verification

Implementation head `0144b935965c1597fb5d0c43de351d910197c9fa` is green on:

- workspace formatting;
- Clippy with warnings denied;
- the full Rust implementation test step, including the new third-version-check regression and all Experiments 0151–0152 regressions;
- concrete Reqwest transport tests;
- async targeted lookup, full validation, linked history, and recovery tests;
- the versioned S3 adapter tests;
- policy, parser, invalid-corpus, vector, and EXP-0003 scaffold/amendment verification;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The standard framing replay was still completing when this evidence note was written; it does not alter the source-freshness lifecycle claim above.

## Next executable slices

1. Split bounded source genesis into a prepared metadata/sort phase and a prepared emission phase so private output staging begins only after all pre-output failures have been resolved.
2. Route prepared emission through the existing `PersistentStagingBackend` lifecycle: begin private, construct, validate, sync, publish without overwrite, sync parent, retire private.
3. Extend the Experiment 0152 quota to include the private final output artifact across every post-preflight overlap window.
4. Prove that post-payload freshness failure aborts the private output artifact and leaves the destination untouched.
5. Preserve explicit indeterminate outcomes after a possible link or failed parent sync rather than converting them into generic errors.
6. Consolidate the test-only fixed-stage helpers and add encryption/restart semantics before proposing a production API.

## Governance boundary

This is implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change immutable-successor bytes, or create a compatibility promise.