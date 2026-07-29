# Phase 1 Status — Minimal Wire-Format Experiment

**Status:** In progress  
**Started:** 2026-07-29  
**Experimental epoch:** `UCOF-EXP-0001`  
**Working branch:** `phase-1/minimal-wire-experiment`

## Objective

Create the smallest end-to-end format experiment that can encode, locate, inspect, and verify objects while exposing framing, canonicalization, limit, and footer-discovery assumptions.

## Implemented foundation

| Deliverable | Status | Evidence |
|---|---|---|
| Workspace decision | Complete | `docs/decisions/0002-rust-workspace-and-toolchain.md` |
| First normative proposal | In review | `docs/proposals/0001-exp-0001-framing.md` |
| Experimental wire specification | Draft implemented | `spec/experimental/UCOF-EXP-0001.md` |
| Rust workspace | Implemented on phase branch | `Cargo.toml`, `crates/` |
| Fixed header, records, and footer | Implemented | `ucof-core` |
| Restricted deterministic CBOR | Implemented | `ucof-core::cbor` |
| Deterministic writer | Implemented | `ucof-core::writer` |
| Strict reader and validator | Implemented | `ucof-core::reader` |
| Object lookup | Implemented | validated directory entries |
| Inspector and verifier CLI | Implemented | `ucof-cli` |
| Independent vector generator | Implemented | `tools/generate_exp_0001_vectors.py` |
| Independent second parser | Implemented | `tools/validate_exp_0001.py` |
| Valid and invalid hexadecimal vectors | Initial set implemented | `tests/vectors/exp-0001/` |
| CI format, lint, Rust tests, and independent validation | Passing prototype checks | `.github/workflows/rust.yml` |

## Current experiment decisions

These choices apply only to `UCOF-EXP-0001`:

- 32-byte fixed header;
- little-endian framing integers;
- 40-byte record header;
- 64-bit object identifiers;
- restricted deterministic CBOR metadata;
- exact-end 80-byte footer;
- SHA-256 over all bytes before the footer;
- directory as a checked accelerator rather than authority;
- no padding, transforms, encryption, signatures, external references, or append history.

## Evidence collected

- Rust and Python independently reproduce the valid file structure.
- The Rust deterministic writer matches the independently generated minimal vector byte for byte.
- Both parsers enforce exact-end footer discovery and classify a shortened footer as an invalid shifted footer magic rather than searching backward.
- Payload mutation that preserves framing reaches the digest check and produces `digest_mismatch`.
- Required capabilities fail closed.
- Every byte-boundary truncation of the Rust demo fails without a parser panic.
- Caller file-size limits fail before structural parsing.

## Remaining exit work

Phase 1 is not complete until:

1. Source files are committed in rustfmt-normalized form so CI can restore `cargo fmt --check` rather than formatting the runner workspace.
2. Mutation tests cover every fixed header and footer field.
3. The Python parser receives direct limit and canonical-CBOR adversarial tests beyond the shared vector corpus.
4. Fixed-width and variable-width framing alternatives are measured.
5. Exact-end footer discovery is compared with bounded backward search.
6. Canonical CBOR subset behavior is compared with another established CBOR implementation.
7. Resource limits are stress-tested against UC-02 and UC-10 scales.
8. FCP-0001 receives public review and disposition of objections.
9. The threat model is updated with findings from executable tests.

## Explicit limitations

The current Rust reader validates a complete in-memory byte slice. It demonstrates sequential framing and validated random lookup, but it is not yet a bounded-buffer streaming API over `Read` or a range-source API.

The current digest provides integrity relative to the stored footer, not authenticity.

The experiment has one active root and no recovery from a missing footer. Append-only snapshots and checkpoint recovery belong to Phase 3 after framing experiments establish safe root-selection rules.

## Promotion rule

No byte choice in this epoch becomes stable merely because the implementation works. Promotion requires accepted proposals, hostile-input evidence, cross-language reproduction, and a later stable-version decision.
