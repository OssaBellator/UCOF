# Experiment 0046 — Reusable Rust Immutable-Successor API

**Status:** Executable non-normative evidence  
**Date:** 2026-07-30  
**Epoch allocation:** None

## Question

Can the immutable-page successor microformat move from test-local byte construction into a reusable production-language experiment without collapsing strict validity, historical verification, recovery, rewrite, and source lookup into one assurance claim?

## Implemented surface

`ucof-experiments::immutable_successor` now exposes reusable synchronous Rust APIs for:

- deterministic genesis construction;
- deterministic replacement append;
- exact-end strict slice validation;
- verified linked-history traversal;
- bounded suffix recovery that reports candidates without selecting one;
- verified-source rewrite of all active objects;
- verified-source caller-selected rewrite without semantic dependency discovery;
- bounded random-access full validation;
- bounded authenticated object lookup and absence.

The implementation uses explicit file, object, page, depth, allocation, output, history, recovery-scan, recovery-attempt, and recovery-result limits.

## Cross-language byte evidence

The reusable Rust writer reproduces the independently pinned Python recipe identities:

| Recipe | Bytes | SHA-256 |
|---|---:|---|
| Four-object genesis | 16,886 | `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23` |
| Replacement append | 33,550 | `e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc` |
| 400-object multi-level genesis | 89,316 | `d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70` |

Input order is canonicalized before publication. A reauthenticated false `page_count_current` claim and partially overlapping page ranges are rejected.

## Assurance separation

### Exact-end validation

`validate` accepts only the footer at the exact physical end and never invokes recovery.

### Verified history

`validate_history` revalidates every linked prefix and checks:

- strictly decreasing physical footer offsets;
- exact sequence decrements;
- parent snapshot digest agreement;
- chain depth limits;
- ancestor commit integrity.

A valid newest commit cannot hide a corrupt ancestor when the caller requests verified history.

### Recovery

`scan_recovery_candidates`:

- scans only a caller-bounded suffix;
- caps footer attempts and returned candidates independently;
- treats footer magic as a hint with no authority;
- returns only exact strictly validated prefixes;
- orders results by physical recency;
- never chooses an active replacement.

### Rewrite

`rewrite_all` and `rewrite_selected` accept only a strictly validated active source and publish a new genesis file. Caller-selected rewrite performs no semantic dependency discovery. Byte-scoped signatures are explicitly reported as not preserved.

### Source lookup

The random-access source path separates full active-state validation from targeted authenticated lookup. Targeted lookup authenticates one root-to-leaf path and the selected object or absence result; it does not claim unrelated objects were rehashed.

## Continuous evidence

The reusable API is covered by:

- exact pinned-vector tests;
- malformed page-count, overlap, limit, history, and recovery tests;
- Rust 1.85 and portability compilation;
- raw strict-validation fuzzing;
- deterministic genesis/append roundtrip fuzzing;
- linked-history and recovery fuzzing.

The repository fuzz matrix now contains 24 targets.

## Findings

1. The microformat is no longer Python-only or test-local.
2. Strict validation, history, recovery, rewrite, and targeted lookup remain distinct API types and calls.
3. Production-language evidence does not allocate Candidate 2 or make compatibility promises.
4. The current source API remains synchronous.
5. Repair/rewrite currently materializes selected payloads and output in memory.
6. No implementation-local API establishes authenticity or external freshness.

## Remaining work

- integrate arbitrary-depth mixed insert/delete batching into the reusable Rust writer;
- add source-based history and recovery without whole-file materialization;
- select normative identifier, locator, occupancy, split, and deletion policies;
- add independently maintained implementation or external review;
- define production spill confidentiality, durability, and portable publication requirements.
