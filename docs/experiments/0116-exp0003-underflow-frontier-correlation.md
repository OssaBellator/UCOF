# Experiment 0116: EXP-0003 underflow-frontier correlation

- **Status:** Reproducible stochastic diagnostic; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0110, 0114, and 0115
- **Script:** `tools/experiment_exp0003_underflow_correlation.py`

## Question

Experiment 0115 showed that an iid-neighbor mean-field model gets the direction of the deletion-policy comparison right but misses absolute fill and restructuring rates badly.

What correlation state is actually missing?

The important distinction is between:

1. ordinary neighboring leaves sampled from the tree; and
2. neighboring leaves **conditioned on the target leaf having just crossed the deletion underflow frontier**.

If ordinary adjacency is already strongly dependent everywhere, a large fringe-state model may be unavoidable.

If most of the error appears only after conditioning on the repair frontier, a much smaller frontier-conditioned transition model may be sufficient.

## Mathematical motivation

Classical fringe analysis represents local tree configurations as a finite collection of states and derives a transition matrix whose recurrence converges to a linear-system solution. This supports expected split and related local-event calculations:

- B. Eisenbarth, N. Ziviani, G. H. Gonnet, K. Mehlhorn, D. Wood, *The theory of fringe analysis and its application to 2-3 trees and B-trees*, Information and Control 55 (1982), 125-174, DOI `10.1016/S0019-9958(82)90534-4`.

Johnson and Shasha's mixed insert/delete work likewise shows that utilization alone is insufficient and that restructuring probability depends on retained local history/state:

- Theodore Johnson and Dennis Shasha, *B-trees with inserts and deletes: Why free-at-empty is better than merge-at-half*, JCSS 47(1) (1993), 45-76, DOI `10.1016/0022-0000(93)90020-W`.

Experiment 0116 does not yet solve the final transition matrix. It identifies the smallest local state that a useful matrix must retain.

## Workload and geometry

The experiment reuses the first EXP-0003 Draft leaf geometry and balanced workload from Experiment 0110:

```text
page size      = 16 KiB
leaf capacity  C = 254
minimum        M = 127
```

Each cycle performs one random-gap insertion and one random-key deletion in randomized order while preserving live key cardinality.

The two policies are:

```text
half-left-first
half-fullest-borrow
```

Insertion splitting and the half-full minimum are identical between them.

## Occupancy bands

Exact occupancy is retained for the global marginal used to predict borrow/merge outcomes.

For robust joint-distribution diagnostics, neighboring pages are also grouped into seven bands:

```text
M
M+1
M+2 .. M+7
M+8 .. M+31
M+32 .. M+63
M+64 .. C-1
C
```

These preserve exact states closest to underflow while compressing interior occupancies that do not immediately change the repair branch.

## Metrics

The script measures:

### Ordinary adjacency

For sampled adjacent leaves, compare the observed joint occupancy-band distribution with the product of its observed left/right marginals.

Report:

```text
total-variation distance from iid
mutual information in bits
```

### Underflow-conditioned neighborhood

Whenever deletion moves an interior target from `M` to `M-1`, record the left and right sibling occupancy bands **before repair**.

Compare that joint distribution with the product of the global occupancy-band marginal.

This asks the exact question Experiment 0115 could not answer:

> are the siblings of an underflowing target distributed like two ordinary independent leaves?

### Repair outcome error

Using the global exact occupancy marginal, compute the same iid predictions as Experiment 0115.

For left-first, if `m = P(occupancy=M)`:

```text
P(left borrow | underflow)  = 1-m
P(right borrow | underflow) = m(1-m)
P(merge | underflow)        = m^2
```

For fuller-sibling borrowing, donor direction is computed from the exact marginal cumulative distribution with deterministic left tie-break.

These iid predictions are compared with the actual observed repair outcomes at interior underflows.

## Full five-seed result

The full ensemble uses seeds:

```text
3, 17, 29, 43, 71
```

with 400,000 cycles per seed and a 100,000-cycle burn-in.

### Distribution-level result

| Policy | Mean fill | Ordinary adjacent TV from iid | Underflow-neighbor TV from global iid | Underflow neighbor MI |
|---|---:|---:|---:|---:|
| half-left-first | 62.02% | 0.0585 | 0.2451 | 0.0120 bits |
| half-fullest-borrow | 61.85% | 0.0564 | 0.2128 | 0.0685 bits |

The key result is not that all neighboring leaves are strongly dependent.

Ordinary adjacent leaves are only modestly different from an independent product at this band resolution:

```text
TV ~= 0.056 .. 0.059
```

But conditioning on the target being exactly at the repair frontier changes the sibling-pair distribution much more strongly:

```text
TV ~= 0.213 .. 0.245
```

So the dominant missing state in Experiment 0115 is **target-conditioned local context**, not merely a generic global neighbor-correlation coefficient.

## Repair-outcome result

### Left-first

Global iid predicts approximately:

```text
left borrow  = 96.23%
right borrow =  3.63%
merge        =  0.14%
```

Observed at interior underflows:

```text
left borrow  = 89.05%
right borrow =  9.92%
merge        =  1.03%
```

The complete repair-outcome distribution is about `0.0718` total-variation distance from the iid prediction.

Most importantly:

```text
actual merge probability / iid merge probability ~= 7.29x
```

### Fuller-sibling

Global iid predicts approximately:

```text
left borrow  = 50.70%
right borrow = 49.20%
merge        =  0.10%
```

Observed:

```text
left borrow  = 52.26%
right borrow = 46.89%
merge        =  0.86%
```

The overall direction prediction is closer than for left-first, but merge probability is still severely wrong:

```text
actual merge probability / iid merge probability ~= 8.41x
```

That large merge error is especially important for UCOF because merge events change parent boundaries and can propagate immutable rewrite cost upward.

## Interpretation

Experiment 0115 suggested a raw state such as:

```text
(left occupancy, target occupancy, right occupancy)
```

for a correlated fringe model.

Experiment 0116 narrows that recommendation.

A practical first transition model should concentrate exact state around:

```text
target occupancy near M
left sibling occupancy near M
right sibling occupancy near M
```

while treating ordinary non-frontier leaves with a coarser marginal/banded state.

This is a substantial state-space reduction.

Instead of treating every triple across the full `128^3` occupancy cube as equally important, the model can use a two-regime representation:

```text
ordinary regime:
    coarse marginal / occupancy-band state

repair-frontier regime:
    explicit local sibling tuple conditioned on target near M
```

The transition matrix can then attach rewards for:

```text
borrow direction
merge
split
leaf pages written
parent boundary changes
expected recursive parent repair
retained bytes
```

and validate the resulting stationary reward rates against Experiments 0110 and 0114.

## Important nuance

Underflow-neighbor mutual information is not enormous for left-first (`~0.012` bits), even though its pair distribution is far from the **global iid product**.

That distinction matters.

Much of the error comes from the fact that neighbors **conditioned on an underflowing target** have different marginals from ordinary leaves. It is not necessary for left and right siblings to be highly dependent on each other for the iid global model to fail.

For fuller-sibling, the underflow-conditioned left/right mutual information is larger (`~0.069` bits), indicating additional pair dependence induced by the borrower-selection rule.

This is why the next model should encode the target-conditioned fringe state explicitly rather than trying to repair Experiment 0115 with one scalar correlation factor.

## Relation to batched updates

Bender et al., *Bounding the Fragmentation of B-Trees Subject to Batched Insertions* (arXiv:2603.12211, 2026), shows that insertion workload structure can change fragmentation behavior and appropriate split strategies.

That remains relevant to UCOF's mixed writers, but Experiment 0116 further supports the progression:

```text
iid marginal model                  <- 0115; rejected quantitatively
frontier-conditioned diagnostic     <- 0116
frontier fringe transition model    <- next
batch-aware transition extension    <- after validation
```

## Decision impact

Experiment 0116 strengthens two conclusions:

1. `FullerSiblingLeftTie` remains a serious EXP-0003 candidate because its repair direction stays substantially less biased while Experiment 0114 already shows lower real persistent write cost.
2. The next mathematical model can be smaller and more targeted than a full raw occupancy-triple chain: exact local state is most valuable at the underflow frontier.

No normative deletion rule changes here. Existing/default successor behavior remains left-first, and no epoch allocation or FCP acceptance is implied.

## Reproduction

Quick CI diagnostic:

```console
python3 tools/experiment_exp0003_underflow_correlation.py --quick
```

Full evidence run:

```console
python3 tools/experiment_exp0003_underflow_correlation.py
```

JSON output is available with `--json`.
