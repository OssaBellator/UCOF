# Experiment 0115: EXP-0003 mean-field correlation gap

- **Status:** Reproducible analytical diagnostic; intentionally incomplete model
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0110, 0112, and 0114
- **Script:** `tools/experiment_exp0003_mean_field_gap.py`

## Question

Can Experiment 0110's mixed insert/delete behavior be predicted from only the marginal leaf occupancy distribution if sibling occupancies are treated as independent samples from that distribution?

A good fit would support a small mean-field model. A material miss identifies the state information a proper fringe/Markov model must retain.

## Mathematical basis

Classical fringe analysis tracks local tree configurations, derives transition matrices, and solves the resulting recurrences/linear systems for quantities such as expected split rates:

- Eisenbarth, Ziviani, Gonnet, Mehlhorn, and Wood, *The theory of fringe analysis and its application to 2-3 trees and B-trees*, Information and Control 55 (1982), 125-174, DOI `10.1016/S0019-9958(82)90534-4`.

Johnson and Shasha's mixed insert/delete analysis likewise treats utilization together with restructuring probability rather than using utilization alone:

- Theodore Johnson and Dennis Shasha, *B-trees with inserts and deletes: Why free-at-empty is better than merge-at-half*, JCSS 47(1) (1993), 45-76, DOI `10.1016/0022-0000(93)90020-W`.

Experiment 0115 deliberately starts with a weaker iid-neighbor closure to test whether the extra fringe state is actually necessary for UCOF.

## State and workload

Use the first EXP-0003 Draft leaf geometry from Experiment 0110:

```text
capacity C = 254
minimum M  = 127
```

The model state is only

```text
p[k] = fraction of legal leaves with occupancy k, M <= k <= C
```

One cycle contains one uniformly random-gap insertion and one uniformly random-key deletion.

Insertion target probability is proportional to `(k+1)p[k]`; deletion target probability is proportional to `k p[k]`.

Full-page insertion uses the proposed exact split:

```text
255 -> 128,127
```

For an underflow from `M` to `M-1`, left and right siblings are assumed iid from `p`.

## Mean-field repair probabilities

Let `m = p[M]`.

For left-first borrowing, conditional on underflow:

```text
P(left borrow)  = 1-m
P(right borrow) = m(1-m)
P(merge)        = m^2
```

For fuller-sibling borrowing, with cumulative distribution `F(k)`, occupancy `k>M` is selected as donor with mass

```text
p[k] * (F(k) + F(k-1))
```

which represents left=`k` with right<=`k` plus right=`k` with left<`k`, preserving the deterministic left tie-break.

Splits add one leaf and merges remove one leaf. If expected page-count drift is `dN`, normalized probability drift is

```text
dp[k]/dt = delta_count[k] - p[k] dN
```

The script integrates this deterministic ODE until the maximum residual is below `1e-9`.

## Result

The equilibrium prediction is approximately:

| Policy | Mean fill | Restructure / op | Left share of borrows |
|---|---:|---:|---:|
| half-left-first | 57.4% | 2.39% | ~95% |
| half-fullest-borrow | 57.4% | 1.74% | ~51% |

So the model reproduces the **direction** from Experiment 0110: fuller-sibling borrowing restructures less and largely removes the directional borrow bias.

But the absolute prediction is materially wrong.

Experiment 0110 reported approximately:

| Policy | Mean fill | Restructure / op |
|---|---:|---:|
| half-left-first | 62.31% | 1.4899% |
| half-fullest-borrow | 61.85% | 1.2524% |

The iid model therefore misses by roughly:

```text
left-first:
    fill              -4.9 percentage points
    restructuring     +60% relative

fuller-sibling:
    fill              -4.4 percentage points
    restructuring     +39% relative
```

Those errors are too large for an FCP write-amplification model.

## Interpretation

The one-dimensional occupancy marginal is useful as a sign check, but not accurate enough for normative cost estimates.

The likely missing quantity is local correlation created by the operations themselves:

- a split creates a related sibling pair;
- borrowing changes target and donor together;
- merging removes a jointly low pair;
- later random-key/gap selection is conditioned on resulting page occupancies.

The next model should therefore retain local fringe state rather than adding more detail to an iid closure.

A minimum useful neighborhood is something like

```text
(left occupancy, target occupancy, right occupancy)
```

near the repair frontier, with transition rewards for page writes, parent-boundary changes, split/borrow/merge events, and leaf-count change.

The state need not contain all `128^3` raw tuples: occupancies can be grouped into bands where exact values do not change the next repair branch, while keeping exact states near `M`, near `C`, and at split outputs.

## Validation path

A correlated model should be validated against two independent evidence sources:

1. Experiment 0110's long randomized leaf simulation for occupancy/restructuring distributions;
2. Experiment 0114's real Rust constant-set trace for page-write/reuse and retained-byte rewards.

The model should only gain authority once it predicts both within explicit error tolerances.

## Batched-update follow-up

Bender et al., *Bounding the Fragmentation of B-Trees Subject to Batched Insertions* (arXiv:2603.12211, 2026), generalizes classical random-insertion fragmentation analysis to batched consecutive-key workloads. UCOF's mixed writers naturally create batches, so batch-aware transitions matter eventually.

But the order should be:

```text
marginal iid model            <- Experiment 0115 diagnostic
correlated fringe model       <- next
batch-aware transition model  <- after correlation model validates
```

rather than adding batch complexity to a closure that already misses single-update behavior materially.

## Decision impact

Experiment 0115 strengthens the case for mathematically informed review while rejecting an oversimplified model:

- the cheap model agrees with the direction of the fuller-sibling benefit;
- the quantitative miss proves sibling/history correlations matter;
- classical fringe analysis supplies the appropriate next methodology;
- real Rust traces provide independent rewards for validation.

No normative deletion rule should be chosen from the iid mean-field numbers themselves.

## Reproduction

```console
python3 tools/experiment_exp0003_mean_field_gap.py
python3 tools/experiment_exp0003_mean_field_gap.py --json
```
