# Experiment 0039: Cross-Language Immutable Successor Generation Recipes

- **Status:** Executable cross-language generation evidence
- **Date:** 2026-07-30
- **Recipes:** `tests/vectors/exp-0002-immutable/generated-recipes.json`
- **Python verifier:** `tools/verify_exp0002_immutable_generated_recipes.py`
- **Rust generator:** `crates/ucof-experiments/tests/exp0002_immutable_generated_vectors.rs`
- **Byte draft:** `docs/spec/IMMUTABLE_SUCCESSOR_MICROFORMAT.md`

## Question

Can independent Python and Rust writers reproduce exact immutable-page bytes for genesis, append, and a multi-level tree without storing large duplicate hexadecimal files?

## Recipe representation

The compact contract pins generation inputs, structural facts, decoded lengths, and SHA-256 identities. Implementations generate the exact file in memory and compare the resulting identity.

The recipes are non-normative. They do not allocate Candidate 2 or make the successor microformat durable.

## Base vector

The four-object exact-end genesis remains stored as full bytes:

- decoded length: 16,886;
- SHA-256: `94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23`;
- one leaf root;
- payloads `alpha`, `bravo`, `charlie`, and `delta`.

The Rust generator must reproduce the stored bytes exactly, not only the same hash.

## Replacement append recipe

The append recipe:

- starts from the pinned four-object genesis;
- appends object identifier 1, kind 9, payload `alpha-v2`;
- keeps identifiers 2–4 at historical offsets;
- emits one new immutable leaf;
- publishes sequence 1 linked to the sequence-zero footer.

Pinned result:

- decoded length: 33,550;
- SHA-256: `e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc`;
- root level: 0;
- current page count: 1;
- active object count: 4.

## Multi-level recipe

The multi-level recipe generates identifiers 1 through 400 with:

```text
kind = 1 + object_id % 5
payload = UTF-8("payload:" || decimal(object_id))
```

Pinned result:

- decoded length: 89,316;
- SHA-256: `d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70`;
- sequence: 0;
- root level: 1;
- three leaf pages and one internal root;
- active object count: 400.

## Independence

The Rust generator writes fields directly with little-endian byte operations and computes the domain-separated digests itself. It does not call the Python writer, Candidate 1 Rust codec, or the independent Rust parser from Experiment 0037.

The Python verifier calls the executable successor model and strictly validates each generated file before comparing the pinned facts.

Agreement therefore covers:

- object header bytes and object digests;
- leaf and internal page construction;
- page packing and padding;
- snapshot bytes and parent linkage;
- commit preimage boundaries and footer semantics;
- exact-end publication;
- deterministic output.

## Findings

1. Compact generation recipes can provide exact cross-language byte evidence without storing page-padding duplicates.
2. One stored base vector remains valuable because it detects shared generator drift and validates the recipe bootstrap.
3. The append identity proves parent-linked publication and historical object reuse.
4. The 400-object identity proves independent agreement on a multi-level directory.
5. Recipe identities are not a substitute for stored invalid, recovery, fork, and compaction corpora.
6. A later candidate should publish both compact recipes and selected full byte fixtures for independent implementations.
