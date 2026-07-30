# Experiment 0018: Stable-Source Retry and Cancellation Semantics

- **Status:** Reproducible
- **Date:** 2026-07-30
- **Related:** ADR-0013, ADR-0014, FCP-0002
- **Script:** `tools/experiment_exp0002_stable_source_retries.py`

## Question

Which retry and cancellation behaviors preserve one immutable source view during a multi-range assurance operation?

## Model

The experiment uses a scripted conditional range source with a strong 32-byte expected version token. It can produce:

- complete data;
- a transient failure before response bytes;
- a transient failure after partial response bytes;
- a source-version change;
- cancellation.

A multi-range operation reads two ranges and publishes a result only after both complete under the same expected token.

## Tested rules

### Same-version transient retry

A transient request may be retried only against the same expected version. The source must enforce that version as an atomic request precondition or provide equivalent immutable generation semantics.

### Partial-response discard

If a request fails after returning partial bytes, the entire partial response is discarded. No partial bytes become visible to the parser, hasher, or operation result. A retry starts the complete range again under the same expected version.

The experiment records both accepted and discarded wire bytes so retry overhead remains accountable.

### Version change

A version mismatch is terminal for the current operation. The adapter does not adopt the new token and continue, because that would combine bytes from different source versions.

### Cancellation and deadline

Cancellation is terminal for the current operation and produces no verified result. A deadline has the same assurance semantics even though the model represents only cancellation explicitly.

### Whole-operation restart

A caller may begin a new operation with a new source instance and new expected token. The restarted operation cannot reuse parsed state, digest state, page references, candidate classifications, or partial output from the failed operation.

### Retry budget

Attempts are explicitly bounded. Exhaustion returns failure rather than weakening version requirements or publishing partial assurance.

## Findings

1. Conditional retries are compatible with ADR-0013 only when every attempt uses the same expected immutable version.
2. Partial response bytes must be discarded and separately charged.
3. Version mismatch, cancellation, deadline, and retry exhaustion are terminal for one assurance operation.
4. A new version token requires a complete operation restart.
5. Retry policy must not silently convert source mutation into successful recovery or freshness.
6. Coalescing and asynchronous scheduling may change request order, but every request remains conditional on the same token and actual work remains bounded and reported.

## Security implications

- Retrying without an atomic version precondition leaves a time-of-check/time-of-use window.
- Adopting a new token mid-operation permits mixed-version parsing and hash confusion.
- Reusing parser or digest state after restart can mix trusted and untrusted state even when byte buffers are cleared.
- Unbounded retries create denial-of-service and hidden transfer amplification.
- Stable source view is still not external freshness; the source may consistently serve an older valid generation.

## Reproduction

```console
python3 tools/experiment_exp0002_stable_source_retries.py
```
