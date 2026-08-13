# Experiment 0141 — Async conditional HTTP authentication refresh

**Status:** non-normative Phase 3 implementation/qualification evidence  
**Date:** 2026-08-13  
**Tracking:** issue #10  
**Depends on:** Experiments 0139–0140 / PRs #122–#123

## Purpose

The concrete HTTP transport now has native asynchronous cancellation and one operation-wide retry/backoff authority. The remaining authentication gap is to permit an **application-owned credential refresh** without turning HTTP 401 into an unbounded retry mechanism or letting the transport invent provider credential policy.

This experiment adds exactly that boundary.

## Credential ownership

UCOF does not obtain, persist, rotate, sign, or infer credentials.

An application implements:

```text
AsyncConditionalAuthenticationRefresher
```

and supplies:

1. the current optional HTTP `Authorization` value immediately before each transport attempt;
2. one asynchronous refresh operation when explicitly requested by the UCOF classification policy.

The authorization scheme is opaque to UCOF. The research implementation can therefore carry Bearer, Basic, or another syntactically valid Authorization value without assigning provider semantics.

Cloud-provider multi-header signing remains a later provider-specific adapter concern.

## Credential redaction

`ReqwestAuthorizationHeader` wraps an HTTP `HeaderValue` and:

- marks the value `sensitive` before use;
- implements `Debug` as a fixed `<redacted>` representation;
- never exposes a getter returning the credential as text.

This reduces accidental logging risk. It is not a secret-memory or compromised-process confidentiality claim.

## One-refresh rule

For one logical metadata or range request:

1. send attempts with `OneRefreshPermitted` authentication classification;
2. ordinary retryable transport/HTTP failures continue to use the shared retry/backoff budget;
3. only an HTTP 401 classified as `RefreshAuthentication` can invoke the application refresher;
4. invoke the refresher **once**;
5. replay the same logical request under `Terminal` authentication classification;
6. a second 401 is terminal `http unauthorized` and cannot trigger another refresh.

The rule is per logical request, matching the existing synchronous authentication experiment. A later independent range request may again permit one refresh if the application chooses to use the authenticated wrapper.

## Attempt accounting

Authentication refresh itself is not an HTTP transport attempt.

The replay is an HTTP transport attempt and therefore consumes the same operation-wide attempt budget owned by `AsyncRetryingReqwestConditionalClient`.

Example:

```text
max transport attempts = 1
first request           = 401
refresh                 = succeeds
replay                   = refused before network I/O with Limit("transport attempts")
```

No hidden replay escapes the operation budget.

Retry/backoff state is also preserved across the refresh boundary. A sequence such as:

```text
503 -> backoff -> 401 -> refresh -> 503 -> backoff -> 200
```

continues to consume one shared attempt counter and cumulative delay budget.

## Native cancellation/deadline during refresh

The async refresh future is raced against the same `ImmutableOperationControl` watcher used for in-flight HTTP and async retry waits.

If cancellation/deadline wins:

- the refresh future is dropped;
- no replay occurs;
- the operation returns the corresponding control error.

The control is checked again after a refresh future reports success and before the replay begins.

## Authorization application

The current authorization value is read immediately before each HTTP attempt and applied only as the HTTP `Authorization` header.

Protocol-critical headers remain controlled by UCOF:

```text
Accept-Encoding: identity
Range
authoritative If-Match token
```

An application credential therefore cannot replace or weaken the strong-version range fields through this interface.

## Real HTTP tests

The optional Reqwest feature suite adds loopback tests for:

### One stale credential → one refresh → success

- first HEAD carries `Authorization: Bearer stale` and receives 401;
- refresher updates application state;
- replay carries `Authorization: Bearer fresh` and receives exact metadata;
- two HTTP attempts and one refresh are recorded.

### Second 401 is terminal

Two 401 responses cause exactly:

```text
2 HTTP attempts
1 refresh
terminal http unauthorized
```

No second refresh is possible.

### Refresh failure prevents replay

A refresher error is returned directly after the first 401. Only one HTTP request reaches the server.

### Replay respects attempt budget

With one permitted transport attempt, the first 401 consumes it. Refresh may succeed, but the replay is rejected before network I/O with `Limit("transport attempts")`.

### Cancellation during stalled refresh

The refresher signals that it has started and then stalls. The test cancels the operation only after that signal. The refresh future is dropped and no replay request reaches the server.

### Refreshed credential persists for later range

Metadata first refreshes stale→fresh. A later `GET + Range + If-Match` carries the refreshed Authorization value while preserving the exact strong-version headers.

## Relationship to generic authentication policy

This experiment does not replace `execute_conditional_http_with_refresh`. It implements the same boundary for the concrete asynchronous Reqwest/retry stack:

- refresh only after explicit classifier decision;
- one refresh;
- one replay;
- second 401 terminal;
- control checks around refresh;
- no provider credential policy in the format layer.

## Still open under #10

After this experiment, the main remaining #10 work is:

1. async strong-version random-access/assurance integration so targeted lookup, full validation, linked history, and recovery execute through the concrete HTTP stack;
2. one maintained immutable/versioned cloud-object adapter with provider-specific version-token and signing semantics;
3. provider/TLS/credential/cache/decompression qualification and emulator/provider evidence.

Authentication for a cloud provider may require multiple signed headers rather than this generic Authorization-only boundary and should be implemented in the provider adapter rather than overloading the generic HTTP credential interface.

## Reproduction

```console
cargo test --locked -p ucof-experiments --features http-reqwest conditional_reqwest
```

The existing dedicated feature/MSRV gate exercises the new tests without adding Reqwest/Tokio to ordinary feature-off portability builds.

## Boundary

This experiment does not change EXP-0003 bytes, D1–D7 governance decisions, FCP status, or epoch allocation. It is transport/authentication implementation evidence only.
