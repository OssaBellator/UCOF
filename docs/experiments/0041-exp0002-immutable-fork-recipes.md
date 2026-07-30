# Experiment 0041: Immutable Successor Fork and Recovery Recipes

- **Status:** Reproducible fork and interrupted-publication evidence
- **Date:** 2026-07-30
- **Script:** `tools/experiment_exp0002_immutable_fork_recipes.py`
- **Related:** Experiments 0032, 0035, and 0039

## Question

Can one physical append-only byte stream contain two independently valid sequence-one children of the same genesis, while recovery enumerates both terminals without selecting one?

## Construction

The experiment builds:

1. a four-object sequence-zero genesis;
2. child A, which replaces object 1 with payload `alpha-v2`;
3. child B, appended physically after child A but linked directly to the same sequence-zero parent, replacing object 2 with payload `bravo-fork`.

Child B's commit digest covers all bytes appended since the genesis footer, including the physically earlier child-A commit. Its authenticated snapshot and directory reference the genesis objects plus child B's new object. The exact-end file therefore publishes child B, while the prefix ending at child A remains a separate valid terminal.

The two sequence-one snapshots have distinct snapshot and commit identities and the same verified genesis parent.

## Recovery behavior

A bounded backward scan reports valid prefixes in physical-recency order:

```text
sequence 1 — child B
sequence 1 — child A
sequence 0 — genesis
```

The report exposes both equal-sequence terminals and their distinct identities. It does not choose one as preferred.

Verified history starting from either sequence-one prefix produces `(1, 0)` and revalidates the shared genesis independently.

## Interrupted latest publication

Removing half of child B's footer causes exact-end strict validation to fail. Recovery then reports only child A and genesis. It does not partially accept or reconstruct child B.

## Invalid-link recipes

The experiment reauthenticates outer bytes after two semantic mutations:

- a final snapshot whose parent snapshot digest does not match genesis;
- a final snapshot/footer sequence of 2 pointing directly to sequence-zero genesis.

Both fail at parent linkage rather than at a shallow digest mismatch.

## Findings

1. Physical append order and parent-chain order are distinct concepts.
2. Multiple valid equal-sequence terminals can coexist in one byte stream.
3. Exact-end strict mode has one active publication: the final complete footer.
4. Recovery enumeration must expose fork ambiguity and identities without imposing a selector.
5. Verified history is evaluated separately for each candidate prefix.
6. Interrupted newest publication preserves an older complete sibling prefix.
7. Parent digest and sequence increments remain mandatory even when all enclosing hashes are valid.

## Limitations

The experiment uses two one-level children and one shared genesis. It does not define application fork policy, external freshness, multi-writer coordination, or trusted conflict resolution.
