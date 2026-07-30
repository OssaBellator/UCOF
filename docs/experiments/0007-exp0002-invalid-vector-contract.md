# Experiment 0007: EXP-0002 Invalid-Vector Contract

- **Status:** Reproducible
- **Date:** 2026-07-30
- **Related:** FCP-0002, Candidate 1 byte specification, concrete security findings
- **Generator:** `tools/exp0002_invalid_vectors.py`
- **Corpus:** `tests/vectors/exp-0002-invalid`

## Question

What part of malformed-file behaviour should be pinned across independent Candidate 1 implementations while the proposal remains Draft?

A single mutation can fail at more than one valid validation layer. For example, a forged child locator may be rejected as a page-range error, a page-digest mismatch, a physical-overlap error, or an invalid object header depending on which outer digests were refreshed and which safe validation order an implementation uses.

Pinning exact exception strings or implementation-local enum variants now would accidentally make one validation order normative and discourage readers from rejecting unsafe structure earlier.

## Contract

Each invalid vector pins:

1. exact bytes;
2. whole-file SHA-256;
3. expected outcome: strict rejection;
4. a coarse intended validation layer;
5. a human-readable mutation description.

The coarse layers are:

- bootstrap;
- object;
- directory page;
- physical layout;
- snapshot;
- parent chain;
- exact end;
- publication.

The layer is diagnostic intent, not an exact public error code. A conforming experimental reader may reject at an earlier safe layer.

## Corpus design

The generated corpus includes:

- non-zero bootstrap reserved bytes;
- object logical-length mismatch under a refreshed commit digest;
- authenticated non-zero leaf padding;
- authenticated overlapping internal child ranges;
- an authenticated leaf locator that points into its directory page;
- non-zero snapshot reserved bytes under refreshed snapshot and commit digests;
- a previous-footer pointer that points forward;
- a child snapshot with the wrong parent snapshot digest;
- valid bytes followed by an unpublished tail;
- a footer missing its final byte;
- deterministic append cuts after an object header, before snapshot completion, and within the footer.

Outer page, snapshot, and commit digests are recomputed where required so the malformed claim reaches the intended deeper validation layer rather than stopping at the first outer hash mismatch.

## Cross-language requirement

The independent Python implementation regenerates every vector, compares it byte-for-byte with the stored corpus, checks manifest lengths and hashes, and rejects it.

The Rust implementation reads the stored corpus independently and must reject every file under strict validation. Rust is not required to reproduce the Python exception text.

## Finding

Candidate 1 can pin a stable invalid outcome and diagnostic layer without stabilizing exact validation-order-specific errors.

Before FCP-0002 enters Review, maintainers must decide whether stable Core error categories are needed and, if so, define categories around security-relevant outcomes rather than parser call order.

## Reproduction

```console
python3 tools/exp0002_invalid_vectors.py --write-vectors tests/vectors/exp-0002-invalid
python3 tools/exp0002_invalid_vectors.py --verify-vectors tests/vectors/exp-0002-invalid
```

The first command is for deterministic corpus maintenance. CI normally runs verification only.
