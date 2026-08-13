# Experiment 0130 — Internal donor-cliff policy divergence

## Question

Can `LeftFirst` and `FullerSiblingLeftTie` diverge at an **internal** deletion frontier in the real persistent writer, and if so, is that divergence an immediate page-write reward difference or a next-state occupancy difference?

## Why a constructed fixture is needed

A fresh canonical level-2 build places the sparsest internal group at an edge. The recursive fixture in Experiment 0129 therefore had only one eligible internal donor and could not distinguish the borrower policies.

Producing an interior minimum internal node with unequal eligible siblings through thousands of historical split/merge operations would add large append history without adding information about the local repair rule. Experiment 0130 instead constructs a test-only file directly with the existing private immutable-successor encoders and publisher.

The fixture is strictly valid under the current research validator and occupancy floor, then consumed by the same read-only repair-path inspector and public persistent deletion writer used elsewhere. No fixture-construction API is exposed outside `#[cfg(test)]`.

## Fixture

Current research geometry remains:

```text
LEAF_MIN_OCCUPANCY        = 93
INTERNAL_MIN_OCCUPANCY    = 128
INTERNAL_FANOUT           = 255
```

All 512 leaves contain exactly 93 objects. The level-2 root has three level-1 children with leaf-child occupancies:

```text
[129, 128, 255]
```

The file contains 47,616 objects and 516 active pages before deletion.

The selected object is the final object in the final leaf of the middle internal child. Its leaf has one minimum-occupancy left sibling and no right sibling within that parent.

## Common lower-level repair

For both policies:

```text
leaf before delete      = 93
leaf after delete       = 92
left leaf sibling       = 93
eligible leaf donor     = none
leaf repair             = merge
```

The leaf merge removes one child from the middle level-1 node:

```text
middle internal before  = 128
middle internal after   = 127
```

The internal node now underflows. At the root, both internal siblings can lend:

```text
left internal sibling   = 129
right internal sibling  = 255
```

## Policy divergence

The repair-path inspector and writer agree on the internal donor choice.

### LeftFirst

```text
selected donor          = left
selected occupancy      = 129 = M + 1
donor cliff             = yes
strictly fuller option  = right at 255
```

After lending:

```text
[129, 128, 255]
    -> [128, 128, 255]
```

There are now two root children exactly at the internal minimum.

### FullerSiblingLeftTie

```text
selected donor          = right
selected occupancy      = 255
donor cliff             = no
```

After lending:

```text
[129, 128, 255]
    -> [129, 128, 254]
```

Only the repaired middle child is at the internal minimum.

## Immediate reward is identical

Both persistent writer results satisfy:

```text
root level              = 2
object count            = 47,615
active page count       = 515
pages written           = 4
pages reused            = 511
canonical validation    = pass
```

The resulting byte strings and snapshot identities differ, as expected from the different donor choice, but strict validation returns identical logical locator sets.

Thus the policy distinction in this fixture is not a direct reward-map difference. The immediate structural reward is exactly equal; the policy changes the next occupancy state.

## Markov-reward interpretation

Experiments 0123–0128 showed that, at the leaf frontier, fuller-sibling's aggregate page-write advantage is a **visitation effect** rather than a different reward for the same repair class.

Experiment 0130 establishes the same separation one level higher:

```text
same internal underflow frontier
    -> policy selects different donor
    -> same immediate 4-page reward
    -> different next internal minimum mass
```

For this state, LeftFirst creates one additional minimum internal node by draining the `M+1` donor. Fuller-sibling avoids that creation by taking from the much fuller sibling.

This is the exact internal analogue of the donor-cliff flow identified at leaves in Experiments 0125 and 0128.

A multi-level renewal/Markov-reward model can therefore keep **reward at the repair event** separate from **policy-dependent evolution of minimum-frontier mass**. Internal minimum mass is not merely a theoretical extension; the real writer can change it differently under the two policies while doing the same amount of immediate page work.

## Scope boundary

The fixture is valid under the current research occupancy validator but is intentionally not claimed to be the output of a fresh canonical partition. It represents a reachable-shape-style local state for testing the persistent repair rule, not a new normative vector.

This single transition does not establish the stationary frequency of internal donor-cliff opportunities in realistic workloads. That is the next measurement problem.

No default borrower policy, writer bytes for existing inputs, FCP-0003 disposition, page geometry, authoritative vector identity, epoch allocation, or wire-format status changes in this experiment.
