# Experiment 0078: dependency-aware streaming compaction

## Question

Can a caller-supplied semantic dependency graph be combined with active-file streaming so that only reachable authenticated payloads are read and emitted, while graph failures remain distinct from source or sink failures?

## API

`rewrite_compacted_active_file_to` composes two existing bounded layers:

1. `ObjectGraph::plan` computes deterministic dependency closure under independent node, edge, and depth limits.
2. `rewrite_selected_active_file_to` strictly validates the complete active source, filters authenticated payload sources by the reachable identifier set, and streams canonical genesis output through bounded source and sink requests.

The result reports both the compaction plan and the selected streaming report. `ImmutableSemanticStreamingError` preserves whether failure came from graph planning or from source/output processing.

## Evidence

Pinned tests cover:

- a graph rooted at object 1 reaching objects 1, 2, 3, and 4 while orphaning object 5;
- byte equivalence with the existing selected rewrite;
- exact active payload read accounting, excluding a 4,096-byte orphan and an inactive replaced record;
- independent graph edge and maximum-depth facts;
- bounded 7-byte source reads and 113-byte sink writes;
- missing graph dependencies failing before source validation or sink output;
- a graph-reachable object absent from the active file failing before the first sink byte.

## Boundary

The caller supplies semantic dependencies. This API does not interpret opaque payloads, choose application roots, or define a normative reference profile. It begins from an in-memory source slice and produces a new genesis file, so historical, offset, snapshot, commit, extension, provenance, and signature identity are not preserved. Bounded remote inventory and production publication remain separate work.
