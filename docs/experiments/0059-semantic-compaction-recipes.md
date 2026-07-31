# Experiment 0059 — Independent semantic compaction graph recipes

**Status:** cross-implementation policy evidence  
**Date:** 2026-07-31

## Question

Can semantic-compaction retention and failure decisions be reproduced from a machine-readable dependency graph without calling the Rust resolver or byte-rewrite implementation?

## Contract

`graph-recipes.json` defines eight active objects with:

- a reachable cycle;
- shared dependencies;
- an object with unknown semantics;
- an object that references a missing dependency;
- an object whose resolver fails.

Each case supplies selected roots, unknown-semantics policy, independent root/node/edge/depth limits, and expected retained/discarded identities or a single fail-closed error.

## Independent verifier

`verify_exp0002_immutable_compaction_recipes.py` implements the policy directly in Python:

- canonicalize selected roots;
- traverse iteratively with identity-based cycle suppression;
- canonicalize and deduplicate dependency lists;
- count every declared known edge before following it;
- check dependency existence before enqueueing;
- reject unknown semantics or retain the complete active set;
- independently enforce root, node, edge, and depth limits;
- emit no partial retention claim for error cases.

The verifier evaluates every case twice, compares expected fields, canonicalizes the complete result set as JSON, and pins its SHA-256 aggregate.

## Cases

- reachable cycle and shared dependency closure;
- cycle selected directly;
- duplicate and unsorted root canonicalization;
- unknown semantics with rejection;
- unknown semantics with conservative full retention;
- missing dependency;
- resolver failure;
- node limit;
- edge limit;
- dependency-depth limit.

## Assurance boundary

These recipes validate dependency-selection policy, not source bytes, payload interpretation, profile conformance, rewrite identities, historical retention, extension preservation, or provenance reissuance. Rust integration tests continue to cover the byte-backed operation; this experiment prevents those tests from being the only implementation of the graph policy.
