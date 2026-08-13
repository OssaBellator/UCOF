# Experiment 0129 — Recursive deletion repair frontier

## Question

Does the persistent deletion frontier mechanism from Experiment 0128 remain mechanically interpretable when a leaf repair changes parent occupancy and triggers a second repair one level higher?

## Method

This experiment adds a non-normative, read-only bottom-up repair-path inspector for the immutable-successor persistent deletion experiment.

For every non-root node on the target path, the inspector records:

- tree level;
- target occupancy before the local change;
- target occupancy after deleting the object or removing one merged child;
- immediate left/right sibling occupancies at the same level;
- whether the node underflows;
- the borrower policy's selected donor and donor occupancy;
- whether the selected donor is exactly `minimum + 1`;
- whether a strictly fuller eligible alternative exists;
- whether the node merges and therefore removes one child from its parent.

The path is evaluated bottom-up. A higher level is reached only when the lower level merges. Each repair classification uses the same `deletion_minimum` and `choose_deletion_borrow_side` helpers as the real persistent writer. Root occupancy retains the writer's root exception.

The inspector does not materialize pages or produce successor bytes.

## Level-2 fixture

The experiment deliberately reuses the existing persistent-deletion integration fixture rather than a synthetic occupancy-only model.

Current research geometry:

```text
LEAF_CAPACITY             = 185
LEAF_MIN_OCCUPANCY        = 93
INTERNAL_FANOUT           = 255
INTERNAL_MIN_OCCUPANCY    = 128
object count              = 47,361
root level                = 2
```

Canonical construction gives two level-1 children containing 129 and 128 leaf children. The final two leaves are both at the leaf minimum.

Deleting object `47,361` therefore has the following inspected path.

### Level 0: leaf repair

```text
target before        = 93
target after delete  = 92
left sibling         = 93
right sibling        = none
underflow            = yes
eligible donor       = none
repair               = merge
```

The leaf merge removes one child from its level-1 parent.

### Level 1: internal repair

```text
target before        = 128
target after removal = 127
left sibling         = 129
right sibling        = none
underflow            = yes
selected donor       = left
selected donor size  = 129
repair               = borrow
```

Because `129 = INTERNAL_MIN_OCCUPANCY + 1`, this is an **internal donor-cliff borrow**: the donor becomes exactly the internal minimum after lending.

There is no second eligible internal sibling, so this donor-cliff event is unavoidable in the fixture. `LeftFirst` and `FullerSiblingLeftTie` therefore make the same choice.

## Writer cross-check

For both experimental borrower policies, the same fixture is then executed through the real persistent writer. The test requires:

```text
root level after deletion  = 2
objects after deletion     = 47,360
page count delta           = -1
pages written              = 4
pages reused               = original page count - 5
canonical validation       = pass
left/full policy bytes     = identical
```

The 4 written pages correspond to the recursively repaired path already pinned by the pre-existing persistent-deletion test. The new experiment does not introduce that reward as a fitted or newly chosen golden value; it requires the read-only repair path to agree with the writer behavior that was already under test.

## Instrumentation request contract

Experiment 0129 also closes a small instrumentation boundary discovered while reviewing the inspectors. The persistent writer rejects deletion of the last remaining object with `invalid batch result`; both read-only deletion inspectors now reject that request identically instead of describing an unexecutable transition.

A non-final root-leaf deletion remains a valid one-level path and is classified as non-underflowing because the root is exempt from the non-root occupancy floor.

## Interpretation

Experiment 0128 established the leaf-level persistent mechanism:

```text
borrower choice
    -> donor-cliff creation/avoidance
    -> minimum-leaf mass
    -> later leaf-underflow visitation
    -> page-write reward
```

Experiment 0129 shows that the same repair vocabulary extends one level upward. A lower-level merge can create an internal underflow frontier, and the internal repair can itself land on an `M+1` donor cliff.

That matters because a future multi-level policy comparison cannot treat leaf occupancy as the only persistent state variable. Internal minimum mass and internal donor-cliff flow are potential renewal/frontier variables whenever leaf merges propagate upward.

## What this experiment does not show

This fixture does **not** establish an internal-level policy advantage. Only one internal sibling can lend, so both policies necessarily choose it. The next discriminating test must construct or discover a multi-level state in which an underflowed internal node has two eligible siblings of unequal occupancy, allowing `LeftFirst` and `FullerSiblingLeftTie` to choose different internal donors.

Nor does this experiment establish a closed multi-level Markov chain, stationary internal occupancy distribution, or normative EXP-0003 deletion policy.

## Boundary

Research instrumentation only. No persistent writer bytes, default `LeftFirst` behavior, FCP-0003 disposition, page geometry, authoritative vector identity, epoch allocation, or wire-format status changes in this experiment.
