# Experiment 0086: Conditional HTTP response classification

## Question

Can an HTTP-style conditional range adapter classify responses conservatively and deterministically before provider-specific code, while keeping retries, authentication refresh, version changes, and protocol failures distinct?

## Policy

The classifier accepts only:

- metadata status `200` with no body or content range, an explicit length, and a valid strong version token;
- range status `206` with an exact strong-version match, content length, content range, total object length, and body length.

HTTP `412` is a terminal source-version change. Redirects are protocol failures and are never followed automatically. HTTP `401` requests one authentication refresh only when the application explicitly permits it; `403` remains terminal. `404` and `416` are terminal client outcomes.

Only `408`, `425`, `429`, `500`, `502`, `503`, and `504` are generically retryable. Unknown `4xx` and `5xx` statuses are terminal because a generic layer cannot safely infer provider semantics. A positive `Retry-After` value becomes a minimum passed to the existing bounded delay planner; values above the configured cap are rejected rather than truncated.

## Evidence

Rust tests pin:

- exact metadata and range acceptance;
- weak or missing version rejection;
- mismatched range versions and content ranges as protocol failures;
- terminal version, authorization, redirect, missing-object, and unsatisfiable-range decisions;
- explicit authentication-refresh authority;
- the retryable status allowlist and server-minimum delay composition;
- unknown `5xx` terminal behavior;
- rejection of `200` full-body responses to range requests;
- failure of an oversized server retry delay without charging retry state.

## Boundary

This is a pure response-head classifier and delay-plan adapter. It performs no HTTP requests, body streaming, redirects, authentication refresh, waits, jitter, or cancellation. Concrete maintained adapters must map provider-specific statuses, parse headers without ambiguity, bind bodies to the classified head, and check cancellation and monotonic deadlines around real waits and transport calls.
