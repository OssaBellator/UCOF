# Experiment 0029: Immutable-Page Layer-Targeted Adversarial Cases

- **Status:** Executable prototype corpus
- **Date:** 2026-07-30
- **Related:** Experiments 0015 and 0023–0027
- **Script:** `tools/experiment_exp0002_immutable_page_adversarial.py`

## Question

Do successor immutable-page validators reject malformed inner structure even when outer snapshot and commit hashes are recomputed to authenticate the mutated bytes?

## Fixture

The experiment builds the 100,000-object immutable-page genesis file from Experiment 0015. Its directory has a level-two root, level-one internal pages, and leaf pages.

Each adversarial case asserts one exact validation layer. Where necessary, the script recomputes:

- the mutated leaf digest;
- the level-one child digest;
- the root child and root page digest;
- the snapshot root and snapshot digest;
- the exact-end commit digest.

This prevents a shallow outer-hash failure from hiding malformed inner structure.

## Cases

The executable covers:

- file-header magic;
- commit digest;
- historical leaf page digest;
- leaf key order;
- non-zero leaf padding;
- non-zero page-header reserved bytes;
- forged child digest;
- overlapping internal child ranges;
- out-of-range child locator;
- forged snapshot root digest;
- strict trailing bytes;
- interrupted footer.

## Required distinctions

- A raw historical page mutation with a recomputed current commit must fail at the page digest.
- Reauthenticated unordered leaf entries must fail at leaf ordering.
- Reauthenticated non-zero unused page bytes must fail at padding.
- Reauthenticated child overlap must fail before traversal treats either child as authoritative.
- Reauthenticated out-of-range locators must fail before any page read.
- A snapshot can authenticate a wrong root digest, but traversal must still reject the actual root page.
- Exact-end strict validation must reject trailing and incomplete footer state without invoking recovery.

## Findings

1. Immutable content identity does not replace canonical page parsing or range validation.
2. The current commit digest need not include every historical page byte only because each reachable historical page is independently authenticated and rehashed.
3. Parent, snapshot, and commit reauthentication cannot authorize malformed child structure.
4. Successor invalid vectors must target every validation layer independently.
5. Expected diagnostic layers are useful conformance evidence; implementation-specific exception strings should remain outside the wire specification unless separately standardized.

## Next evidence

A complete successor corpus still needs:

- pinned byte fixtures rather than only generated mutations;
- invalid insertion, split, merge, deletion, and root-collapse outputs;
- repeated-offset and aliasing cases where level/range rules permit construction;
- object and structural overlap mutations;
- extension and capability failures;
- interrupted multi-page update cuts;
- independent implementation agreement;
- continuous fuzzing.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_adversarial.py
```
