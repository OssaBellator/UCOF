# Experiment 0043 — Conditional range adapter and cancellation semantics

## Status

Executable non-normative successor transport evidence.

## Question

Can a mutable or remote range source preserve one stable byte view while still supporting retries, deadlines, cancellation, and request coalescing?

## Model

`tools/experiment_exp0002_conditional_range_adapter.py` implements two concrete source patterns:

1. an `If-Match`-style current-object adapter, where every range response must carry the exact strong version token used to start the assurance operation;
2. an immutable-version adapter used to test asynchronous coalescing keyed by version token, offset, and length.

The model validates all response metadata before exposing bytes:

- status must be partial-content success;
- the response token must be strong and exactly equal to the expected token;
- start, inclusive end, total length, and body length must exactly match the requested range;
- weak validators are rejected before I/O;
- short or metadata-inconsistent responses expose no accepted bytes.

A logical request budget models a deadline boundary without depending on wall-clock timing. Cancellation is checked before every new request.

## Cases

The executable cases prove:

- mutation between two range reads causes the second `If-Match` request to fail and the operation returns no mixed result;
- a caller may begin a new operation with a new token, but bytes accepted by the failed operation are not reusable assurance state;
- cancellation after one accepted range is terminal for that operation;
- deadline exhaustion after one accepted range is terminal for that operation;
- weak ETags, wrong ETags, wrong content ranges, and short bodies fail before bytes are accepted;
- two concurrent requests for the same `(strong token, offset, length)` share one underlying immutable-version read;
- cancelling one waiter does not cancel the shared read needed by another waiter;
- requests for different version tokens never coalesce.

## Findings

1. **The coalescing key must include the strong immutable version token.** Offset and length alone are unsafe.
2. **Cancellation belongs to a waiter and an assurance operation, not automatically to a shared transport read.** Cancelling one waiter must not corrupt another waiter using the same immutable request.
3. **Version change, cancellation, and deadline are terminal for one assurance operation.** A retry with a new version is a new operation with no inherited validation state.
4. **Response metadata is part of the trust boundary.** A byte body is not acceptable without exact version and range evidence.
5. **Stable view is not freshness.** A strong token prevents mixed-version reads but cannot prove that the selected version is the newest trusted state.

## Non-claims

This experiment does not select one HTTP client, cloud provider, asynchronous runtime, retry backoff, or cancellation API. It does not prove external freshness or authenticity. It is an executable contract for adapter behavior that production integrations must preserve.

## Reproduction

```text
python3 tools/experiment_exp0002_conditional_range_adapter.py
```
