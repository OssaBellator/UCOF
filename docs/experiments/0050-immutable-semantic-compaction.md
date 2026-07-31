# Experiment 0050 — Reusable immutable-successor semantic compaction

**Status:** Reusable synchronous Rust evidence  
**Scope:** Active-snapshot object retention only; no epoch or profile contract is allocated

## Question

Can the immutable-page successor rewrite a dependency-complete active object set while keeping semantic interpretation, unknown-object policy, work limits, and byte-identity consequences explicit?

## Implementation

`semantic_compact` now:

1. strictly validates the exact-end active source;
2. canonicalizes caller-selected logical roots;
3. asks a profile or application resolver for each visited object's dependencies;
4. traverses iteratively with cycle-safe identity tracking;
5. independently limits roots, nodes, edges, and depth;
6. rejects missing logical dependencies;
7. rewrites the retained set into a new genesis file through the verified-source rewrite path.

The output reports roots, retained and discarded identifiers, traversal work, maximum depth, unknown-semantics trigger, and whether conservative full retention occurred.

## Unknown dependency semantics

Two policies are exposed:

- `Reject`: stop at the first object whose dependency semantics are unknown;
- `RetainAllActive`: retain every object in the strictly validated active snapshot.

Retaining only the objects whose semantics are unknown is not a sound conservative policy. An unknown object may depend on a known object that was otherwise unreachable, so the generic tool cannot prove completeness without retaining the entire active set.

## Boundary vectors

`tests/vectors/exp-0002-immutable-compaction/cases.tsv` pins:

- an exact reachable set;
- unknown-semantics rejection;
- unknown-triggered full active retention.

Rust integration tests additionally cover cycles, missing dependencies, resolver failure, absent roots, independent node/edge/depth limits, and signature invalidation through rewrite.

## Findings

1. Semantic compaction is a claim relative to a resolver contract, not a property inferred from container bytes alone.
2. Caller-selected rewrite remains a lower-assurance operation because it cannot establish dependency completeness.
3. Unknown semantics require abort or whole-active-set retention.
4. Rewriting changes commit identity and does not preserve byte-scoped signatures.
5. Active-only compaction does not implement snapshot-history retention, provenance reissuance, or extension preservation.

## Remaining work

- profile-specific resolver contracts and conformance vectors;
- selected snapshot-history retention and pinned historical roots;
- unknown optional extension preservation policy;
- provenance and signature reissuance rules;
- source-streaming rewrite without whole-file materialization;
- large graph spill strategy and hostile resolver testing.
