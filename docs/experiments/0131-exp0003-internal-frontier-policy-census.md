# Experiment 0131 — Internal frontier policy census

## Question

How large is the local internal-state region in which `LeftFirst` and `FullerSiblingLeftTie` choose different donors, and how much of that region actually changes future minimum-frontier mass through the donor-cliff mechanism established by Experiment 0130?

## Scope

This experiment is an **exact unweighted local-state census**. It enumerates sibling occupancies at a fixed internal underflow frontier; it does not estimate how frequently those states occur under a workload.

The conditioned event is:

```text
non-root internal node before child merge = M
a lower merge removes one child           = M - 1
```

Under the current research geometry:

```text
M = INTERNAL_MIN_OCCUPANCY = 128
C = INTERNAL_FANOUT        = 255
```

Each immediate sibling occupancy ranges over `M..=C`, so the ordered local sibling-state space contains:

```text
(C - M + 1)^2 = 128^2 = 16,384 states
```

The executable `tools/experiment_exp0003_internal_frontier_census.py` independently implements both borrower rules and checks the enumerated counts against closed-form identities.

## Borrow/merge geometry

Across all 16,384 ordered sibling pairs:

```text
no eligible donor / merge     = 1
exactly one eligible donor     = 254
two eligible donors            = 16,129
```

These are the identities:

```text
merge       = 1
single      = 2(C - M) = 254
two-donor   = (C - M)^2 = 127^2 = 16,129
```

For every local state, the executable asserts that the two policies agree on whether a donor exists. Borrower policy can change donor **identity**, but not borrow-versus-merge classification at a fixed frontier.

That is the local reward-class invariant used by the transition/reward decomposition: if page reward is conditioned on the same repair class and path context, changing donor selection does not by itself change whether the repair is a borrow or a merge.

## Donor-identity divergence

In the two-donor domain, `LeftFirst` always chooses the left donor. `FullerSiblingLeftTie` chooses the right donor exactly when the right sibling is strictly fuller.

Therefore the policies choose different donors in:

```text
sum(left=129..254, 255-left)
= 127 * 126 / 2
= 8,001 states
```

This is:

```text
8,001 / 16,129 = 49.6062992126%
```

of the two-donor state space.

So donor-identity divergence is not rare in an unweighted geometric sense.

## Donor-cliff divergence is much narrower

A donor-cliff event occurs when the selected donor has occupancy `M+1 = 129`, so lending drains it to the minimum `M`.

Within the two-donor domain:

```text
LeftFirst donor-cliff states = 127
Fuller donor-cliff states    = 1
```

`LeftFirst` hits the cliff whenever the left donor is 129 and the right donor is any eligible value `129..255`.

Fuller-sibling hits the cliff only in the unavoidable tie:

```text
(left, right) = (129, 129)
```

The avoidable donor-cliff states are therefore exactly:

```text
(129, right), right=130..255
```

for a total of:

```text
126 states
```

Those same 126 states are exactly the states in which the two policies produce a different local post-repair minimum count.

The fractions are:

```text
avoidable cliff / two-donor states       = 126 / 16,129 = 0.7812015624%
avoidable cliff / donor-divergent states = 126 / 8,001  = 1.5748031496%
```

## Why the distinction matters

Experiment 0130 exhibited one concrete internal state from the 126-state donor-cliff subset:

```text
(left, target, right) = (129, 128, 255)
```

After the target loses one child, both policies perform the same repair class and write the same number of pages, but:

```text
LeftFirst            drains 129 -> 128
FullerSiblingLeftTie drains 255 -> 254
```

Only the first choice creates an additional minimum internal node.

Experiment 0131 shows that **policy divergence is not a sufficient statistic for future frontier hazard**. Nearly half of the unweighted two-donor states change donor identity, but only about 0.78% change minimum mass through this specific donor-cliff mechanism.

A reduced multi-level model should therefore weight the narrow state statistic that controls minimum-mass flow—such as `M+1` donor-cliff opportunity mass—rather than using generic donor-choice disagreement as a proxy.

## Modeling boundary

The census is intentionally unweighted. The 16,129 two-donor states are not assumed equiprobable under persistent workloads, and the percentages above are **state-space fractions**, not observed event probabilities.

The remaining empirical question is the stationary/workload-dependent weight placed on the 126 avoidable internal donor-cliff states and on analogous internal merge/split frontier states.

This experiment does not prove Markov lumpability, stationary internal occupancy, or a normative borrower policy.

## Boundary

Research/modeling evidence only. No default `LeftFirst` behavior, persistent writer bytes, FCP-0003 disposition, page geometry, authoritative vectors, epoch allocation, or wire-format status changes.
