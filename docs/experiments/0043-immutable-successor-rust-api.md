# Experiment 0043 — Reusable Rust Immutable-Successor API

- **Status:** Passed as non-normative successor evidence
- **Date:** 2026-07-30
- **Epoch impact:** None; this does not allocate Candidate 2

## Question

Can the independently parsed immutable-page successor vector be promoted into one reusable Rust experiment module that preserves exact byte identity while keeping current validity, linked history, and recovery as distinct bounded assurance scopes?

## Implementation

`crates/ucof-experiments::immutable_successor` now provides:

- deterministic canonical genesis generation;
- deterministic replacement append;
- exact-end strict slice validation;
- page, object, structural-range, canonical-padding, and current-page-count checks;
- linked-history validation that independently revalidates every prefix;
- bounded suffix recovery that reports strictly valid prefixes without selecting one;
- independent limits for file size, objects, pages, depth, allocation, output, history entries, recovery scan bytes, footer attempts, and returned candidates.

The writer canonicalizes object input order and rejects duplicate or zero identifiers and invalid kinds. The strict reader never invokes recovery.

## Exact byte results

The reusable Rust writer reproduces the already proven successor identities exactly:

| Case | Length | SHA-256 |
|---|---:|---|
| Four-object genesis | 16,886 | `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23` |
| Replacement append | 33,550 | `e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc` |
| 400-object multi-level genesis | 89,316 | `d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70` |

The genesis bytes are identical to the manifest-pinned Python-generated vector.

## Assurance findings

- A forged footer current-page count is rejected even when the outer commit digest is recomputed.
- Partially overlapping directory pages are rejected rather than only exact duplicate offsets.
- A newest commit can remain exact-end valid while a prior commit's own digest is corrupt; `validate_history` detects this because it revalidates the ancestor prefix independently.
- An interrupted append reports the complete genesis prefix without selecting or activating it.
- Recovery footer attempts and returned candidates are capped independently; zero scan, zero attempt, and one-candidate policies are explicit and tested.

## Fuzzing

The branch now has three immutable-successor fuzz targets:

1. arbitrary raw bytes through strict validation;
2. bounded canonical genesis and replacement-append roundtrips;
3. generated two-commit history, suffix recovery, interrupted publication, and ancestor-digest corruption.

The total cargo-fuzz matrix is 24 targets.

## Limitations

- The reusable API is slice-based and synchronous.
- History revalidation may repeat work across prefixes.
- The append API currently supports replacement, not a general arbitrary-depth mixed-operation batch.
- No production random-access/conditional remote source adapter, repair library, streaming writer, or spill integration is provided by this module.
- The byte microformat remains experimental, incomplete as a specification, and without a compatibility promise.

## Conclusion

The experiment passes. The successor now has a reusable Rust writer/strict-reader/history/recovery core with pinned byte identity and bounded assurance behavior. This closes the blocker that successor parsing and validation existed only as Python models plus one one-off Rust parser, but it does not close production source, repair, mixed-operation, profile, corpus, or independent-review blockers.
