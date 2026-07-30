# ADR-0014: Restart the Whole Assurance Operation After Source-Version Change

- **Status:** Accepted
- **Date:** 2026-07-30
- **Scope:** Phase 3 implementation-local source adapters
- **Related:** ADR-0013, Experiment 0018, FCP-0002

## Context

ADR-0013 requires one strong source-version token before and after every length or range read. Real remote storage also exposes transient failures, partial responses, cancellation, deadlines, request coalescing, and retries.

A naive retry policy can violate the stable-view guarantee by:

- adopting a new version token after some bytes have already been parsed or hashed;
- retaining partial response bytes across attempts;
- reusing parser, digest, traversal, candidate, or output state after an operation restart;
- retrying indefinitely and hiding transfer amplification;
- treating source mutation as recovery or freshness.

These choices are implementation behavior rather than UCOF wire fields, but they affect the assurance claim of source-based validation, lookup, recovery, and history enumeration.

## Decision

One assurance operation uses one immutable expected source-version token.

### Request retry

An individual range request may be retried only when:

1. every attempt is atomically conditional on the same expected token or immutable generation;
2. a failed partial response is discarded completely before retry;
3. no partial response bytes are exposed to parsing, hashing, traversal, or output;
4. attempt count, wire bytes, discarded bytes, and elapsed deadline are bounded and accountable.

### Terminal conditions

The current operation fails without a verified result on:

- version mismatch;
- cancellation;
- deadline expiration;
- retry exhaustion;
- inability to enforce the expected version atomically;
- ambiguous partial-response state.

The adapter never adopts a new token mid-operation.

### Restart

A caller may start a new operation with a new source instance and new expected token. A restart begins with fresh:

- parser state;
- hash state;
- page and object traversal state;
- recovery candidate state;
- diagnostics and work accounting;
- output buffers or temporary output.

No partial assurance or cached authenticated structure from the failed operation is reused unless it is independently keyed by immutable content identity and revalidated under a separately specified cache contract. Candidate 1 defines no such cache contract.

### Asynchronous and coalesced reads

An implementation may issue conditional reads concurrently or coalesce adjacent ranges. All underlying requests still use the same expected token, and operation limits charge actual requests, requested bytes, received bytes, discarded bytes, and hashed bytes.

Concurrency must not publish a verified result after cancellation or after any request detects a version mismatch.

## Consequences

### Positive

- Mixed-version parsing and hashing fail closed.
- Retry overhead is visible and bounded.
- Cancellation cannot leave a partially verified result.
- Transport policy remains separate from canonical UCOF bytes.
- Synchronous and future asynchronous adapters share the same assurance boundary.

### Negative

- A source change near operation completion can require repeating substantial work.
- Implementations cannot transparently continue with the newest token.
- Efficient immutable-content caches require a future explicit cache contract.
- Atomic conditional range support is a storage-adapter requirement, not something UCOF can infer from bytes alone.

## Alternatives rejected

### Adopt the newest token mid-operation

Rejected because it can combine bytes from different file generations.

### Retry partial ranges from the failure offset

Rejected unless the transport proves the partial and resumed bytes belong to the same immutable response. The baseline policy restarts the full range and discards partial bytes.

### Reuse completed page validations after restart

Rejected for Candidate 1 because page and commit identities do not define a general cross-operation cache contract, and source freshness remains external.

### Disable all retries

Rejected as an implementation-wide rule because same-version conditional retries can safely handle transient transport failures under explicit limits.

## Evidence

Experiment 0018 demonstrates:

- successful retry after a transient failure before bytes;
- complete discard and retry after a partial response;
- terminal failure on version change;
- terminal failure on cancellation;
- bounded retry exhaustion;
- successful independent restart with a new token and no reused partial result.

## Review trigger

Revisit this ADR when:

- a concrete HTTP or cloud-object adapter is implemented;
- asynchronous APIs are added;
- an immutable-content cache contract is proposed;
- a storage backend cannot provide atomic conditional ranges;
- transport work accounting becomes part of a public API contract.
