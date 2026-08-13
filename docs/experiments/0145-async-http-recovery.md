# Experiment 0145 — Async recovery-candidate validation over real HTTP

**Status:** non-normative Phase 3 implementation/qualification evidence  
**Date:** 2026-08-13  
**Tracking:** issue #10  
**Depends on:** Experiments 0143–0144

## Purpose

Experiments 0143 and 0144 establish native async strict active-state and linked-history validation over one concrete strong-version HTTP source. This experiment completes the format-assurance operation set by adding separately requested, bounded, report-only recovery-candidate validation.

Recovery remains deliberately distinct from strict validity and linked history. It scans a caller-bounded suffix for footer-magic candidates, strictly validates candidate prefixes, and reports evidence without selecting or activating any recovered state.

## Source-view boundary

`scan_source_recovery_async` acquires the real remote object metadata once:

```text
full remote object length
one strong source version
```

The bounded suffix scan and every later candidate-prefix read remain conditioned on that same source view.

Historical candidate prefixes are parser views only. Provider responses must continue to report the real full object length; a candidate prefix never becomes a fictional shorter remote object.

## Recovery accounting boundary

False footer-magic hits are attacker-controlled work. A format-invalid candidate must therefore consume the same operation-wide source read/byte budget as a valid candidate attempt.

`AsyncTrackedPrefixStrongVersionSource` records accepted underlying read operations and bytes independently of the strict validator's success result. `validate_async_recovery_prefix` then mirrors the synchronous recovery contract:

- on success, charge attempted read operations/bytes plus the strict validator's hash/allocation accounting;
- on format failure, retain the attempted read/byte charge even though the candidate report is discarded;
- propagate transport/version/cancellation/deadline/resource failures rather than reclassifying them as harmless invalid candidates.

This prevents repeated malformed footer candidates from resetting the source budget and becoming a read-amplification path.

## Recovery algorithm

1. acquire one strong-version full-object source view;
2. read only the configured bounded suffix;
3. enumerate footer-magic hits newest first;
4. stop at `max_recovery_attempts` and record truncation;
5. for each candidate, derive the exact prefix ending at that footer;
6. strictly validate the candidate prefix through the fixed source view;
7. ignore only format-invalid candidates while preserving their attempted-I/O charge;
8. propagate source/transport/resource failures;
9. stop at `max_recovery_candidates` and record candidate truncation;
10. return candidates as evidence only.

No candidate is automatically selected. Strict active-state validation never invokes this operation.

## Differential real-HTTP tests

### Interrupted publication tail

A valid linked state is followed by trailing bytes without a complete footer. The test compares:

- synchronous `ImmutableSliceSource + scan_source_recovery`;
- native async authenticated/retrying Reqwest + `scan_source_recovery_async`.

Candidate reports, truncation facts, and cumulative source stats must match exactly.

The loopback server additionally requires:

- one HEAD for the operation;
- `If-Match` on every GET;
- `Accept-Encoding: identity` on every GET;
- full-object provider `Content-Range` totals even while validating candidate prefixes;
- accepted GET count equal cumulative async read operations.

### Version change after suffix scan

The suffix read is accepted under the initial version, then candidate validation receives HTTP 412. Recovery must terminate with:

```text
Conditional(VersionChanged)
```

It must not return a candidate assembled from mixed source versions.

### Candidate truncation

A recovery scan constrained to one candidate must reproduce the synchronous candidate list and `candidates_truncated` behavior.

## Assurance boundary

This experiment completes targeted lookup, strict full validation, linked history, and report-only recovery over the concrete HTTP transport.

It does not establish:

- freshness or rollback authorization;
- one maintained versioned cloud-object provider adapter;
- provider-specific version/signing semantics;
- provider/TLS/credential/redirect/proxy/cache/decompression qualification;
- provider-scale request/byte/latency measurements.

Those remain the #10 gate after this slice.

## Reproduction

```console
cargo test --locked -p ucof-experiments --features http-reqwest conditional_async_source_recovery
```

The HTTP feature must remain compatible with Rust 1.85.0; default i686 and powerpc64 portability checks remain required.

## Governance boundary

This uses current immutable-successor research bytes only. It does not select D1–D7, change FCP-0003 status, allocate EXP-0003, or make HTTP/provider behavior part of the normative wire format.
