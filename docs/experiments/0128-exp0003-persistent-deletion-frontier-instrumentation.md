# Experiment 0128 — Persistent deletion frontier instrumentation

## Question

Do the minimum-frontier and donor-cliff mechanisms measured in the leaf-only EXP-0003 stochastic model appear directly in the real persistent Rust deletion workloads that produced the Experiment 0123 page-write advantage?

## Method

This experiment adds a non-normative, read-only inspector for the active immutable-successor tree. For a requested deletion it strictly validates the current state, locates the target leaf and immediate siblings, and reports the exact pre-deletion state consumed by the experimental borrower rule:

- target, left-sibling, and right-sibling occupancies;
- whether deleting the target reaches the non-root minimum-occupancy frontier;
- the borrower policy's selected donor and donor occupancy;
- whether the selected donor is exactly `M+1`, so lending creates a new minimum leaf;
- whether that `M+1` donor choice had another eligible sibling that was strictly fuller;
- whether the repair would merge;
- active leaf count and minimum-leaf count.

The inspector calls the same borrower-selection helper as the persistent writer, but does not produce or modify successor bytes.

The existing five-workload deletion-policy trace matrix invokes the inspector immediately before every persistent deletion under both `LeftFirst` and `FullerSiblingLeftTie`. These fixtures remain at root level 1, so the matrix can assert the immediate reward map exactly:

```text
borrow selected  -> target + donor + root = 3 pages written
no donor selected -> 2 pages written
```

The aggregate borrow count is also asserted equal to the delete-only 3-page histogram count. Fuller-sibling additionally asserts zero donor-cliff choices when a strictly fuller eligible sibling exists.

## CI-reproduced result

The final semantic run covered 48 delete/reinsert cycles for each of five workloads, or 240 deletions per policy.

| Workload | Policy | Underflows | Borrows | Merges | `M+1` donor cliffs | Avoidable donor cliffs | Sum pre-delete `n_M` |
|---|---|---:|---:|---:|---:|---:|---:|
| whole-set-lcg | left-first | 20 | 18 | 2 | 17 | 8 | 51 |
| whole-set-lcg | fuller-sibling | 3 | 3 | 0 | 0 | 0 | 3 |
| left-leaf-hot | left-first | 0 | 0 | 0 | 0 | 0 | 48 |
| left-leaf-hot | fuller-sibling | 0 | 0 | 0 | 0 | 0 | 48 |
| middle-leaf-hot | left-first | 1 | 1 | 0 | 1 | 1 | 48 |
| middle-leaf-hot | fuller-sibling | 1 | 1 | 0 | 0 | 0 | 1 |
| right-leaf-hot | left-first | 0 | 0 | 0 | 0 | 0 | 48 |
| right-leaf-hot | fuller-sibling | 0 | 0 | 0 | 0 | 0 | 48 |
| left-middle-boundary-hot | left-first | 23 | 23 | 0 | 23 | 12 | 48 |
| left-middle-boundary-hot | fuller-sibling | 2 | 2 | 0 | 0 | 0 | 2 |

Aggregate over all five traces:

```text
                              LeftFirst   FullerSiblingLeftTie
observed deletions                  240                      240
underflow frontiers                  44                        6
borrow frontiers                     42                        6
merge frontiers                       2                        0
M+1 donor-cliff borrows              41                        0
avoidable donor-cliff borrows        21                        0
sum pre-delete n_M                  243                      102
sum active leaf count               720                      720
mean pre-delete n_M              1.0125                   0.4250
```

Thus 41 of 42 left-first borrow repairs drain a barely eligible `M+1` donor down to `M`. In 21 of those 41 cases, the other eligible sibling is strictly fuller, so the donor-cliff creation is locally avoidable by the fuller-sibling rule. Fuller-sibling records no donor-cliff borrow in this deterministic matrix.

The observed underflow-frontier mass also separates strongly: mean pre-delete `n_M` is `243/240 = 1.0125` for left-first versus `102/240 = 0.4250` for fuller-sibling, a 58.0% reduction in this small persistent workload ensemble.

## Relation to Experiment 0123

Experiment 0123 reported 44 versus 6 three-page transitions over 480 total operations per policy. The persistent frontier inspector refines that number by operation type:

- left-first has 42 three-page deletion borrows plus 2 three-page insertion splits;
- fuller-sibling has 6 three-page deletion borrows and no three-page insertion split in this matrix.

So the deletion component of the reward difference is now causally tied to the inspected repair frontier rather than inferred from aggregate page counts.

The implementation evidence supports the same chain developed in Experiments 0124–0127:

```text
borrower choice
    -> donor-cliff creation or avoidance
    -> minimum-leaf mass n_M
    -> later underflow-frontier visitation
    -> persistent page-write reward
```

This experiment does not claim that all of the long-run policy advantage is explained by one-step donor-cliff avoidance. It shows that the proposed mechanism is present directly in the persistent implementation and that the expensive deletion reward class is exactly the inspected borrow frontier for these depth-1 fixtures.

## Validation

The instrumentation head passed:

- stable Rust formatting, clippy, unit/integration/doc tests;
- MSRV 1.85;
- i686 and powerpc64 portability checks;
- immutable-successor vector verification;
- Phase 3 integration and evidence workflows;
- fuzz workflow;
- the complete EXP-0003 experiment block, including Experiments 0124–0127;
- the existing Experiment 0123 reward decomposition unchanged.

## Boundary

Research instrumentation only. No deletion bytes, default `LeftFirst` behavior, FCP-0003 disposition, page geometry, authoritative vectors, epoch allocation, or wire-format status are changed by this experiment.
