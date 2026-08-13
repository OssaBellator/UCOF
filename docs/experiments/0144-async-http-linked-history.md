# Experiment 0144 — Async linked-history validation over real HTTP

**Status:** non-normative Phase 3 implementation/qualification evidence  
**Date:** 2026-08-13  
**Tracking:** issue #10  
**Depends on:** Experiment 0143 / PR #126

## Purpose

Experiment 0143 established native async exact-end active-state validation over one concrete strong-version HTTP source. This experiment extends that assurance boundary to the complete linked snapshot history without reacquiring or weakening the remote source view for historical prefixes.

The operation remains distinct from recovery: every linked prefix must be strictly valid at its own exact end and must agree with its child's authenticated parent linkage.

## Source-view boundary

`validate_source_history_async` acquires the real source metadata exactly once:

```text
full remote object length
one strong source version
```

Those facts remain fixed for the complete history operation.

Historical commits are validated as parser prefixes, not as separate remote objects. `AsyncPrefixStrongVersionSource` therefore exposes a historical prefix length to the strict parser while every underlying conditional range request still uses the original full remote object length and the same strong version token.

This distinction is important for HTTP and object stores: a historical prefix must not require or pretend that `Content-Range` reports the prefix as the provider object's total length.

## Linked-history algorithm

For each state, newest first:

1. derive the current exact prefix length from the active footer chain;
2. strictly validate that exact prefix with `validate_source_at_async`;
3. preserve cumulative source read/hash/allocation limits and stats across the whole history operation;
4. reread/authenticate the prefix footer and snapshot parent digest under the remaining budget;
5. require the validated child sequence and parent snapshot digest to select the next prefix exactly;
6. reject a non-decreasing or malformed previous-footer offset;
7. terminate only at a valid sequence-zero genesis with an all-zero parent digest.

No backward scan or candidate selection occurs. Recovery remains a separate requested operation.

## Report type

`AsyncImmutableSourceHistoryReport` wraps the existing synchronous history result with only the fixed source-view facts:

```text
source_version
source_length
history: ImmutableSourceHistoryReport
```

The inner history entries and cumulative `ImmutableSourceStats` are therefore directly comparable with synchronous `validate_source_history`.

## Differential real-HTTP tests

### Three-commit linked history

The test creates a genesis state, a replacement commit, and an insertion commit. The complete bytes are validated through:

- synchronous `ImmutableSliceSource + validate_source_history`;
- native async authenticated/retrying Reqwest + `validate_source_history_async`.

The complete history report and cumulative source stats must match exactly.

The loopback HTTP server also asserts:

- exactly one HEAD for the complete operation;
- every GET carries `If-Match: "v1"`;
- every GET requests `Accept-Encoding: identity`;
- every provider `Content-Range` total remains the full object length, including reads used to validate historical parser prefixes;
- accepted GET count equals cumulative async source read operations.

### Version change

After a bounded number of accepted ranges, the server returns HTTP 412. The operation must terminate with:

```text
Conditional(VersionChanged)
```

No later historical state may be reported from a mixed source version.

### History resource limit

A three-state valid history with `max_history_entries = 1` must fail with the same `Limit("history entries")` class as the synchronous history policy.

## Assurance boundary

This experiment establishes linked-history verification over the concrete HTTP stack while preserving one full-object strong-version source view.

It does not establish:

- recovery-candidate validation over real HTTP;
- freshness or rollback authorization;
- provider-specific cloud-object version/signing semantics;
- provider/TLS/credential/cache qualification beyond the concrete Reqwest research adapter.

Those remain separate #10 gates.

## Reproduction

```console
cargo test --locked -p ucof-experiments --features http-reqwest conditional_async_source_history
```

The HTTP feature must continue compiling on Rust 1.85.0, and default portability checks remain required.

## Boundary

This uses the current immutable-successor research microformat. It does not select or change EXP-0003 bytes, D1–D7, FCP status, or epoch allocation.
