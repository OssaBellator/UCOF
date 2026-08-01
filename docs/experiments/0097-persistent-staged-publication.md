# Experiment 0097: Version-bound private staging and publication

## Question

Can strong-version base copying and a preconstructed persistent tail be composed with private staging so partial output is never destination-visible, while publication durability and indeterminate outcomes remain explicit?

## Construction

`stage_and_publish_versioned_source_with_tail` accepts a `PersistentVersionedReadAt` source and a `PersistentStagingBackend`. The backend contract separates:

1. private artifact creation;
2. bounded source/tail writes;
3. complete staged length and SHA-256 validation;
4. private-file synchronization;
5. no-overwrite destination publication;
6. parent namespace synchronization;
7. private-name retirement or prepublication abort.

The orchestrator reuses Experiment 0096 for whole-file identity, strong non-ABA version checks, cumulative read budgets, and tail withholding. It hashes the complete staged output while writing and supplies the expected length and digest to backend validation.

Publication outcomes are explicit:

- destination already exists: definitely not published, with private abort attempted;
- link indeterminate: publication state unknown and private state retained;
- link succeeds but parent sync fails: publication durability indeterminate and private state retained;
- parent sync succeeds: publication is durable; later private-name cleanup failure is reported but cannot downgrade durability.

## Evidence

Rust tests cover durable success, destination-exists refusal, indeterminate link, parent-sync failure, cleanup failure after durability, and copy-phase source-version failure before any link.

The `immutable_successor_persistent_staged_publication` fuzz target varies source and tail bytes, read/write chunks, strong-version changes, destination state, link outcomes, backend stage failures, and abort failures. A destination may remain its original sentinel or become the exact complete staged artifact; it may never contain a partial or mixed artifact.

## Boundary

This is a backend contract and orchestration model. It does not implement filesystem path resolution, ownership, descriptor-relative handles, encryption, authenticated journals, physical power-loss qualification, network-filesystem policy, or platform-specific directory synchronization. Those remain production spill/publication gates.
