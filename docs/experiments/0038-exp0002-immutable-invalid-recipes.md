# Experiment 0038: Immutable Successor Invalid and Interrupted Recipes

- **Status:** Reproducible compact invalid corpus
- **Date:** 2026-07-30
- **Base vector:** `tests/vectors/exp-0002-immutable/genesis-four.hex`
- **Contract:** `tests/vectors/exp-0002-immutable-invalid/cases.json`
- **Materializer:** `tools/verify_exp0002_immutable_invalid_recipes.py`

## Question

Can a successor invalid corpus remain deterministic and independently reviewable without storing thirteen almost-identical 16 KiB page images?

## Representation

The corpus stores:

- the exact base-vector SHA-256;
- thirteen unique case names;
- one semantic mutation operation per case;
- one expected coarse rejection layer per case.

The materializer derives all offsets from the strictly validated base file. For cases intended to reach inner canonical or physical-layout checks, it recomputes the enclosing page, snapshot, and commit authentication as needed.

The generated malformed byte strings are not written back to the repository. Their deterministic SHA-256 values are emitted by CI, while the compact recipes and executable materializer are the pinned corpus representation.

## Cases

The contract covers:

1. bootstrap magic;
2. footer reserved bytes;
3. commit digest mismatch;
4. object-header reserved bytes with a valid outer commit;
5. object-payload digest mismatch with a valid outer commit;
6. leaf ordering with reauthenticated page, snapshot, and commit;
7. non-zero leaf padding with reauthentication;
8. non-zero leaf-header reserved bytes with reauthentication;
9. an authenticated object locator overlapping its directory page;
10. snapshot root digest mismatch with a valid commit;
11. a non-zero genesis parent snapshot identity;
12. strict trailing data;
13. an interrupted half-footer.

## Assurance boundary

The contract pins coarse conceptual layers, not exact implementation exception types, offsets, or diagnostic wording. A later independent implementation may reject an unsafe file earlier, but it must never accept the materialized bytes.

The compact representation does not yet include multi-level internal-page corruption, append-chain forks, recovery ambiguity, or semantic metadata failures. Those require additional base vectors and recipes.

## Reproduction

```console
python3 tools/verify_exp0002_immutable_invalid_recipes.py
```
