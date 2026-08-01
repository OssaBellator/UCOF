# Experiment 0082: versioned historical semantic streaming

## Question

Can one exact linked-history state from a non-ABA versioned bounded source be reduced by bounded dependency closure and emitted through a bounded canonical sink without materializing the source file or complete output?

## Construction

The experiment composes four previously separate assurance layers:

1. bounded graph planning over caller-supplied object dependencies;
2. complete linked-history validation under one strong source version;
3. exact historical-prefix selection by authenticated sequence/footer boundary;
4. selected active-object streaming under the remaining cumulative source budget.

`rewrite_versioned_source_selected_to` filters an authenticated active inventory before emission. Selection is canonicalized by identifier and must be non-empty, duplicate-free, and fully present. The selected-history wrapper locks every prefix version check to the version observed before history validation. `rewrite_compacted_versioned_history_sequence_to` performs graph planning first and preserves separate compaction versus historical/source/output errors.

## Evidence

Unit tests cover:

- exact byte equality with owned selected rewrite of a historical prefix;
- deterministic reachable/orphan sets, edge counts, and depth;
- exact selected payload reread accounting;
- bounded source requests and sink writes;
- graph failure before source work or output;
- reachable objects missing from the historical state before output;
- missing and duplicate source selections before output.

The `immutable_successor_historical_semantic_streaming` fuzz target varies object counts, one-to-three linked commits, selected sequence, graph shape, root order, source/hash/payload/sink chunk sizes, replacement locations, invalid graphs, missing source objects, and history-time version changes. It compares emitted bytes with the owned exact-prefix selected writer and checks cumulative reread accounting and request bounds.

## Boundary

This is a research composition, not a normative application profile. The caller still supplies the dependency graph and trusted roots. Complete selected-prefix inventory validation may read every active payload once; only reachable payloads are reread for output. The result is a new genesis file and does not preserve historical identity, extensions, provenance, or signatures. Concrete maintained provider adapters, retry/authentication integration, atomic staging/publication, multi-snapshot output, and large-graph spill remain separate.
