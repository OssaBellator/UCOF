# EXP-0002 algorithm-model security findings

## Status

This document records executable findings from the non-normative Phase 3 research models. No EXP-0002 byte layout exists yet. These findings constrain FCP-0002 but do not define a wire format, cryptographic scope, or compatibility promise.

## Evidence surface

The findings come from:

- `ucof-experiments::directory`;
- `ucof-experiments::snapshots`;
- `ucof-experiments::enumeration`;
- `ucof-experiments::publication`;
- `ucof-experiments::recovery`;
- `ucof-experiments::compaction`;
- `ucof-experiments::repair`;
- `tools/experiment_directory_models.py`;
- `tools/validate_phase3_models.py` and its independent JSON cases;
- dedicated cargo-fuzz targets;
- stable, Rust 1.85, 32-bit, and big-endian CI.

## Confirmed design constraints

### Paged lookup is required

The flat EXP-0001 directory remains unsuitable for massive object counts. The Phase 3 ordered-page model demonstrates bounded root-to-leaf lookup without materializing all entries.

With representative research parameters of 240 leaf entries and internal fanout 256:

- 1,000 entries require 6 pages and depth 2;
- 1,000,000 entries require 4,185 pages and depth 3;
- 100,000,000 entries require 418,303 pages and depth 4.

These figures do not select a page size or encoding. They demonstrate that a future design must preserve bounded path lookup and page-local validation.

The model rejects duplicate leaf identifiers, unordered keys, overlapping child ranges, forged child-range claims, invalid page references, and page cycles. Page-read limits apply before unbounded traversal.

### A compact monolithic array is not an append-friendly default

The initial comparison model finds that a 64-byte sorted entry array is nominally compact but rewrites approximately 6.0 GiB for one canonical insertion at 100 million entries. This is incompatible with frequent append snapshots and immutable object-storage workflows unless chunking or another indirection layer changes the model.

### Expected constant-time hashing is not sufficient evidence

The hash-page baseline has a short expected lookup path but leaves unresolved deterministic collision handling, overflow limits, canonical resizing, ordered enumeration, authenticated locator pages, and attacker-selected keys. A hash layout must include those costs before it can displace the ordered-page baseline.

## Root-selection findings

### Strict validation and recovery must remain separate

The exact-end model accepts only one verified complete candidate at the physical end. It never scans backward implicitly.

The recovery model may consider earlier candidates under independent limits. An interrupted append can therefore produce:

- strict failure for the damaged full source;
- recovery of the earlier complete snapshot;
- an explicit statement that the recovered snapshot may be stale.

An exhaustive model test covers every truncation point across an 8,192-byte append while preserving recovery of the earlier candidate when the configured scan window includes it.

### Candidate discovery requires independent budgets

The generic recovery scanner has separate limits for:

- bytes scanned;
- magic matches;
- candidate validations;
- returned results.

A tail filled with candidate magic fails once its candidate or validation budget is exhausted. A magic match never becomes a valid root without caller-supplied validation.

A valid candidate outside the bounded search window is not guessed. The report states that earlier bytes were unsearched.

### Sequence does not resolve forks or freshness

The parent-chain model rejects:

- missing parents;
- unverified parents;
- progress checkpoints in a complete chain;
- non-increasing sequences;
- sequence gaps under the current research rule;
- parent cycles;
- excessive parent depth.

Two unrelated highest candidates at the same sequence form an ambiguous fork and are never selected silently.

Snapshot sequence is only file-local ordering evidence. It does not prove freshness against replacement of the entire source with an older valid copy.

## Enumeration findings

Root enumeration and active-root selection are separate operations.

The enumerator reports every bounded candidate as one of:

- verified terminal;
- verified ancestor;
- verified highest-fork terminal;
- progress checkpoint;
- integrity failed;
- unsupported required capability;
- truncated;
- invalid;
- missing parent;
- parent not verified;
- non-increasing sequence;
- sequence gap;
- parent cycle;
- parent depth exceeded.

Results are ordered by physical footer recency but physical order does not override authenticated chain validity. Equal highest forks are marked rather than selected.

## Publication and checkpoint findings

The publication state model enforces the logical order:

1. objects;
2. directory leaves;
3. directory root;
4. snapshot manifest;
5. footer.

Only a complete footer event publishes the main snapshot. Every interruption before that event preserves the previous complete snapshot as authoritative.

Complete and progress checkpoints have different authority:

- a complete checkpoint is independently readable and active-root eligible;
- a progress checkpoint is not independently readable and cannot be selected as an active root.

Stages cannot regress or repeat, a footer cannot precede the snapshot manifest, and event/checkpoint counts are bounded.

## Compaction findings

Compaction planning uses iterative reachability with independent limits for:

- nodes;
- edges;
- dependency depth.

Graph cycles terminate through visited-identity tracking rather than recursion. Missing dependencies fail closed. The plan separates reachable objects from physical orphans deterministically.

The model does not authenticate dependencies. A future compactor must operate only from a verified selected snapshot and authenticated directory/object graph.

## Repair findings

Repair planning accepts only a verified complete snapshot. It rejects integrity-failed candidates and progress checkpoints before reachability or copying.

For reachable objects it requires:

- exactly one locator per object;
- checked physical ranges;
- non-overlapping copy ranges;
- bounded range count;
- bounded total copy bytes.

The plan always requires a new snapshot identity and states that byte-scoped signatures cannot be preserved. Salvaged but unverified records cannot be silently included in a verified repair plan.

## Independent and fuzz evidence

The Rust models are checked against independent Python cases for:

- directory shape;
- strict linear-chain selection;
- interrupted-append recovery;
- ambiguous forks;
- progress-checkpoint exclusion;
- reachability and orphan reporting;
- graph-cycle termination.

The branch currently has thirteen fuzz targets covering:

1. EXP-0001 full-file validation;
2. canonical metadata;
3. metadata-only inspection;
4. prefix salvage;
5. sequential reading;
6. writer round trips;
7. paged directories;
8. snapshot selection;
9. root enumeration;
10. publication state transitions;
11. recovery scanning;
12. compaction planning;
13. repair planning.

All targets compile under nightly and run bounded pull-request smoke campaigns. Scheduled campaigns retain read-only repository permissions.

The research crate also compiles at Rust 1.85 and for 32-bit little-endian and 64-bit big-endian targets.

## Residual risks

The current models do not resolve:

- page size or entry encoding;
- physical page locators;
- page and snapshot identity algorithms;
- digest domain separation and scope;
- footer size and fields;
- previous-footer pointer representation;
- authenticated candidate classification;
- remote source mutation during validation;
- copy-on-write page reuse and garbage collection;
- deterministic external sorting for streaming writers;
- cryptographic freshness or rollback resistance;
- protected directory metadata;
- signature or provenance semantics;
- transform and compression interactions;
- concrete repair and compaction output bytes.

In-memory page indexes and candidate structures must never be copied directly into a wire specification. Every physical range, identifier, authentication scope, and error rule still requires a language-neutral EXP-0002 specification and independent byte implementation.

## Required next evidence

Before FCP-0002 can move to Review:

- implement and compare 4 KiB, 16 KiB, and 64 KiB page layouts;
- compare fixed binary entries and restricted canonical metadata pages;
- measure remote range requests and cache effects;
- model copy-on-write page reuse across realistic append workloads;
- define a candidate footer and authenticated parent/directory scopes;
- publish deterministic Rust and independent Python byte vectors;
- fuzz concrete page, snapshot, footer, and recovery parsers;
- simulate interruption at every byte boundary around candidate publication;
- implement verified root enumeration and repair/compaction against actual EXP-0002 bytes;
- integrate concrete-byte findings into the primary threat model.
