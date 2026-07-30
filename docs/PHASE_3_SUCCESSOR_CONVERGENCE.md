# Phase 3 Immutable-Successor Convergence Record

**Status:** Executable non-epoch evidence  
**Date:** 2026-07-30  
**Related:** `docs/PHASE_3_SUCCESSOR_EVIDENCE.md`, Experiments 0037–0040

## Purpose

This record captures the successor frontiers completed after the main evidence appendix was consolidated. It does not allocate Candidate 2, stabilize any byte value, or amend Candidate 1.

## Independently parsed exact-end vector

The stored `genesis-four` vector is pinned at:

- 16,886 decoded bytes;
- SHA-256 `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23`;
- one exact-end footer and zero trailing bytes;
- four complete objects with payloads `alpha`, `bravo`, `charlie`, and `delta`.

Python generates and strictly validates the file. A separate Rust test parses the raw fields and independently verifies bootstrap, object, page, snapshot, and commit bytes without calling the Python validator or Candidate 1 parser.

The independent gate discovered and replaced an earlier malformed fixture that contained no footer magic.

## Compact invalid and interrupted corpus

`tests/vectors/exp-0002-immutable-invalid/cases.json` pins thirteen deterministic mutation recipes covering:

- bootstrap and footer fields;
- commit identity;
- object header and payload integrity;
- leaf ordering, padding, and reserved bytes;
- authenticated object/page physical overlap;
- snapshot root identity;
- genesis linkage;
- strict trailing data;
- interrupted footer publication.

Every recipe pins its decoded length and SHA-256. The aggregate corpus identity is:

`fa689319d7dd81cc6dc64e4bb7cb932d24961305245f4f6a36b815070cd009bd`

Deep cases recompute outer page, snapshot, and commit authentication so rejection reaches the intended canonicality or physical-layout layer. The contract pins coarse rejection concepts rather than implementation-specific exception types.

## Cross-language generated vectors

Python and independent Rust writers reproduce three deterministic identities:

| Vector | Bytes | SHA-256 | Structure |
|---|---:|---|---|
| Stored four-object genesis | 16,886 | `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23` | Sequence 0, one leaf |
| Replacement append | 33,550 | `e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc` | Sequence 1, one new leaf, historical object reuse |
| 400-object genesis | 89,316 | `d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70` | Three leaves and one level-one root |

The base file is stored in full. The append and multi-level files are compact deterministic recipes to avoid checking in duplicate zero-padded page images.

## Provisional byte draft

`docs/spec/IMMUTABLE_SUCCESSOR_MICROFORMAT.md` now records the complete current research fields and relationships:

- bootstrap, object, page, leaf, internal, snapshot, and footer layouts;
- domain-separated digest preimages;
- genesis and append publication order;
- strict validation order;
- targeted lookup, history, and recovery boundaries;
- catalog placement experiment;
- resource and security requirements;
- exact vector and recipe identities.

The document explicitly has no epoch allocation or compatibility promise. Final identifiers, locator widths, occupancy, split/delete policy, catalog placement, profiles, spill policy, transport APIs, and security extensions remain unresolved.

## Jointly satisfiable support profiles

Experiment 0040 replaces independent maxima with three example tuples whose object, page, request, read-operation, read-byte, hash, allocation, history, and recovery budgets are jointly satisfiable under a conservative validation model.

The profiles are research examples rather than mandatory product tiers. They preserve the distinction between malformed input and a valid file refused by local policy.

## Current verification boundary

The latest source normalization passed:

- rustfmt;
- clippy with warnings denied;
- independent Rust base-vector byte equality;
- exact cross-language append identity;
- exact cross-language multi-level identity.

Permanent workflows remain read-only. Bot-created normalization commits can produce `action_required` follow-on checks, so a normal repository commit is used to obtain the final read-only convergence run.

## Remaining successor work

The largest unresolved frontiers are now:

1. a general arbitrary-depth mixed replacement/insertion/deletion batch planner;
2. reusable Rust successor parser/writer modules rather than integration-test-only implementations;
3. successor fuzz targets for bytes, sources, operations, recovery, and history;
4. fork, recovery, and compaction vector recipes;
5. selected locator, identifier, occupancy, split, and deletion policies;
6. conditional remote-source adapters and asynchronous cancellation;
7. production spill confidentiality, cleanup, and durability;
8. independent external implementation or review;
9. trusted external freshness policy where rollback resistance is required.
