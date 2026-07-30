# Experiment 0047 — Bounded Source History and Recovery

**Status:** Executable non-normative evidence  
**Date:** 2026-07-30  
**Epoch allocation:** None

## Question

Can the immutable-page successor verify an active state, every linked historical prefix, and recovery candidates directly through bounded random-access reads without materializing the whole file or weakening the distinction between those assurance modes?

## APIs

The reusable Rust experiment now exposes:

- `validate_source_at` — exact-end full active-state validation;
- `validate_source_history` — newest-to-oldest linked-prefix validation;
- `scan_source_recovery` — bounded suffix discovery and report-only candidate validation.

All three consume `ImmutableReadAt` and `ImmutableSourceLimits`.

## Full source validation

`validate_source_at`:

- authenticates the exact-end commit and snapshot;
- walks every reachable active page;
- canonicalizes the collected locator order before global duplicate checks;
- validates every active object record and payload digest;
- rejects page/page, object/object, and object/structural overlap;
- cross-checks the footer's current-commit page count;
- streams commit and object hashes in bounded blocks;
- reports read operations, bytes read, bytes hashed, and largest allocation.

A 400-object, four-page vector validates under 4 KiB maximum range requests.

## Linked source history

`validate_source_history`:

- validates the current exact prefix;
- follows only the authenticated previous-footer locator;
- validates each ancestor as its own exact prefix;
- requires exact sequence decrements;
- requires parent snapshot identity agreement;
- requires strictly decreasing physical footer offsets;
- enforces a caller history-entry limit;
- carries one cumulative source-read budget across all ancestors.

The operation intentionally provides stronger evidence than active-state validation. A modified object that was replaced in sequence 1 can leave the sequence-1 active state valid, while history validation rejects sequence 0 at the object digest.

## Source recovery

`scan_source_recovery`:

- reads only a caller-bounded suffix;
- supports suffixes shorter than footer magic without indexing failure;
- treats magic matches as hints with no authority;
- caps attempted footers and returned candidates independently;
- validates every candidate through a prefix-limited source view;
- charges successful and failed candidate reads to one cumulative budget;
- orders valid candidates by physical recency;
- never selects an active replacement.

An interrupted append reports the complete sequence-1 and sequence-0 prefixes. A candidate-result cap reports truncation explicitly. A suffix containing many malformed footer decoys terminates at the global byte budget rather than granting each failed candidate a fresh allowance.

## Allocation and ordering findings

The implementation checks allocation policy before reserving object-range storage. Internal-page traversal order is not treated as canonical object order; locators are sorted by object identifier before whole-directory uniqueness checks.

## Tests

Focused tests cover:

- 400-object multi-level full source validation;
- maximum request-size enforcement;
- linked history `[1, 0]`;
- active validity versus historical corruption;
- interrupted publication recovery;
- explicit recovery candidate caps;
- short suffix safety;
- failed-candidate cumulative work accounting;
- low read-budget failure before policy excess.

The implementation passes rustfmt, Clippy with warnings denied, Rust 1.85, i686 and powerpc64 compilation, and the direct successor vector workflow.

## Assurance boundaries

- Full active-state validation does not claim ancestor integrity.
- Linked history does not establish freshness.
- Recovery does not choose a candidate.
- A stable source view is still required for one assurance operation.
- Source integrity is not authenticity, provenance, or confidentiality.

## Remaining work

- concrete conditional HTTP and cloud adapters using strong version tokens;
- asynchronous cancellation and deadline behavior around real I/O;
- support-profile boundary vectors for source operations;
- independent implementation or external review.
