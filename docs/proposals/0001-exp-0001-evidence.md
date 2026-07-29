# FCP-0001 Evidence Appendix

- **Proposal:** [FCP-0001](0001-exp-0001-framing.md)
- **Status:** Review evidence
- **Experimental epoch:** `UCOF-EXP-0001`
- **Collected:** 2026-07-29

This appendix is part of the FCP-0001 review record. It records executable findings discovered after the initial proposal entered Review. It does not stabilize any byte choice.

## Evidence summary

| Area | Result | Evidence |
|---|---|---|
| Independent reproduction | Rust writer, Rust reader, Python generator, and Python reader agree on the shared corpus | `crates/`, `tools/`, `tests/vectors/exp-0001/` |
| Formatting and lint | Strict rustfmt and clippy checks pass | `.github/workflows/rust.yml` |
| Truncation | Every byte-boundary truncation of the Rust demo is rejected without panic | `crates/ucof-core/tests/exp_0001.rs` |
| Fixed fields | File-header, record-header, and footer fields have targeted mutations | `tools/test_exp_0001_adversarial.py` |
| Resource limits | File, record, payload, metadata, depth, item, text, and byte-string limits are exercised | Rust and Python tests |
| Canonical metadata | Invalid shortestness, indefinite forms, key ordering, duplicate keys, UTF-8, negative integers, and floats are rejected | Python attacks and Rust codec tests |
| CBOR interoperability | Supported primitive encodings match pinned Ciborium 0.2.2 | `docs/experiments/0004-cbor-interoperability.md` |
| Framing width | Compact variable framing saves header bytes but has negligible whole-file impact for large payloads | `docs/experiments/0001-framing-widths.md` |
| Footer discovery | Backward search permits thousands of attacker-controlled candidates in a 64 KiB tail | `docs/experiments/0002-footer-discovery.md` |
| UC-02 scale | Flat records and directory fail the one-million-object lower bound | `docs/experiments/0003-scale-limits.md` |
| Threat analysis | Executable findings and residual risks are recorded | `docs/security/EXP_0001_FINDINGS.md` |

## Decisions supported for EXP-0001

### Fixed-width framing remains experimental

The compact strawman saves 55–70% of record-header bytes in the measured workloads. The whole-file saving is approximately 0.04% for 64 KiB table pages and effectively zero for 64 MiB media payloads.

FCP-0001 may retain its 40-byte fixed header for continued experimentation because it simplifies offset arithmetic, mutation tests, and direct field access. This is not evidence that a stable UCOF header should remain 40 bytes. A hybrid bootstrap and canonical variable extension remains open for a later epoch.

### Strict footer discovery remains exact-end

Normal validity has one candidate footer at the exact end of the file. A reader must not silently fall back to backward search after strict validation fails.

Future recovery work must define separate operations:

1. strict active-root validation;
2. bounded checkpoint discovery with explicit scan and candidate budgets;
3. salvage that reports candidates without promoting them to valid active state.

This distinction is required to prevent recovery convenience from creating root-selection ambiguity or attacker-controlled validation amplification.

### The flat directory fails UC-02

At one million zero-byte logical objects, the current layout needs a lower-bound 40,000,000 bytes of record headers and 51,865,384 bytes of directory payload. At one hundred million objects, lower-bound metadata exceeds 9 GB.

The in-memory reader and flat canonical directory therefore do not satisfy UC-02. Increasing default limits is not an acceptable resolution. Paged or hierarchical directories, range-backed lookup, compact grouping, and bounded index traversal belong in later phases.

Acceptance of FCP-0001 would accept a disposable experiment, not claim that this directory design is suitable for massive archives.

## Validation-order finding

Error categories depend on the earliest violated layer. A changed framing length is rejected structurally before digest comparison. A changed payload byte that preserves framing reaches `digest_mismatch`. A changed record identifier with a recomputed experimental digest reaches `directory_mismatch`.

Conformance tests must target a declared validation layer rather than require every mutation of committed bytes to produce the same error.

## Acceptance checklist update

Completed technical evidence:

- reference workspace builds and tests on stable Rust;
- strict formatting and lint checks pass;
- valid vectors are deterministic;
- independent Python writer and reader reproduce structure;
- malformed vectors and fixed-field attacks produce categorized errors;
- truncations fail safely;
- required capabilities fail closed;
- resource limits are exercised;
- established CBOR output is compared;
- framing, footer, and scale alternatives are measured.

Still required before proposal disposition:

- public review period completion;
- disposition of material interoperability, security, and compatibility objections;
- integration of the findings into the primary threat model;
- maintainer decision to accept, revise, defer, reject, or supersede FCP-0001.
