# Experiment 0106: Per-state historical semantic selection planning

## Question

Can multiple retained historical states receive independent dependency graphs and trusted roots without reusing one global closure across time?

## Policy

`plan_historical_semantic_selections` accepts one request per retained sequence. Each request carries its own `ObjectGraph` and root set. Requests are canonicalized chronologically and duplicate sequences are rejected.

Every state invokes the existing bounded compaction planner independently. Reachable objects, orphaned objects, edge counts, and maximum depth are therefore state-local facts. The aggregate plan enforces both a maximum state count and a cumulative reachable-object bound.

## Evidence

Deterministic tests cover:

- two states with the same object identifiers but different edges producing different reachable and orphan sets;
- chronological canonicalization of caller order;
- duplicate-sequence rejection;
- cumulative reachable-object limit enforcement.

## Boundary

This is graph-only planning. It does not validate that a sequence exists in a source history, prove that selected objects are present in that state, emit a multi-snapshot chain, preserve provenance or signatures, spill large graphs, or adopt a normative application dependency profile. Those remain separate source/output, large-graph, and policy layers.
