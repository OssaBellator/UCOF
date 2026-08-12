# Experiment 0044 — Reusable Rust immutable-successor core

## Status

Executable non-normative successor implementation evidence.

## Question

Can the immutable-page successor microformat be implemented as a reusable bounded Rust API, rather than only as independent tests and Python experiments, while preserving the pinned cross-language byte identities?

## Implementation surface

`crates/ucof-experiments/src/immutable_successor.rs` exposes synchronous experimental APIs for:

- exact-end strict slice validation;
- deterministic genesis construction;
- deterministic object-replacement append;
- explicit file, object, page, depth, allocation, and output limits.

The module remains inside `ucof-experiments`. It does not allocate an epoch, promise compatibility, invoke recovery from strict validation, or claim production readiness.

## Validation properties

The strict validator checks:

- fixed header, object, page, snapshot, and footer structure;
- exact-end publication;
- domain-separated object, page, snapshot, and commit digests;
- canonical page ordering, padding, levels, ranges, and child references;
- object locator agreement, physical overlap, and digest agreement;
- page cycles, duplicate page offsets, and partial page overlap;
- current-commit page-count agreement without counting reused historical pages;
- sequence, previous-footer, and parent-snapshot linkage;
- all configured limits before successful return.

A verified report is returned only after every active object and page has been authenticated.

## Writer evidence

The reusable Rust writer reproduces the established identities exactly:

| Recipe | Bytes | SHA-256 |
|---|---:|---|
| four-object genesis | 16,886 | `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23` |
| replacement append | 33,550 | `e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc` |
| 400-object multi-level genesis | 89,316 | `d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70` |

The four-object genesis is also byte-identical to the checked-in vector. Reversed caller input produces the same canonical genesis bytes.

## Hostile and limit cases

Focused tests require:

- writer output failure under an insufficient output budget;
- validator failure under a zero-page budget;
- rejection of a reauthenticated false current-page-count claim;
- rejection of partially overlapping directory pages;
- no fallback from strict validation into recovery.

## Independence boundary

The reusable module shares a repository with the Python writer and independent Rust parser. Exact byte agreement is strong implementation evidence but is not equivalent to an independently maintained implementation or external review.

## Non-claims

This experiment does not provide a production source adapter, recovery API, verified-history API, repair writer, asynchronous I/O, signatures, authenticity, trusted freshness, or stable format status. Those remain separate frontiers.
