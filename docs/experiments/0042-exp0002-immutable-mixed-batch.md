# Experiment 0042: Arbitrary-Depth Immutable Mixed-Operation Batch

- **Status:** Reproducible mixed-operation byte experiment
- **Date:** 2026-07-30
- **Script:** `tools/experiment_exp0002_immutable_mixed_batch.py`
- **Related:** Experiments 0015, 0023–0027, and 0031–0032

## Question

Can replacements, insertions, and deletions be canonicalized into one deterministic publication over an arbitrary-depth immutable page tree, while reusing exact historical pages and emitting only pages reachable from the final root?

## Planner

The experiment starts from a canonical 100,000-object tree whose root is at level two. Object identifiers are even so odd identifiers remain available for insertion.

One batch contains:

- three replacements in widely separated key ranges;
- two insertions;
- two deletions;
- shuffled caller order.

The planner:

1. bounds and sorts operations by identifier;
2. rejects duplicate/conflicting operations;
3. validates insert/replace/delete existence rules;
4. appends new object records in canonical identifier order;
5. computes the final sorted locator set;
6. constructs canonical leaves and internal levels bottom-up;
7. reuses any exact old page with matching level, range, digest, and bytes;
8. appends only new pages reachable from the final root;
9. publishes one new snapshot and footer;
10. strictly validates the result.

The algorithm works at arbitrary tree depth because every internal level is built until one root remains.

## Determinism

The same operation set is executed in original and deterministic shuffled order. The complete resulting byte stream, root identity, page accounting, and object inventory must be identical.

A no-op publication reuses the exact historical root and every page while publishing a new snapshot/commit identity.

## Work limits

The experiment independently caps:

- operation count;
- cumulative new object-record bytes;
- newly emitted pages;
- final output bytes.

Invalid insert-existing, delete-missing, and same-identifier conflict cases fail closed.

## Important trade-off

Exact content reuse naturally shares unaffected pages. However, canonical bulk packing means insertion or deletion can shift later leaf boundaries and cause wider repacking than replacement-only batches.

This is a correct deterministic baseline, not proof of minimal write amplification. A future production planner may use localized split/merge rules while preserving the same final canonicality and operation-order independence.

## Findings

1. Mixed operation types can be resolved before one immutable publication.
2. Caller operation order need not influence output bytes.
3. Exact old-page reuse can be selected by authenticated content identity.
4. Only final reachable pages are emitted; transient intermediate roots are absent.
5. No-op publication can reuse the exact entire directory.
6. Insert/delete packing policy materially affects write amplification and must be selected normatively.
7. Arbitrary-depth mixed-operation fuzzing remains required after this deterministic experiment.
