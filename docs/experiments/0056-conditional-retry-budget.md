# Experiment 0056 — Conditional source operation-wide retry budget

**Status:** reusable synchronous transport-policy evidence  
**Date:** 2026-07-31

## Question

Can transient conditional-source transport failures be retried without weakening strong-version binding, cancellation/deadline behavior, request accounting, or fail-closed byte acceptance?

## Policy

- Metadata and every range transport attempt consume one shared operation-wide budget.
- The client must explicitly classify a failure as retryable.
- Terminal client errors, version changes, protocol errors, cancellation, deadlines, and limits are never retried.
- Cancellation and the monotonic deadline are checked before and after every transport attempt.
- Response bytes are copied to the caller only after the retry wrapper succeeds and the existing conditional adapter validates version, offset, total length, and body length.
- Budget exhaustion is reported as a limit failure. The caller must restart the assurance operation with newly acquired metadata rather than continuing with partial assurance state.

## Implementation

`RetryingConditionalClient` wraps the transport-specific `ConditionalRangeClient`. `ConditionalReadAt::new_with_retry` binds the wrapper and the byte-validating adapter to the same operation control.

The wrapper exposes total transport attempts. The existing adapter continues to expose logical range requests and accepted bytes, keeping those measurements distinct.

## Evidence

Tests cover:

- transient metadata and range failures succeeding exactly at the operation-wide limit;
- exhaustion without copying bytes or increasing accepted-byte accounting;
- terminal authorization-style failures returning after one attempt;
- version change returning after one attempt;
- cancellation preventing a new transport attempt;
- rejection of a zero-attempt policy.

## Assurance boundary

This is a bounded generic retry contract, not a maintained HTTP, object-store, or cloud SDK adapter. It deliberately defines no sleep, jitter, provider-specific status mapping, authentication refresh, redirect policy, or asynchronous interruption of an already-blocking synchronous call.

A concrete adapter must classify only transport failures that are safe to repeat under the same strong version precondition. In particular, it must not classify version mismatch, malformed range metadata, authorization failure, or ambiguous provider responses as retryable.
