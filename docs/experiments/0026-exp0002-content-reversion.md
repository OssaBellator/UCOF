# Experiment 0026: Historical Root Reuse After an Inverse Update

- **Status:** Prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0014, 0015, 0023, and 0025
- **Script:** `tools/experiment_exp0002_content_reversion.py`

## Question

If an immutable-page update is later reversed, can a new snapshot reference the exact earlier historical root rather than emitting replacement merge pages?

## Fixture

The experiment builds a full level-one tree:

- 255 full leaves;
- 185 entries per leaf;
- 47,175 objects;
- one full internal root.

Inserting identifier `1` splits the first leaf, splits the full internal root, and creates a level-two root. The active inserted tree contains five new structural pages and reuses the other 254 historical leaves.

## Inverse update

Deleting identifier `1` makes the two split leaves' canonical contents equal the original first leaf.

The experiment proves:

1. the merged leaf bytes equal the historical first leaf bytes;
2. the page digest equals the historical leaf digest;
3. replacing the split pair with the historical leaf reconstructs the exact original child-reference list;
4. the reconstructed internal root bytes and digest equal the historical root;
5. the new logical snapshot can therefore point directly to the earlier root offset and digest.

No directory page needs to be emitted for the inverse update when a writer can discover the historical content identity.

## Identity boundaries

Reusing an earlier directory root does not reuse the earlier snapshot or commit identity.

A new publication still has:

- a new sequence;
- a new parent snapshot relationship;
- a new snapshot record and digest;
- a new commit range and digest;
- potentially a new external trust decision.

Only the structural directory root is reused.

## Writer implications

A writer without historical content lookup would ordinarily emit:

- one merged leaf;
- one replacement internal root.

A content-indexed writer can emit zero directory pages and reference the earlier exact root.

This creates a new design frontier:

- whether a writer maintains a bounded digest-to-page locator cache;
- how cache entries are authenticated before reuse;
- whether stale or untrusted cache data can cause incorrect physical references;
- how cache lookup work and retained history are bounded;
- whether profiles permit historical root resurrection after logical deletion and re-addition.

## Security implications

Content identity cannot authorize a page by itself. Before reusing a cached locator, the writer or validator must verify:

- exact page bytes or trusted immutable storage identity;
- page digest;
- canonical page structure;
- physical bounds and non-overlap;
- compatibility with the new parent range and level;
- availability for the intended retention period.

Root reuse does not provide external freshness and must not resurrect objects contrary to the requested logical state.

## Reproduction

```console
python3 tools/experiment_exp0002_content_reversion.py
```
