# Experiment 0037: Independently Parsed Immutable Successor Vector

- **Status:** Reproducible cross-implementation evidence
- **Date:** 2026-07-30
- **Related:** Experiments 0015, 0031, and 0033
- **Vector:** `tests/vectors/exp-0002-immutable/genesis-four.hex`
- **Manifest:** `tests/vectors/exp-0002-immutable/manifest.json`
- **Independent parser:** `crates/ucof-experiments/tests/exp0002_immutable_vector.rs`

## Question

Can the immutable-page successor microformat produce one exact-end file that is generated and validated by the Python model, then independently parsed and hashed from raw bytes by Rust without calling the Python validator or the Candidate 1 Rust codec?

## Pinned vector

The stored file is a genesis publication containing four complete object records with identifiers 1 through 4 and payloads `alpha`, `bravo`, `charlie`, and `delta`.

The manifest pins:

- decoded length: **16,886 bytes**;
- SHA-256: `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23`;
- sequence: 0;
- exact-end footer placement;
- the four object identifiers and payloads.

The vector is non-normative successor evidence. It does not allocate a Candidate 2 epoch or stabilize the microformat.

## Independent Rust checks

The Rust test manually:

1. decodes the stored hexadecimal bytes;
2. validates the bootstrap header and zero-reserved bytes;
3. parses the exact-end footer and snapshot fields;
4. recomputes domain-separated snapshot and commit digests;
5. recursively parses immutable leaf and internal pages;
6. validates page digests, levels, ranges, ordering, entry widths, and zero padding;
7. parses all 48-byte object headers and payloads;
8. recomputes every object digest;
9. cross-checks object identifiers, kinds, logical lengths, and locators;
10. rejects object/object and object/structural physical overlap;
11. verifies the exact four payloads.

It does not call the Python code and does not reuse the existing Rust EXP-0002 Candidate 1 parser.

## Fixture failure found during the experiment

The first stored fixture was malformed: it decoded to 189,698 bytes, contained no footer magic, and ended in zero bytes. A read-only inspection step exposed the absence of any exact-end footer. The fixture was regenerated from the executable Python writer, strictly validated before commit, and pinned by manifest.

This demonstrates why a corpus needs independent structural verification rather than only a checked-in filename or generator claim.

## Findings

1. The successor microformat is independently parseable at the fixed-field and digest layers.
2. Exact-end publication, canonical page bytes, and complete object records agree across Python generation and independent Rust parsing for this vector.
3. A manifest must pin decoded bytes and cryptographic identity; a textual hex file alone is insufficient evidence.
4. Independent parsing found a malformed generated fixture before the result was promoted.
5. One four-object genesis vector is not an independently maintained implementation, a multi-level corpus, or a normative compatibility suite.

## Continuous verification

The read-only `Verify Immutable Successor Vector` workflow enforces:

- manifest status and shape;
- exact decoded length and SHA-256;
- one footer magic at exactly `file_len - 128`;
- zero trailing bytes;
- workspace rustfmt;
- clippy with warnings denied;
- the independent Rust parse and hash test.
