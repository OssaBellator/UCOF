# Experiment 0052 — Source-based rewrite and semantic compaction

**Status:** Reusable synchronous Rust evidence  
**Scope:** Immutable-page successor research; no epoch or production streaming claim

## Question

Can verified rewrite and dependency-driven compaction operate over a bounded random-access source without first copying the entire UCOF file into one contiguous memory buffer?

## Implementation

The reusable successor module now provides:

- `rewrite_source_all`;
- `rewrite_source_selected`;
- `semantic_compact_source`.

Each operation:

1. strictly validates the exact-end active snapshot through `ImmutableReadAt`;
2. carries the remaining request and byte budget into a second authenticated directory inventory pass;
3. compares active sequence, snapshot digest, commit digest, root level, object count, and page count to reject a changed source view;
4. resolves selected locators from the authenticated inventory;
5. rereads only selected or dependency-visited object records under the same cumulative source budget;
6. revalidates each record header, locator fields, length, and object digest;
7. emits a deterministic new genesis through the existing verified writer and validates the result.

The semantic compaction path keeps resolver semantics, cycle handling, missing-dependency rejection, independent node/edge/depth limits, and unknown-policy behavior identical to the slice operation.

## Evidence

Integration tests compare source and slice operations for:

- rewrite of all 400 active objects;
- selected rewrite of three objects from a multi-page source;
- dependency-complete semantic compaction;
- operation-wide read-budget exhaustion.

The tracing source enforces 4 KiB maximum read requests and confirms that no individual request materializes the complete input file.

A dedicated `immutable_successor_source_rewrite` cargo-fuzz target:

- generates bounded canonical object sets and selected identifier subsets;
- compares source-selected rewrite bytes with slice-selected rewrite bytes;
- enforces a 256-byte maximum source request;
- mutates one arbitrary source byte and requires source rewrite to reject it;
- runs in the permanent fuzz build and smoke matrix.

## Findings

1. Whole-file source materialization is not required for strict validation, authenticated inventory, selected rewrite, or dependency traversal.
2. A stable source view remains required across validation, inventory, traversal, and record rereads. Strong conditional adapters are the intended remote binding.
3. The current deterministic genesis writer still owns all retained payload inputs before output, so this is not yet a constant-memory output pipeline.
4. Semantic traversal currently rereads retained records when building output. This is explicit cumulative work, not an uncharged cache.
5. Source rewrite changes file-instance and byte-scoped identities and does not preserve signatures.

## Remaining work

- stream retained record payloads directly into the deterministic writer;
- spill locator and payload state under production confidentiality and publication policy;
- bind the operation to concrete conditional HTTP/cloud adapters;
- source-based historical-snapshot retention;
- preservation of unknown optional extensions and provenance policy;
- mutation-during-operation and injected transport-failure fuzzing;
- output cancellation and no-overwrite durable publication.
