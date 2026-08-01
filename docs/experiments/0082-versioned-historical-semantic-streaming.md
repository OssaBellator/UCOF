# Experiment 0082: Versioned Historical Semantic Streaming

**Status:** Research evidence  
**Date:** 2026-08-01  
**Epoch allocation:** None

## Question

Can one strongly versioned bounded random-access source select an authenticated linked-history sequence, compute a bounded dependency closure, and emit only the reachable active objects into canonical genesis output without materializing the complete input or output?

## Construction

This experiment stacks three explicit contracts:

1. `ObjectGraph::plan` computes bounded dependency closure before any source read or sink write.
2. `rewrite_selected_versioned_source_sequence_to` validates the complete linked history under one non-ABA version, chooses one exact footer-bounded prefix, and applies the remaining cumulative source budget to that prefix.
3. `rewrite_selected_versioned_source_to` strictly inventories the selected prefix, canonicalizes the reachable identifier set, rereads only those payloads for output, rechecks their authenticated digests, and emits a new canonical genesis file through a bounded sink.

Graph, source, selection, version, and output failures remain distinguishable. Missing graph dependencies fail before touching the source. Missing selected source objects fail after strict source validation but before output.

## Evidence

The Rust tests pin:

- exact byte equality with an owned selected rewrite of the same historical prefix;
- deterministic reachable and orphaned identifiers;
- edge and maximum-depth accounting;
- canonical caller-order-independent selected identifiers;
- exact selected second-pass payload bytes;
- bounded source and sink request sizes;
- graph failure before any source read or output;
- missing historical objects before output;
- terminal source-version change handling.

The `immutable_successor_historical_semantic_streaming` fuzz target varies:

- one to eight active objects;
- one to three linked commits;
- selected historical sequence;
- acyclic dependency shape and root;
- source request, hash, payload, and sink chunk bounds;
- replacement payloads;
- missing graph dependencies;
- source-version changes during history validation.

It compares the streamed bytes and report against the owned exact-prefix selected rewrite and verifies exact selected second-pass payload accounting.

## Important boundary

Strict inventory authenticates every active object in the chosen historical state. The semantic selection avoids unselected **second-pass** payload reads and output bytes; it does not eliminate the first validation pass over those payloads.

The output is a new genesis file. It does not preserve the selected source sequence number, linked history, offsets, commit identity, extensions, provenance, or signatures. The graph is caller-supplied and is not inferred from opaque payloads. Application-profile adoption, large-graph spill, multi-snapshot historical output, concrete maintained transport adapters, retry/authentication integration, and atomic publication remain separate gates.
