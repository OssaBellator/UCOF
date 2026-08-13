# Experiment 0127: EXP-0003 minimum-frontier phase-blind fixed point

- **Status:** approximate stationary-balance / finite-horizon closure diagnostic; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0123–0126
- **Tool:** `tools/experiment_exp0003_minimum_frontier_fixed_point.py`

## Question

Experiment 0126 established an exact one-step drift kernel for the number `n_M` of leaves at minimum legal occupancy `M`.

Can that kernel be turned into a useful stationary-balance prediction of `n_M` **without feeding the observed `n_M` back into the predictor**?

Experiment 0127 tests the smallest such closure.

It keeps five mean inputs from the Experiment 0126 kernel:

```text
E[n_(M+1)] before deletion
E[n_C] before insertion
E[leaf_count] before insertion
E[n_cliff] before deletion
E[n_merge] before deletion
```

and solves the zero-drift equation for a phase-blind minimum mass `n_M*`.

The result is deliberately compared with both operation phases separately and with their empirical midpoint. This exposes, rather than hides, the residual state carried by operation phase.

## Zero-drift closure

For fixed live-key cardinality `K`, capacity `C`, and minimum occupancy `M`, Experiment 0126 gives the conditional drift components:

```text
insert from M:                 -(M+1)n_M/(K+L)
split creates M:               +(C+1)n_C/(K+L)
delete M+1 -> M:               +(M+1)n_(M+1)/K
borrow donor M+1 -> M:         +M n_cliff/K
merge two minimum leaves:      -2M n_merge/K
```

Experiment 0127 replaces the state-dependent quantities other than `n_M` with their finite-horizon means and approximates

```text
E[n_C/(K+L)]
```

by

```text
E[n_C] / (K + E[L]).
```

It then solves:

```text
0 = -(M+1)n_M*/(K+E[L])
    + (C+1)E[n_C]/(K+E[L])
    + (M+1)E[n_(M+1)]/K
    + M E[n_cliff]/K
    - 2M E[n_merge]/K
```

so that:

```text
n_M*
  = (K+E[L])/(M+1)
    * [
        (C+1)E[n_C]/(K+E[L])
        + (M+1)E[n_(M+1)]/K
        + M E[n_cliff]/K
        - 2M E[n_merge]/K
      ]
```

Observed `E[n_M]` is **not** an input to this fixed-point predictor.

## Why there are two empirical phases

Each cycle contains one insertion and one deletion, with their order randomized.

That means the same long-run process has two nearby observation phases:

```text
minimum mass immediately before insertion
minimum mass immediately before deletion
```

These are not identical because an insertion or deletion can itself move occupancy mass across the `M` frontier.

A scalar phase-blind fixed point should therefore not be expected to reproduce both phase-specific means exactly unless operation phase is irrelevant.

Experiment 0127 makes this a falsification test:

- the fixed point must lie between the two phase means;
- it must close tightly to their midpoint;
- it must preserve the borrower-policy ordering;
- underflow hazard derived from `n_M*` must remain close to the exact pre-delete state-hazard value from Experiment 0124.

## Deterministic CI ensemble

The quick ensemble remains:

```text
seeds             = 3, 17, 29
cycles / seed     = 200,000
burn-in / seed    = 50,000
observed ops      = 900,000 per policy
capacity C        = 254
minimum M         = 127
live keys K       = 5,690
```

The same underlying deterministic ensemble is used by Experiments 0124–0127, so the closures are directly comparable.

## CI-reproduced fixed-point closure

| Quantity | Left-first | Fuller-sibling |
|---|---:|---:|
| predicted phase-blind `n_M*` | **1.429421226** | **1.151300765** |
| observed `n_M` before insert | 1.448108889 | 1.166377778 |
| observed `n_M` before delete | 1.416202222 | 1.140926667 |
| empirical phase midpoint | 1.432155556 | 1.153652222 |
| error vs pre-insert | -1.2905% | -1.2926% |
| error vs pre-delete | +0.9334% | +0.9093% |
| error vs midpoint | **-0.1909%** | **-0.2038%** |
| pre-insert minus pre-delete phase gap | 0.031906667 | 0.025451111 |

For both policies:

```text
pre-delete n_M < predicted n_M* < pre-insert n_M
```

and the phase-blind fixed point lands within about 0.2% of the empirical phase midpoint.

That is much tighter than either phase-specific error, which remains around 1%.

## Underflow-hazard consequence

Experiment 0124 gives the exact state-dependent underflow arrival law:

```text
lambda_underflow / operation
  = 0.5 * M * n_M / K
```

Using the fixed-point `n_M*` instead of observed pre-delete minimum mass gives:

| Quantity | Left-first | Fuller-sibling |
|---|---:|---:|
| fixed-point predicted underflow rate/op | 0.0159522403962 | 0.0128484356028 |
| exact state-hazard rate from observed pre-delete `n_M` | 0.0158047172427 | 0.0127326613943 |
| relative error | +0.9334% | +0.9093% |

The hazard error is identical to the pre-delete `n_M` relative error because the hazard is linear in `n_M`.

The fixed-point closure therefore preserves the correct scale and policy ordering without observing `n_M` directly.

## Policy separation remains much larger than closure error

The predicted minimum-mass gap is:

```text
1.429421226 - 1.151300765 = 0.278120461
```

or about a 19.5% reduction relative to the left-first fixed point.

The corresponding predicted underflow-rate gap is:

```text
0.0159522403962 - 0.0128484356028
  = 0.0031038047934 operations^-1
```

The roughly 0.2% midpoint closure error is therefore far smaller than the between-policy signal. The ordering is not manufactured by the approximation error.

## What the phase residual means

The fixed point succeeds best as a **phase-neutral** occupancy summary and misses each operation phase by about 1% in opposite directions.

That suggests the next irreducible compact state variable is not another raw sibling occupancy value but a one-bit operation phase:

```text
before insertion
before deletion
```

This is consistent with the flow equations:

- insertion tends to remove minimum mass through `M -> M+1` while occasionally creating it through splits;
- deletion tends to add minimum mass through `M+1 -> M` and donor cliffs while removing it through merges.

A two-phase closure could therefore model the alternating occupancy bias explicitly instead of asking one scalar fixed point to represent both phases.

Experiment 0127 does not add that phase correction because the purpose is to measure whether it is needed. The observed ±1% residual says that it is relevant if sub-percent phase-specific prediction becomes a requirement.

## What this closure still uses as measured inputs

Although observed `n_M` is removed from the predictor, Experiment 0127 still uses measured means for:

```text
E[n_(M+1)]
E[n_C]
E[L]
E[n_cliff]
E[n_merge]
```

So this is **not** an autonomous stationary model of the occupancy process.

The result should be read as:

> given the other five compact kernel-state means, the zero-drift equation predicts the phase-neutral minimum-frontier mass very accurately in this deterministic finite-horizon ensemble.

It does not yet answer how those five predictor means arise from policy and workload.

## Relation to Experiments 0123–0126

The analytical chain is now:

```text
borrower choice
  -> policy-aware donor/merge target classes
  -> exact one-step minimum-mass drift kernel
  -> approximate zero-drift minimum-mass fixed point
  -> exact underflow arrival hazard
  -> structural-state visitation
  -> immutable page-write reward
```

Experiment 0127 closes the scalar minimum-mass balance much more tightly than the earlier global-iid repair models because it preserves the policy-sensitive frontier flow terms identified in Experiments 0124–0126.

## Remaining model gap

The main mathematical gap is now the evolution of the five predictor statistics themselves.

A complete reduced stationary model would need to predict, or explicitly bound approximation error for:

```text
n_(M+1)
n_C
leaf count
cliff-target mass
merge-target mass
operation phase
```

The cliff/merge target statistics still depend on local adjacency and sibling ordering, so Experiments 0116–0120 remain relevant to closing that transition law.

## Next evidence priority

Before adding more mean-field layers, the most decision-relevant next check is to instrument the **real persistent Rust policy traces** with analogous frontier state:

```text
target occupancy
left/right eligible sibling occupancy
selected donor occupancy
M+1 donor-cliff event
strictly fuller alternative available
minimum-leaf mass where feasible
```

That would test whether the mechanism established in the leaf-only stochastic model—avoiding barely eligible donors and reducing future frontier visitation—appears directly in the persistent implementation workloads that produced Experiment 0123's page-write savings.

## Decision impact

Experiment 0127 strengthens the reduced-state analytical case for `FullerSiblingLeftTie`: the policy-specific fixed-point minimum mass remains substantially lower, and a simple zero-drift closure predicts phase-neutral minimum mass to about 0.2% in the CI ensemble.

The phase-specific residual is preserved as an explicit limitation rather than fitted away.

This remains research evidence only. `LeftFirst` remains the repository default. FCP-0003 acceptance, epoch allocation, authoritative-vector status, and wire-format stability remain unchanged.

## Reproduction

Quick CI ensemble:

```console
python3 tools/experiment_exp0003_minimum_frontier_fixed_point.py --quick
```

Longer evidence ensemble:

```console
python3 tools/experiment_exp0003_minimum_frontier_fixed_point.py
```
