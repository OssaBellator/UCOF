# Experiment 0140 — Async conditional HTTP retry/backoff execution

**Status:** non-normative Phase 3 implementation/qualification evidence  
**Date:** 2026-08-13  
**Tracking:** issue #10  
**Depends on:** Experiment 0139 / PR #122

## Purpose

Experiment 0139 deliberately made the concrete Reqwest transport **single-attempt** and disabled Reqwest's internal retries. That preserves one authority for request/retry accounting, but it initially flattened retryable HTTP classifications into `RetryableClient` and therefore discarded server `Retry-After` minima before an async retry layer could use them.

This experiment adds the missing operation layer without moving retry policy back into the transport.

## Attempt/result boundary

`ReqwestConditionalRangeClient` now also exposes one-attempt methods that preserve generic classifier outcomes:

```text
Accepted(value)
Retry { error, server_minimum_millis }
RefreshAuthentication
terminal error
```

The existing simple `metadata()` / `read_range_if_match()` methods remain available as single-attempt convenience calls.

For retryable HTTP statuses, integer `Retry-After` delta-seconds therefore remain visible to the operation layer instead of being discarded.

## Async operation-wide retry wrapper

`AsyncRetryingReqwestConditionalClient` owns:

- one `ConditionalRetryPolicy` attempt limit;
- one `ConditionalBackoffBudget` across its complete lifetime;
- the same `ImmutableOperationControl` used by the concrete transport;
- one cumulative transport-attempt counter covering metadata and every later range request.

Reqwest itself remains configured for zero internal retries.

### Attempt accounting

Metadata acquisition and all subsequent range calls consume from the same attempt budget.

If the final allowed attempt returns a retryable result, the wrapper returns:

```text
Limit("transport attempts")
```

without planning or sleeping for a retry that cannot occur.

This mirrors the existing synchronous retry semantics.

### Retry classification

The wrapper retries only:

- `ConditionalHttpDecision::Retry` from the generic HTTP classifier;
- transport-level `RetryableClient` failures emitted by Reqwest integration.

It does not retry:

- version change;
- malformed protocol metadata;
- cancellation;
- deadline expiry;
- resource-limit failures;
- terminal client failures;
- authentication refresh requirements.

Authentication refresh remains the next explicit slice rather than being inferred from a 401.

## Backoff accounting

HTTP-classified retries are passed to the existing `plan_conditional_http_retry` helper and `ConditionalBackoffBudget`.

Therefore:

- exponential delay is deterministic;
- server minimum delay participates when present;
- a server minimum above configured per-delay maximum fails rather than being silently truncated;
- cumulative delay is operation-wide;
- a delay that reaches/exceeds the remaining operation deadline fails before sleeping.

Transport-level retryable errors have no server minimum and use the same backoff budget.

## Native async wait cancellation

Accepted retry delays are executed with Tokio sleep raced against the same operation control used for request/body cancellation.

Cancellation or deadline expiry drops the pending sleep future and returns immediately under the control polling bound.

No blocking thread sleep is used by this async wrapper.

## Real HTTP tests

The optional Reqwest feature test set now also exercises:

1. **transient metadata retry then success**
   - first HEAD returns 503;
   - second HEAD returns exact strong-version metadata;
   - two transport attempts and one backoff plan are recorded.

2. **Retry-After propagation**
   - HEAD returns 429 + `Retry-After: 1`;
   - one-attempt API reports a 1000 ms server minimum;
   - the existing backoff planner chooses 1000 ms when its local exponential delay is smaller.

3. **operation-wide attempt budget**
   - metadata consumes one attempt;
   - a later range consumes the final attempt and receives 503;
   - no hidden third request is made and no unusable retry delay is charged.

4. **cancellation during async backoff**
   - retryable response schedules a 500 ms delay;
   - operation cancellation interrupts the wait and returns `Cancelled`.

5. **deadline before retry wait**
   - retryable response requires a 500 ms delay while the operation has less remaining time;
   - `DeadlineExceeded` is returned before sleeping and no retry is planned.

## Relationship to existing synchronous policy

This experiment does not replace:

- `RetryingConditionalClient`;
- `ConditionalBackoffBudget`;
- `ConditionalWaitPolicy` / synchronous cooperative wait;
- generic HTTP classification;
- authentication-refresh policy.

It reuses their policy semantics for a maintained asynchronous transport.

The async wrapper currently uses zero jitter. If jitter is later needed, it must remain caller/policy-controlled and reproducible rather than being hidden inside Reqwest.

## Still open under #10

After this experiment, #10 still requires at least:

- async application-owned authentication refresh integrated with the concrete transport/retry accounting;
- one async strong-version random-access adapter usable by the existing assurance algorithms or an explicit async assurance stack;
- targeted lookup, full validation, linked history, and recovery through the real HTTP path;
- one maintained immutable/versioned cloud-object adapter with provider-specific version semantics;
- provider/TLS/credential/cache/decompression qualification beyond current conservative HTTP defaults.

## Reproduction

```console
cargo test --locked -p ucof-experiments --features http-reqwest conditional_reqwest
```

The dedicated feature test remains native-only. Ordinary default workspace and cross-target portability builds remain feature-off.

## Boundary

This experiment does not change EXP-0003 bytes, D1-D7 governance decisions, FCP status, or epoch allocation. It is transport implementation evidence only.
