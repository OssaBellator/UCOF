# Experiment 0024: Immutable-Page Operation Sequences

- **Status:** Differential prototype
- **Date:** 2026-07-30
- **Related:** Experiments 0015 and 0023
- **Script:** `tools/experiment_exp0002_immutable_page_sequences.py`

## Question

Do immutable append-only split and merge operations remain deterministic and structurally correct across a mixed sequence rather than only isolated inverse examples?

## Oracle

The experiment maintains two representations:

- the byte-level immutable-page tree from Experiments 0015 and 0023;
- an ordinary in-memory sorted set of object identifiers.

After every operation, strict tree traversal must produce exactly the sorted-set contents, root range, and ordering.

## Sequence

A fixed-seed generator performs 512 insertions and deletions while keeping the fixture between 100 and 300 objects. This bound keeps the current prototype at leaf or height-one root level, allowing the test to focus on:

- leaf replacement;
- leaf split;
- sibling merge and redistribution;
- root-height increase and collapse;
- exact reuse of unaffected pages;
- append-only growth;
- deterministic key routing through gaps between child min/max ranges.

The identical seed is executed twice and must produce equal final bytes, root reference, identifiers, and work counts.

## Canonical gap routing

Child ranges authenticate the identifiers currently present in each child. Sparse identifiers can leave numeric gaps between adjacent child maximum and minimum values.

Insertion must not reject those gaps as unowned. The prototype routes a new identifier to the first child whose current maximum is greater than or equal to the identifier, or to the final child when the identifier exceeds every maximum. This is equivalent to a deterministic upper-bound separator rule.

A successor specification must define the routing rule explicitly; child min/max validation alone is insufficient to define insertion.

## Bounded work

For the constrained height-one fixture, each operation may emit at most:

- one changed leaf plus one root for a non-splitting update;
- two split or redistributed leaves plus one root;
- one merged leaf when the root collapses.

The executable records total new pages, reused-page observations, maximum pages emitted by one operation, and root-height transitions.

## Findings

1. Isolated split and merge examples are insufficient; deterministic operation-sequence replay catches routing and state-transition defects.
2. Sparse child ranges require a canonical insertion-routing rule.
3. The byte tree must agree with an independent logical oracle after every operation.
4. Append-only page identity permits old and new roots to coexist, but each active root must still validate exact ordering, ranges, levels, and digests.
5. A complete successor needs property tests and fuzzing over arbitrary batched operations and deeper internal levels.

## Limitations

The experiment intentionally constrains the tree to at most one internal level. It does not yet cover:

- recursive internal splits or merges;
- one transaction containing multiple operation kinds;
- objects, snapshots, footers, recovery, or source readers;
- resource exhaustion and cancellation;
- cross-language operation sequences.

## Reproduction

```console
python3 tools/experiment_exp0002_immutable_page_sequences.py
```
