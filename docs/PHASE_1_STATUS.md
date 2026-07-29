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
| Fixed-field and canonical-CBOR attacks | Implemented | `tools/test_exp_0001_adversarial.py` |
| Framing-width experiment | Complete | `docs/experiments/0001-framing-widths.md` |
| Footer-discovery experiment | Complete | `docs/experiments/0002-footer-discovery.md` |
| UC-02 scale-limit model | Complete; design fails UC-02 | `docs/experiments/0003-scale-limits.md` |
| Threat-model findings | Companion evidence published | `docs/security/EXP_0001_FINDINGS.md` |
| CI format, lint, Rust, Python, attacks, and experiments | Passing strict checks | `.github/workflows/rust.yml` |

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
- Caller limits fail before oversized file, record, payload, metadata, text, byte-string, depth, and item work.
- Fixed file-header, record-header, and footer fields have targeted mutation coverage in the independent parser.
- The restricted CBOR parser rejects non-shortest arguments, indefinite forms, duplicate or out-of-order map keys, invalid UTF-8, negative integers, and floating-point values.
- A compact variable-width strawman saves 55–70% of record-header bytes, but the whole-file benefit is negligible for page- and media-sized payloads.
- A 64 KiB backward-search tail can expose 8,184 footer-magic candidates, supporting strict exact-end discovery for normal validation.
- One million zero-byte objects require a lower-bound 40 MB of record headers and approximately 52 MB of directory payload. The flat materialized directory therefore fails UC-02 and must not be promoted as a massive-archive design.

## Remaining exit work

Phase 1 is not complete until:

1. The canonical CBOR subset is compared with another established implementation and disagreements are documented.
2. FCP-0001 receives public review and all material objections are dispositioned.
3. The executable findings companion is integrated into the primary threat model.
4. The first framing proposal explicitly records the UC-02 failure and the exact-end versus recovery-mode distinction.
5. Maintainers decide whether the evidence justifies accepting EXP-0001 for continued experimentation, revising it, or retiring it in favor of EXP-0002.

## Explicit limitations

The current Rust reader validates a complete in-memory byte slice. It demonstrates sequential framing and validated random lookup, but it is not yet a bounded-buffer streaming API over `Read` or a range-source API.

The current digest provides integrity relative to the stored footer, not authenticity.

The flat directory and in-memory inventory do not satisfy UC-02-scale object counts. Raising limits does not resolve that architecture problem.

The experiment has one active root and no recovery from a missing footer. Append-only snapshots and checkpoint recovery belong to Phase 3 after framing experiments establish safe root-selection rules.

## Promotion rule

No byte choice in this epoch becomes stable merely because the implementation works. Promotion requires accepted proposals, hostile-input evidence, cross-language reproduction, and a later stable-version decision.
