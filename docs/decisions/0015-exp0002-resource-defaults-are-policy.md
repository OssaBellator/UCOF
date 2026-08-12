# ADR-0015: Candidate 1 Resource Defaults Are Deployment Policy, Not Conformance Minima

- **Status:** Accepted
- **Date:** 2026-07-30
- **Scope:** Phase 3 implementation and proposal interpretation
- **Related:** Experiment 0019, FCP-0002

## Context

Candidate 1 exposes many independent hostile-input ceilings, including file bytes, objects, pages, depth, roots, capabilities, payload bytes, source reads, bytes read, bytes hashed, recovery work, rewrite work, and diagnostics.

These defaults were selected incrementally to bound implementation behavior. They were not designed as one jointly satisfiable interoperability profile.

Experiment 0019 demonstrates a direct conflict:

- `max_objects = 10,000,000`;
- `max_read_operations = 1,000,000`.

Even an unrealistically optimistic full source validator requiring one object-related read per object cannot reach the object ceiling before exhausting the operation ceiling.

Similar interactions occur in recovery, where cumulative candidate bytes can bind long before the nominal candidate-validation count for large files.

Promoting these defaults directly into a specification would advertise combinations no conforming implementation could actually support.

## Decision

Candidate 1 and successor implementation defaults remain non-normative deployment policy.

A future conformance support profile must define jointly satisfiable minima across all relevant dimensions, including:

- file and commit bytes;
- objects and payload bytes;
- pages and depth;
- roots, capabilities, and extensions;
- source read operations, bytes read, and request size;
- bytes hashed and allocation;
- recovery scan, candidate, chain, and diagnostic work;
- writer sort, spill, merge, output, and publication work.

An implementation may enforce lower deployment policy limits, but it must report a resource-policy refusal rather than malformed input when the bytes are otherwise structurally valid.

An implementation claiming a named support profile must demonstrate that the profile minima are simultaneously reachable through boundary vectors or virtual-source tests.

## Consequences

### Positive

- Avoids canonizing arbitrary prototype defaults.
- Prevents impossible support claims.
- Preserves low-resource and embedded implementation choices.
- Keeps denial-of-service policy adjustable by deployment.
- Makes conformance profiles testable as multidimensional work envelopes.

### Negative

- Candidate 1 has no universal large-file interoperability floor.
- Applications must inspect implementation support declarations.
- Profile design requires constructed boundary fixtures rather than copying constants from code.
- Two conforming experimental implementations may accept different maximum valid files until a support profile is accepted.

## Result categories

At minimum, public APIs and tools should distinguish:

- valid and verified;
- structurally invalid;
- integrity failure;
- unsupported required semantics;
- resource-policy refusal;
- I/O or source instability;
- explicitly requested recovery or salvage results.

A resource-policy refusal must not be cached or reported as proof that the file is malformed.

## Alternatives rejected

### Promote current defaults unchanged

Rejected because Experiment 0019 proves they are not one coherent support class.

### Specify only one file-size minimum

Rejected because object count, page count, reads, hashing, and allocation can bind first.

### Require unlimited processing for structurally valid files

Rejected because hostile-input safety requires caller-controlled work limits.

### Leave result categories unspecified

Rejected because applications could confuse local policy refusal with corruption or unsupported semantics.

## Required follow-up

Before FCP-0002 Review or a successor Review:

1. propose at least one jointly satisfiable support profile;
2. publish boundary vectors or virtual sources for every dimension;
3. verify the profile in independent implementations;
4. define machine-readable support discovery if applications need preflight selection;
5. document how stricter deployment policy is reported.

## Review trigger

Revisit this ADR when a normative support profile is proposed or when a public stable API defines resource-limit result categories.
