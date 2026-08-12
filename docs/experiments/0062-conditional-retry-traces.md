# Experiment 0062 — Independent conditional retry traces

**Status:** cross-implementation transport-policy evidence  
**Date:** 2026-07-31

## Question

Can operation-wide conditional retry decisions and accounting be reproduced from machine-readable traces without invoking the Rust retry wrapper or a real transport client?

## Contract

`retry-traces.json` defines metadata and range outcome sequences under one total transport-attempt budget. Outcomes distinguish:

- success;
- explicitly retryable transient failure;
- terminal client failure;
- source version change;
- protocol failure;
- cancellation;
- deadline expiry.

Logical range requests, transport attempts, and accepted bytes are independently reported.

## Independent verifier

`verify_exp0002_immutable_retry_traces.py` implements the policy directly in Python:

- reject a zero-attempt policy;
- check cancellation or deadline before beginning the affected operation;
- charge every metadata and range transport attempt to one shared budget;
- retry only an explicitly retryable failure;
- stop immediately on terminal, version, protocol, cancellation, or deadline outcomes;
- accept range bytes only after a successful terminal outcome;
- emit no accepted bytes for failed traces;
- evaluate every trace twice and pin an aggregate canonical-result SHA-256.

## Cases

- metadata and range retries succeeding exactly at the limit;
- metadata exhaustion;
- range exhaustion after metadata consumed part of the budget;
- terminal failure without retry;
- version change without retry;
- protocol failure without retry;
- cancellation before a range without another transport attempt;
- deadline before metadata without any attempt;
- multiple ranges sharing one budget;
- invalid zero-attempt policy.

## Assurance boundary

These traces verify generic retry classification and accounting. They do not define HTTP status mapping, provider error interpretation, backoff, jitter, authentication refresh, redirects, SDK behavior, or asynchronous interruption. Concrete adapters remain responsible for proving that only safe, repeatable failures are classified as retryable under the same strong-version precondition.
