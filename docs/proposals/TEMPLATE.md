# FCP-0000: Short proposal title

- **Status:** Draft
- **Authors:**
- **Created:** YYYY-MM-DD
- **Last updated:** YYYY-MM-DD
- **Target:** Core / Profile / Registry / Process
- **Experimental epoch impact:** None / New epoch required / To be determined
- **Related issues:**
- **Related ADRs:**
- **Supersedes:**
- **Superseded by:**

## Summary

Describe the proposed normative change in a few precise paragraphs.

## Motivation

What concrete problem does this solve? Link the affected Phase 0 use cases, threat-model entries, measurements, or interoperability failures.

## Scope

State exactly what this proposal defines.

## Non-goals

State what remains outside the proposal.

## Terminology

Define new terms or link existing glossary definitions. Do not silently redefine core terms.

## Detailed specification

Describe the required serialized representation and behavior precisely enough for an independent implementation.

Include, as applicable:

- byte order and integer rules;
- field layout and allowed ranges;
- canonicalization;
- discovery and active-root behavior;
- reader requirements;
- writer requirements;
- strict validation and diagnostic behavior;
- unknown required, optional, and advisory handling;
- profile constraints;
- error conditions;
- normative pseudocode.

## Compatibility impact

Address separately:

### Existing files with new readers

...

### New files with old readers

...

### Unknown capabilities and data preservation

...

### Profile and schema compatibility

...

### Canonical identity and signatures

...

### Version or experimental epoch impact

...

### Migration and coexistence

...

## Security impact

Identify attacker-controlled inputs, trust boundaries, new failure modes, parser differential risks, and effects on the threat model.

## Privacy impact

State public and protected metadata, equality leakage, stable identifiers, signer or recipient exposure, access-pattern leakage, and redaction implications.

## Resource-limit impact

Define effects on:

- bytes read;
- logical bytes decoded;
- allocation;
- nesting and recursion;
- object, chunk, and dependency counts;
- index traversal;
- transform expansion;
- cryptographic work;
- recovery scanning;
- diagnostic output.

## Streaming impact

Explain whether a writer can operate without seeking and what a sequential reader must buffer.

## Random-access impact

Explain required seeks or range reads, discovery overhead, and index dependencies.

## Recovery, truncation, and compaction

Define behavior after interrupted writes, malformed tails, stale roots, salvage, and physical rewrite where applicable.

## Canonicalization and identity

Define exact canonical input, ordering, normalization, algorithm tagging, domain separation, and identity scope where applicable.

## Alternatives considered

### Alternative A

Description, advantages, disadvantages, and reason not selected.

### Alternative B

Description, advantages, disadvantages, and reason not selected.

## Unresolved questions

List open decisions explicitly. A proposal should not enter Review while questions prevent independent implementation.

## Implementation plan

Describe reference implementation work without making implementation details normative accidentally.

## Evidence and validation

List required:

- prototypes;
- valid vectors;
- invalid vectors;
- cross-language checks;
- truncation simulations;
- fuzzing;
- benchmarks;
- corpus analysis;
- threat-model updates.

## Registry allocations requested

List requested registries and symbolic names. Permanent values are assigned only after acceptance.

## Rollout plan

Describe draft integration, experimental writer behavior, reader support, release notes, and compatibility messaging.

## Rejection or rollback strategy

Explain how an experiment can be removed, how its epoch is retired, and what happens to produced files.

## References

Prefer primary specifications, papers, and implementation documentation.

## Decision record

Completed by maintainers when the proposal is decided.

- **Decision:**
- **Decision date:**
- **Review period:**
- **Approvers:**
- **Blocking objections and disposition:**
- **Required follow-up:**
