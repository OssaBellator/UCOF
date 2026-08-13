# Experiment 0142 — Async strong-version targeted lookup over real HTTP

**Status:** non-normative Phase 3 implementation/qualification evidence  
**Date:** 2026-08-13  
**Tracking:** issue #10  
**Depends on:** Experiments 0139–0141 / PRs #122–#124

## Purpose

The concrete HTTP stack now has native cancellation, operation-wide retry/backoff, and application-owned one-refresh authentication. The next requirement is to prove that a real assurance operation can use that stack without hiding an async runtime behind the synchronous `ImmutableReadAt` interface.

This experiment ports **targeted authenticated lookup only**.

It intentionally does not port full validation, linked history, or recovery in the same change.

## Native async source contract

`AsyncStrongVersionReadAt` exposes two asynchronous operations:

1. acquire exact object length + one strong source version;
2. read one exact range conditioned on that same version and total object length.

The current Reqwest retrying client and authenticated Reqwest client implement this contract.

The trait is separate from `ImmutableReadAt`. No `block_on`, blocking Reqwest client, runtime handle, or synchronous wrapper is introduced.

## One-version operation

`lookup_at_async` acquires metadata exactly once at the beginning of the lookup.

The resulting strong version token and exact length are retained by an async source reader. Every later range request is issued with:

```text
expected strong version = metadata version
expected total length    = metadata length
```

Even after a concrete transport reports success, the generic async source reader rechecks:

- returned version parses as a strong token and equals the operation token;
- returned range offset equals the requested offset;
- returned total length equals the operation length;
- returned body length equals the requested length.

A contradictory async-source implementation therefore fails closed before bytes are copied into parser buffers.

## Limit/stat parity

The async source reader mirrors synchronous `SourceReader` policy:

- `max_read_operations`;
- `max_read_request_bytes`;
- `max_total_bytes_read`;
- `hash_block_bytes`;
- format file/allocation/depth/page limits;
- `ImmutableSourceStats` read operations, bytes read, bytes hashed, and largest allocation.

Large hash scopes remain streaming: the reader allocates at most the configured hash block and does not cache the whole commit/file merely to reuse the synchronous parser.

This is important because a replay/caching bridge would convert the current commit digest into an accidental whole-commit memory requirement.

## Parser reuse

The async lookup implementation is nested inside `source_api`, allowing it to reuse the existing private:

- `LookupEnvelope`;
- `LookupReference`;
- `PageLookup`;
- `Locator`;
- footer/page/object parsing helpers;
- page-range overlap registration;
- digest domain/constants.

Only I/O-bearing routines have async counterparts:

- source reader;
- lookup envelope reads/hashing;
- root-to-leaf page reads;
- selected-object reads/hashing.

This minimizes semantic drift between sync and async lookup.

## Assurance scope

`lookup_at_async` matches the current targeted lookup claim:

- exact file header;
- exact-end footer;
- snapshot digest/linkage;
- current commit digest;
- one authenticated root-to-leaf page path;
- authenticated found object or authenticated absence;
- selected object record digest when present.

It does not claim that unrelated pages/objects or complete linked history were validated.

## Error boundary

`AsyncImmutableSourceError` keeps two categories distinct:

```text
Source(ImmutableSourceError)
Conditional(ConditionalSourceError)
```

Format/resource failures therefore remain distinguishable from version change, cancellation, deadline, HTTP protocol, authentication, and retry failures.

## Differential real-HTTP tests

The feature test suite builds real immutable-successor genesis bytes and serves them through a loopback Tokio HTTP server implementing HEAD + Range/If-Match.

### Found object

The same file/object is looked up using:

- synchronous `ImmutableSliceSource + lookup_at`;
- authenticated/retrying Reqwest source + `lookup_at_async`.

The complete `ImmutableSourceLookupReport`, including source stats, must be identical.

The server observes exactly one HEAD and one GET per accepted async source read operation.

### Authenticated absence

A sparse missing ObjectId produces the same synchronous and async absence report.

### Version change

Metadata returns strong ETag `"v1"`, then the first range request returns 412.

The lookup terminates with `Conditional(VersionChanged)` after one range attempt and does not accept range bytes.

### Read-operation budget

With `max_read_operations = 1`, the async lookup performs exactly one range read and then returns the same source read-operation limit class before a second range request is issued.

## Still open under #10

After this experiment, remaining assurance work includes:

- async full exact-end validation;
- async linked-history verification;
- async recovery candidate validation;
- provider-specific versioned cloud-object adapter and signing/version semantics;
- broader provider/TLS/cache/credential qualification.

Targeted lookup is deliberately first because it is bounded and exposes the entire source-version/range/path/object assurance chain without requiring all-pages traversal.

## Reproduction

```console
cargo test --locked -p ucof-experiments --features http-reqwest conditional_async_source_lookup
```

The existing HTTP feature MSRV gate must also compile this path on Rust 1.85.0.

## Boundary

This experiment uses the current research immutable-successor microformat. It does not select or change EXP-0003 bytes, D1–D7, FCP status, or epoch allocation.
