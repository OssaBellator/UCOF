# Phase 3 Status — Directory, Snapshots, and Recovery

**Status:** In progress; EXP-0002 Candidate 1 is executable but rejected as a reusable-page design, while an immutable-page successor has reusable Rust evidence without a new epoch allocation  
**Started:** 2026-07-30  
**Working branch:** `phase-3/directory-snapshots-recovery`  
**Stacked pull request:** #3  
**Depends on:** Phase 2 pull request #2

## Objective

Deliver bounded random access, append publication, snapshots, previous-root recovery, repair, rewrite, and compaction inputs while preserving the rule that damaged, recovered, historical, partially interpreted, or merely plausible state never silently becomes active valid state.

## Epoch boundaries

### EXP-0002 Candidate 1

Candidate 1 is a complete disposable byte experiment documented in `docs/spec/EXP_0002_BYTE_CANDIDATE.md`.

It provides:

- a 64-byte bootstrap header;
- 48-byte object records;
- fixed 16 KiB authenticated pages;
- 88-byte leaf and 64-byte internal entries;
- complete snapshots and exact-end commit footers;
- deterministic Rust and Python writers;
- strict slice and bounded source validation;
- targeted authenticated lookup and absence;
- explicit recovery and verified linked history;
- repair-all and caller-selected rewrite;
- an experimental CLI.

Candidate 1 remains unpublished and has no compatibility promise.

### Candidate 1 architectural rejection

Candidate 1 authenticates the active snapshot sequence inside every page and requires page-sequence equality during validation. An unchanged historical page therefore cannot be reused. Re-encoding the page changes its digest and propagates through every ancestor.

At 100 million objects, the Candidate 1 model rewrites approximately 8.89 GB of pages for one object change, while an immutable no-split path rewrites 64 KiB.

This is a wire-design blocker, not merely an unfinished writer optimization. Candidate 1 remains useful as disposable security and implementation evidence but is not the reusable-page successor baseline.

### Immutable-page successor microformat

The successor experiments remove active snapshot sequence from page identity and use immutable content-addressed pages. They are executable non-normative evidence, not Candidate 2.

The repository now contains:

- a standalone non-epoch byte draft;
- deterministic Python and Rust generation;
- a reusable synchronous Rust experiment module;
- manifest-pinned and recipe-pinned valid, invalid, interrupted, fork, append, and multi-level evidence;
- bounded source, history, recovery, rewrite, and lookup APIs;
- continuous fuzz and portability checks.

No successor compatibility promise exists.

## Accepted experimental decisions

- Strict validation is exact-end and never invokes recovery.
- Recovery is explicit, independently bounded, report-only, and never selects a candidate.
- Verified history revalidates every linked prefix and is stronger than active-state validation.
- Structural snapshot identity and file-instance commit identity are separate scopes.
- Candidate 1 checkpoints are ordinary complete commits.
- One remote assurance operation uses one strong source-version token.
- Version change, cancellation, deadline, or exhausted retries terminate the operation; a new token starts clean.
- Current numeric limits are implementation policy ceilings, not normative conformance minima.
- Repair and rewrite accept only strictly verified sources and publish new commit identity.
- Unknown required capabilities preserve integrity evidence but block interpretation.
- Unknown optional extension bytes require an explicit preservation policy.
- Immutable content identity is the current successor research direction; it provides neither authenticity nor freshness.

## Candidate 1 implementation status

| Frontier | Status |
|---|---|
| Complete byte codec | Implemented and tested |
| Deterministic Rust/Python genesis and append writers | Implemented |
| Strict slice validator | Implemented |
| Bounded strict source validator | Implemented |
| Targeted lookup and authenticated absence | Implemented |
| Exact-end/recovery separation | Implemented |
| Bounded recovery and candidate reporting | Implemented |
| Verified linked history | Implemented |
| Repair-all and caller-selected rewrite | Implemented |
| Experimental CLI assurance split | Implemented |
| Valid, invalid, interrupted, and adversarial corpora | Implemented |
| Rust 1.85, 32-bit, and big-endian checks | Passing |

A localhost HTTP Range experiment over an append file containing an unrelated 1 MiB historical object measured:

| Assurance mode | Requests | Bytes transferred | Objects hashed |
|---|---:|---:|---:|
| Targeted lookup | 7 | 16,993 | 1 |
| Full strict validation | 26 | 1,065,673 | 3 |

Targeted lookup did not request the unrelated large payload; full validation did.

## Immutable-page successor implementation status

### Byte and object model

Implemented evidence includes:

- immutable content-addressed pages;
- complete 48-byte object records and payloads;
- domain-separated object, page, snapshot, and commit digests;
- canonical fixed fields and zero padding;
- object/object and object/structural overlap rejection;
- authenticated catalog roots, capabilities, and extension records;
- unknown-required capability handling and byte-preserved unknown optional extensions.

### Persistent tree algorithms

Executable models demonstrate:

- exact historical page reuse and deduplication;
- mixed-age strict traversal;
- deterministic replacement, insertion, and deletion;
- sparse-range insertion routing;
- leaf split, merge, and redistribution;
- root height increase and collapse;
- recursive internal split and recursive underflow/delete;
- exact historical-root reuse after inverse updates;
- deterministic mixed replacements, insertions, and deletions across arbitrary modeled depth;
- a 512-operation differential sequence and a 34-seed, 8,704-operation campaign.

The general mixed planner is executable evidence but is not yet integrated into the reusable Rust byte writer.

### Reusable Rust experiment module

`ucof-experiments::immutable_successor` now exposes reusable synchronous Rust APIs for:

- `build_genesis`;
- `append_replacement`;
- exact-end `validate`;
- slice-based `validate_history`;
- slice-based `scan_recovery_candidates`;
- `rewrite_all`;
- `rewrite_selected`;
- bounded source full validation;
- bounded source linked history;
- bounded source suffix recovery;
- bounded source lookup and authenticated absence.

The module keeps exact-end validity, linked-history validity, report-only recovery, rewrite, and targeted lookup as separate APIs and report types.

The reusable Rust writer reproduces the Python recipe identities:

| Recipe | Bytes | SHA-256 |
|---|---:|---|
| Four-object genesis | 16,886 | `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23` |
| Replacement append | 33,550 | `e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc` |
| 400-object multi-level genesis | 89,316 | `d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70` |

### History and recovery assurance

Both slice and random-access source APIs now exist.

Linked history:

- revalidates every linked strict prefix;
- checks physical footer ordering;
- checks exact sequence decrements;
- checks parent snapshot identity;
- enforces history depth limits;
- carries one cumulative source budget across ancestors;
- rejects ancestor corruption even when the newest active commit remains valid.

Recovery:

- scans only a bounded suffix;
- handles suffixes shorter than footer magic without indexing failure;
- caps attempts and returned candidates independently;
- treats magic as a hint without authority;
- charges successful and failed candidate reads to one cumulative budget;
- returns only exact strictly validated prefixes;
- orders candidates by physical recency;
- never selects an active replacement.

A 400-object, four-page source validates under 4 KiB maximum read requests. An interrupted append reports both complete sequence-1 and sequence-0 prefixes.

### Rewrite and compaction inputs

`rewrite_all` and `rewrite_selected` require a strictly valid active source and publish a new genesis file. Caller-selected rewrite performs no semantic dependency discovery and does not claim semantic compaction. Byte-scoped signatures are not preserved.

Semantic compaction still requires:

- an explicit snapshot-retention policy;
- profile/application dependency semantics for every retained object kind;
- fail-closed or conservative policy for unknown dependency semantics.

### Bounded deterministic writer evidence

The writer experiments provide:

- bounded external sorting of 200,003 locator-shaped records;
- direct canonical page emission from sorted streams;
- descriptor-limited multi-pass merging;
- deterministic identical output across run sizes and fan-in limits;
- private staging and checked byte/inode/descriptor budgets;
- exclusive no-follow creation;
- no-overwrite hard-link publication;
- file and directory synchronization ordering;
- retirement of the staged name after successful durable publication;
- ownership-token and symlink-safe cleanup;
- explicit pre-publication and post-link-indeterminate outcomes.

Production spill encryption, secure deletion, inode exhaustion behavior, hostile filesystem behavior, platform durability guarantees, and portable atomic publication remain unresolved.

## Corpora and continuous testing

Current successor evidence includes:

- a manifest-pinned exact-end genesis vector;
- deterministic append and multi-level generation recipes;
- a compact cryptographically pinned invalid/interrupted recipe corpus;
- valid fork and broken-parent/sequence recipes;
- layer-targeted mutations that recompute outer authentication;
- independent Rust parsing and generation checks;
- jointly satisfiable research support profiles;
- 25 cargo-fuzz targets, including successor strict validation, writer roundtrip, slice history/recovery, source lookup, and source history/recovery.

The current locked matrix passes:

- rustfmt and Clippy with warnings denied;
- all workspace, documentation, integration, and CLI tests;
- Rust 1.85;
- i686 and powerpc64 compilation;
- Candidate 1 Rust/Python corpora and experiments;
- successor vectors, invalid recipes, algorithms, source, metadata, recovery, and spill policy;
- all 25 fuzz builds and bounded smoke campaigns.

Permanent workflows use read-only repository permissions.

## Current limitations and remaining blockers

Before an immutable-page successor can be proposed as an independently implementable epoch, it still needs:

1. an explicit Candidate 2 proposal or other new epoch allocation based on the current non-epoch byte draft;
2. normative identifier width, locator layout, occupancy, split, redistribution, and deletion policies;
3. integration of arbitrary-depth mixed batching into the reusable Rust byte writer;
4. production repair/compaction implementation with profile dependency semantics and preservation policy;
5. compaction vectors and boundary vectors for selected support profiles;
6. concrete HTTP/cloud conditional adapters and asynchronous cancellation tests;
7. production spill confidentiality, durability, cleanup, and portable publication requirements;
8. broader hostile-source and arbitrary-depth selected-implementation fuzzing;
9. independently maintained implementation or external independent review;
10. application freshness policy where rollback resistance is required;
11. maintainer disposition of Candidate 1 and FCP-0002 objections.

## Exit rule

Phase 3 is not complete until a selected experimental layout demonstrates bounded source lookup and validation, append publication, safe page reuse or an explicitly accepted alternative, previous-root recovery, linked history, unambiguous active-root rules, repair, semantic-compaction inputs, cross-language valid and invalid vectors, hostile-input evidence, continuous fuzzing, realistic range-I/O measurements, deterministic large-writer strategy, documented rejected alternatives, independent review, and maintainer disposition of the proposal blockers.
