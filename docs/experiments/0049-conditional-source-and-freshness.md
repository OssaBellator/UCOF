# Experiment 0049 — Conditional source and trusted freshness boundaries

**Status:** Reusable synchronous Rust evidence  
**Scope:** Immutable-page successor research only; no epoch or transport requirement is allocated

## Question

Can one bounded random-access assurance operation prevent mixed-version reads, respond explicitly to cancellation and deadlines, and distinguish integrity from application freshness without embedding a network stack in the core experiment?

## Implementation

The reusable successor module now provides:

- `StrongVersionToken`, rejecting weak HTTP entity tags and malformed tokens;
- `ConditionalRangeClient`, a transport-facing metadata and conditional-range contract;
- `ConditionalReadAt`, which binds its lifetime to one strong token and exact object length;
- response checks for token, offset, total length, and exact body length before copying bytes;
- shared cancellation and monotonic deadline state checked before and after each synchronous request;
- `TrustedFreshnessCheckpoint` and `evaluate_freshness` for explicit rollback and same-sequence fork decisions.

HTTP adapters should implement the conditional request with `If-Match`. Cloud object adapters may use an immutable provider version identifier with equivalent no-mixing behavior. A failed operation is terminal: callers construct a new adapter from newly acquired metadata rather than reusing accepted bytes as assurance state.

## Evidence

Integration tests cover:

- complete strict source validation through one bound version;
- version change between reads;
- cancellation after adapter construction;
- an already-expired deadline;
- wrong response version, offset, total length, and short body;
- weak-token rejection;
- unpinned, current, advancing, rollback, and same-sequence-fork freshness outcomes.

## Findings

1. Stable source view prevents mixed-version reads but does not establish that the selected version is the newest authorized state.
2. Freshness requires trusted external state and an application rule for durably accepting an advance.
3. Synchronous cancellation cannot interrupt an already-blocking transport call; it can prevent the result from being accepted after the call returns. Native asynchronous adapters remain separate production work.
4. Provider-specific authentication, retry classification, TLS policy, credential handling, and request coalescing remain outside this reusable contract.

## Remaining work

- concrete maintained HTTP and cloud implementations;
- asynchronous cancellation tests against real transports;
- retry and backoff policy with operation-wide budgets;
- trusted checkpoint storage integration and crash-consistent advancement;
- hostile intermediary and cache behavior tests.
