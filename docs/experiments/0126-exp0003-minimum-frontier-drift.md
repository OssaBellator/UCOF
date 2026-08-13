# Experiment 0126: EXP-0003 minimum-frontier one-step drift kernel

- **Status:** exact conditional-kernel / finite-horizon closure diagnostic; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0123–0125
- **Tool:** `tools/experiment_exp0003_minimum_frontier_drift.py`

## Question

Experiments 0124 and 0125 established two facts about the half-full EXP-0003 leaf process:

1. deletion-underflow arrival is controlled exactly by the number `n_M` of leaves at minimum occupancy `M`;
2. every realized change in `n_M` belongs to a small set of exactly conserved event flows.

Can the **one-step expected drift** of `n_M` be computed from a much smaller summary than the full ordered occupancy vector?

Yes. For fixed live-key cardinality, six integer statistics are sufficient for the immediate conditional expectation.

This is a reward/drift sufficiency result, **not** a claim that the six statistics form a closed Markov chain.

## Geometry

For the current candidate leaf geometry:

```text
capacity C = 254
minimum  M = 127
live keys K = 5,690 in the deterministic ensemble
```

## Six-count one-step statistic

For a pre-operation state, define:

```text
n_M       = leaves at occupancy M
n_M1      = leaves at occupancy M+1
n_C       = leaves at capacity C
L         = leaf count
n_cliff   = minimum leaves whose policy-selected eligible donor is exactly M+1
n_merge   = minimum leaves with no eligible donor
```

The remaining minimum leaves are neutral-borrow targets:

```text
n_neutral = n_M - n_cliff - n_merge
```

The tool enumerates all minimum targets before each deletion and asserts this partition exactly.

## Exact insertion kernel

Uniform random-gap insertion weights a leaf of occupancy `x` by `x+1`. Total gap mass is `K+L`.

Therefore:

```text
P(insert from M | state)
  = (M+1) n_M / (K+L)

P(split | state)
  = (C+1) n_C / (K+L)
```

Their minimum-mass rewards are `-1` and `+1`, respectively, so:

```text
E[Delta n_M | insert,state]
  = -(M+1)n_M/(K+L)
    + (C+1)n_C/(K+L)
```

No sibling-order information is needed for this immediate insertion drift.

## Exact deletion kernel

Uniform random-key deletion weights a leaf by its occupancy.

An ordinary `M+1 -> M` deletion therefore has hazard:

```text
P(delete M+1 -> M | state)
  = (M+1)n_M1/K
```

Every minimum target contributes `M` keys. If the policy-selected donor for that target is exactly `M+1`, the underflow repair creates one new minimum leaf at the donor:

```text
P(M+1 donor cliff | state)
  = M n_cliff / K
```

If a minimum target has no eligible donor, deletion from that target merges two minimum siblings and removes two minimum leaves:

```text
P(merge | state)
  = M n_merge / K
```

Hence:

```text
E[Delta n_M | delete,state]
  = (M+1)n_M1/K
    + M n_cliff/K
    - 2M n_merge/K
```

The borrower policy enters the immediate scalar drift only through `n_cliff` and `n_merge`, which are policy-aware classifications of minimum targets.

## Deterministic CI ensemble

The quick ensemble is unchanged from Experiments 0124–0125:

```text
seeds             = 3, 17, 29
cycles / seed     = 200,000
burn-in / seed    = 50,000
observed ops      = 900,000 per policy
```

Before every collected operation, the tool computes the exact conditional probabilities above. It then executes the same stochastic operation and records the realized flow class.

The accumulated expected counts are therefore sums of state-dependent conditional hazards, not fitted rates.

## CI-reproduced flow closure

| Policy | Flow | Observed | Expected | Relative residual |
|---|---|---:|---:|---:|
| left-first | insert from `M` | 14,659 | 14,565.932 | +0.639% |
| left-first | split creates `M` | 218 | 240.849 | -9.487% |
| left-first | delete `M+1 -> M` | 13,562 | 13,330.266 | +1.738% |
| left-first | borrow donor `M+1 -> M` | 1,305 | 1,253.907 | +4.075% |
| left-first | merge | 216 | 223.221 | -3.235% |
| fuller-sibling | insert from `M` | 11,627 | 11,731.533 | -0.891% |
| fuller-sibling | split creates `M` | 140 | 141.393 | -0.985% |
| fuller-sibling | delete `M+1 -> M` | 11,499 | 11,409.951 | +0.780% |
| fuller-sibling | borrow donor `M+1 -> M` | 267 | 288.306 | -7.390% |
| fuller-sibling | merge | 140 | 129.678 | +7.959% |

The high-frequency frontier exchanges close within 2%. The rarer structural flows have larger finite-sample residuals but remain within the configured 15% envelope.

No correction coefficient is fitted to any flow.

## Kernel-state means

The same CI trace yields:

| Statistic | Left-first | Fuller-sibling |
|---|---:|---:|
| `E[n_M]` before insert | 1.448109 | 1.166378 |
| `E[n_C]` before insert | 0.012018 | 0.007056 |
| `E[L]` before insert | 36.210024 | 36.538133 |
| `E[n_M]` before delete | 1.416202 | 1.140927 |
| `E[n_(M+1)]` before delete | 1.316827 | 1.127129 |
| `E[n_cliff]` before delete | **0.124842** | **0.028704** |
| `E[n_merge]` before delete | 0.022224 | 0.012911 |
| `E[n_neutral]` before delete | 1.269136 | 1.099311 |

The minimum-target partition is exact in every sampled state:

```text
n_M = n_cliff + n_merge + n_neutral
```

The strongest policy-sensitive reduced statistic is `n_cliff`: fuller-sibling carries only about 23% of left-first's mean donor-cliff target mass.

Because

```text
P(donor cliff | state) = M n_cliff / K
```

this statistic directly connects the local borrower rule to Experiment 0125's realized donor-cliff reduction.

## Separate drift closure

Accumulating the exact one-step rewards gives:

### Left-first

```text
expected insertion drift = -14,325.083
observed insertion drift = -14,441

expected deletion drift  = +14,137.732
observed deletion drift  = +14,435
```

### Fuller-sibling

```text
expected insertion drift = -11,590.141
observed insertion drift = -11,487

expected deletion drift  = +11,438.900
observed deletion drift  = +11,486
```

These large opposing components are the meaningful closure targets.

The net endpoint drift over a long near-stationary trace is tiny because insertion and deletion flows nearly cancel. Comparing a small realized endpoint such as `-6` or `-1` against the difference of two independently noisy expectations near `14,000` would amplify Monte Carlo noise and is therefore a poor kernel-validation statistic.

Experiment 0126 validates the component hazards and their large signed drift components separately.

## What is sufficient, and what is not

For the immediate scalar reward `Delta n_M`, the six-count statistic is sufficient:

```text
(n_M, n_M1, n_C, L, n_cliff, n_merge)
```

determines the exact conditional expected drift.

That does **not** imply:

```text
P(S_{t+1} | full history) = P(S_{t+1} | six counts)
```

for the six-count state `S`.

The ordered occupancy configuration can still affect how `n_cliff`, `n_merge`, and the occupancy masses evolve after a transition. In particular, adjacency and sibling ordering are precisely the correlations exposed by Experiments 0116–0120.

So the model reduction should be stated as:

> six statistics are sufficient for the one-step minimum-frontier drift reward, while additional state or an approximation/error bound is still required for multi-step transition closure.

## Relation to the reward model

Experiments 0123–0126 now separate four mathematical layers:

```text
borrower choice
  -> policy-aware local target classes (`n_cliff`, `n_merge`)
  -> exact one-step occupancy-frontier drift
  -> exact underflow/split arrival hazards
  -> structural-state visitation
  -> immutable page-write reward
```

This is a better modeling architecture than assigning a direct reward discount to fuller-sibling borrowing.

## Next model

The remaining hard problem is no longer the immediate `n_M` reward. It is the evolution of the compact statistics themselves.

A useful next experiment should test a reduced stationary/fixed-point closure for the mean minimum mass, with explicit approximation error. One candidate balance is obtained by setting the long-run expected `n_M` drift to zero and expressing the cliff/merge terms through frontier-conditioned probabilities from Experiments 0116–0120.

That model must be validated out of sample and must not use observed `E[n_M]` as an input when claiming to predict `E[n_M]`.

A complementary implementation check is to instrument the real persistent Rust trace with analogous minimum/donor-cliff state and verify that the same mechanism appears outside the leaf-only stochastic model.

## Decision impact

Experiment 0126 strengthens the state-reduction case for EXP-0003 analysis: immediate minimum-frontier drift does not require a raw full-tree occupancy vector.

It also preserves the causal policy result: fuller-sibling sharply lowers the policy-selected `M+1` donor-cliff target mass that enters the exact drift kernel.

This remains research evidence only. `LeftFirst` remains the repository default. FCP-0003 acceptance, epoch allocation, authoritative-vector status, and wire-format stability remain unchanged.

## Reproduction

Quick CI ensemble:

```console
python3 tools/experiment_exp0003_minimum_frontier_drift.py --quick
```

Longer evidence ensemble:

```console
python3 tools/experiment_exp0003_minimum_frontier_drift.py
```
