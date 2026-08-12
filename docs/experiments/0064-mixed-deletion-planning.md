# Experiment 0064 — Order-independent mixed deletion planning

**Status:** identifier-level planning model, not byte emission  
**Date:** 2026-07-31

## Question

Can a batch combining deletion, insertion, and replacement derive deterministic leaf occupancy repairs from the complete operation set rather than from caller order or intermediate trees?

## Model

`plan_mixed_leaf_updates` accepts strictly ordered original leaf identifiers and a complete identifier-only operation batch. It:

1. validates original capacity, minimum occupancy, global ordering, uniqueness, and configured limits;
2. canonicalizes operations by identifier and rejects duplicate operation identifiers;
3. routes every operation against the original authenticated leaf ranges;
4. classifies `Put` as insertion or replacement from the original active set;
5. applies every operation simultaneously within its original target leaf;
6. splits overfull leaves left-to-right using canonical final-two-page grouping;
7. repairs underfull leaves left-to-right using left borrow, right borrow, left merge, then right merge;
8. reports final leaf identifiers, original pages touched, repair actions, and operation counts.

The model tracks original-page provenance through split and merge actions so later byte-writer work can identify the minimum prior page set requiring replacement.

## Evidence

Unit tests cover:

- deletion and insertion in one original leaf producing no intermediate underflow repair;
- caller-order invariance;
- an insertion in the right sibling enabling deterministic right borrow;
- simultaneous overflow in one original leaf and underflow in another, followed by split and merge;
- replacement accounting without occupancy change;
- duplicate operation, missing deletion target, and final-object deletion rejection.

## Policy boundary

Every operation is routed against the original authenticated page ranges. This avoids order-dependent range changes while planning. A future byte emitter must use the same rule or amend the proposal and vectors explicitly.

The model covers leaf occupancy only. It does not emit locators or pages, plan internal-page underflow, preserve payloads, publish a commit, or prove exact page reuse. Those remain the next integration steps.

## Result interpretation

Passing this experiment removes ambiguity from simultaneous leaf repair and supplies deterministic actions for the remaining mixed persistent writer frontier. It does not claim that deletion-plus-other-operation batches have left the full-rebuild fallback in the reusable byte writer.
