# Experiment 0071: exact mixed page-reference planning

## Question

Can a simultaneous mixed tree plan distinguish exactly reusable original pages from newly emitted pages before offsets and digests are assigned?

## Planner

The reference planner consumes the order-independent leaf and recursive shape plans.

A leaf is marked reusable only when:

- its complete identifier sequence equals one original leaf; and
- that original leaf was not touched by an insertion, replacement, deletion, borrow, split, or merge.

An internal page is marked reusable only when its complete ordered child identity sequence exactly matches one original page at the same level. A shifted group boundary, new child, changed child, split, merge, or height change therefore creates a new page identity.

The result reports final page identities at every level, exact reusable original page indexes, and new page counts by level.

## Evidence

Pinned cases cover:

- replacement changing one leaf and one ancestor per level while reusing every unaffected page;
- insertion without split reusing unaffected leaf and internal groups;
- a split that shifts group boundaries and invalidates all old internal groups while retaining untouched leaves;
- merge and root collapse producing a new root when the child sequence changes;
- caller-order invariance.

## Boundary

This is exact identity planning relative to the abstract page contents available to the model. It does not carry locator bytes, offsets, lengths, digests, object payloads, or snapshot metadata, and it does not emit or authenticate pages. Byte-writer integration must prove that the modeled identity equality corresponds to byte equality in the reusable implementation.
