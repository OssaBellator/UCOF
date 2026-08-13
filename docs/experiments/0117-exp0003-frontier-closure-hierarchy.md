# Experiment 0117: EXP-0003 frontier closure hierarchy

- **Status:** Reproducible stochastic closure diagnostic; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0115 and 0116
- **Script:** `tools/experiment_exp0003_frontier_closure.py`

## Question

Experiment 0116 showed that the neighbors of an underflowing target are distributed very differently from two ordinary iid leaves.

How much of the repair-outcome error disappears if the model retains **only the underflow-conditioned left and right sibling marginals**, while still treating those siblings as independent?

This tests a nested closure hierarchy:

```text
Closure 0: global iid sibling occupancy
Closure 1: target-conditioned left/right marginals, independent
Closure 2: target-conditioned joint pair state
```

Closure 2 exactly contains the immediate repair decision, but is larger. The purpose of Experiment 0117 is to measure how much accuracy Closure 1 recovers before paying for joint pair state.

## Mathematical framing

This is a state-aggregation problem.

Classical Markov-chain **lumpability** asks when a partition of a larger state space can be replaced by a smaller aggregated chain without changing selected transient or stationary quantities. Exact lumpability gives zero aggregation error for supported quantities; near-lumpability/approximate aggregation quantifies residual error.

Relevant references include:

- Peter Buchholz, *Exact and ordinary lumpability in finite Markov chains*, Journal of Applied Probability 31(1) (1994), 59-75, DOI `10.2307/3215235`.
- D. R. Barr and M. U. Thomas, *An Eigenvector Condition for Markov Chain Lumpability*, Operations Research 25(6) (1977), 1028-1031, DOI `10.1287/opre.25.6.1028`.

For UCOF the practical question is not whether every microscopic occupancy state can be aggregated exactly. It is whether a reduced state preserves the **repair rewards that matter**:

```text
borrow direction
merge probability
parent-boundary change
immutable pages/bytes emitted
```

Experiment 0117 uses immediate repair outcome as the first reward-preservation test.

## Workload

The geometry and workload match Experiments 0110, 0115, and 0116:

```text
page size      = 16 KiB
leaf capacity  C = 254
minimum        M = 127
```

Each cycle performs one random-gap insertion and one random-key deletion in randomized order while keeping live key count constant.

Policies:

```text
half-left-first
half-fullest-borrow
```

## Closure 0: global iid

Let `p[k]` be the global occupancy distribution.

Both siblings are treated as independent draws from `p`.

This is the closure rejected quantitatively by Experiment 0115.

## Closure 1: frontier-conditioned independent marginals

At every interior underflow, record exact sibling occupancies before repair.

Define:

```text
pL[k] = P(left sibling occupancy = k | target underflows)
pR[k] = P(right sibling occupancy = k | target underflows)
```

Closure 1 keeps `pL` and `pR`, but approximates the joint as:

```text
P(L=k, R=j | underflow) ~= pL[k] pR[j]
```

This preserves target-conditioning and left/right asymmetry while discarding residual pair dependence.

## Immediate repair formulas

### Left-first

Only lender eligibility matters.

With:

```text
l = pL[M]
r = pR[M]
```

Closure 1 predicts:

```text
P(left borrow)  = 1-l
P(right borrow) = l(1-r)
P(merge)        = lr
```

### Fuller-sibling

When both siblings can lend, the selected side depends on relative occupancy.

Under the independent frontier closure:

```text
P(left wins) = sum_k pL[k] * P(R <= k),  k > M
P(right wins)= sum_k pR[k] * P(L <  k),  k > M
P(merge)     = pL[M] pR[M]
```

The strict/non-strict split preserves the deterministic left tie-break.

## Full five-seed result

The full ensemble uses seeds `3, 17, 29, 43, 71`, 400,000 cycles per seed, and a 100,000-cycle burn-in.

### Closure error

| Policy | Global iid outcome TV | Frontier-independent outcome TV | Error reduction |
|---|---:|---:|---:|
| half-left-first | 0.07185 | 0.00119 | 98.49% |
| half-fullest-borrow | 0.02310 | 0.00907 | 53.67% |

This is the central result.

For left-first, simply conditioning the two sibling marginals on target underflow explains essentially all of the immediate repair-outcome error.

For fuller-sibling, conditioning is still a major improvement, but a meaningful residual remains because the policy explicitly compares the two sibling occupancies.

## Left-first closure

Global iid predicts approximately:

```text
left  = 96.23%
right =  3.63%
merge =  0.14%
```

Frontier-independent predicts:

```text
left  = 89.05%
right =  9.80%
merge =  1.15%
```

Observed:

```text
left  = 89.05%
right =  9.92%
merge =  1.03%
```

The frontier closure reproduces left-borrow probability exactly by construction of the conditional left marginal and misses the complete three-way outcome distribution by only about `0.00119` TV.

The merge estimate is about 10.8% high relative, but its absolute probability error is only about 0.12 percentage points.

### Modeling consequence

For immediate left-first repair rewards, a full `(left,target,right)` occupancy cube is unnecessary.

A reduced model can begin with:

```text
target frontier state
left eligibility / occupancy marginal conditional on frontier
right eligibility / occupancy marginal conditional on frontier
```

and introduce joint state only if recursive or future-transition rewards fail validation.

## Fuller-sibling closure

Global iid predicts approximately:

```text
left  = 50.70%
right = 49.20%
merge =  0.10%
```

Frontier-independent predicts:

```text
left  = 52.26%
right = 47.36%
merge =  0.39%
```

Observed:

```text
left  = 52.26%
right = 46.89%
merge =  0.86%
```

The frontier marginals recover much of the directional effect, but they still underpredict merge probability by roughly 55% relative.

The remaining outcome TV is about `0.00907`.

### Why fuller-sibling needs more state

The policy is itself a joint predicate:

```text
if left occupancy >= right occupancy:
    choose left
else:
    choose right
```

Marginals cannot fully determine the distribution of that comparison.

A decision-sufficient next state should therefore retain, at minimum, information equivalent to:

```text
left lender eligibility
right lender eligibility
left-vs-right occupancy ordering when both can lend
```

plus enough occupancy band information to predict the post-borrow state and future transitions.

This is still far smaller than storing all raw occupancy triples uniformly.

## State-reduction recommendation

The evidence now supports **policy-specific state complexity**.

### Left-first candidate model

Start with:

```text
target distance to M
conditional left occupancy band
conditional right occupancy band
```

and test whether the pair can be factorized for each transition/reward of interest.

For immediate repair direction, it almost can.

### Fuller-sibling candidate model

Retain:

```text
target distance to M
left occupancy band
right occupancy band
comparison sign when both lendable
```

The comparison sign is a decision-sufficient statistic for borrower choice, while occupancy bands retain information needed for the next state after decrementing the donor.

## Relation to lumpability

The correct abstraction should be tested like an approximate lumping, not accepted because it "looks equivalent."

For every proposed aggregate state, measure whether microscopic states inside the aggregate have nearly identical transition mass into each aggregate successor and nearly identical immutable-write rewards.

A useful validation quantity is therefore a **lumpability residual** such as the maximum or weighted total variation between outgoing aggregate-transition distributions of microstates placed in the same macrostate.

That gives the next experiment a concrete objective:

```text
choose occupancy bands / comparison state
-> estimate transition matrix
-> calculate within-lump transition residual
-> attach page/byte rewards
-> refine only lumps whose residual changes the FCP decision
```

This is more principled than adding occupancy bands until a plot looks smooth.

## Decision impact

Experiment 0117 does not choose a normative borrower policy.

It changes the modeling plan:

- left-first immediate repair can probably be modeled with target-conditioned sibling marginals and very little joint state;
- fuller-sibling requires a comparison-aware frontier state;
- state reduction should be evaluated with lumpability-style transition/reward residuals;
- raw `128^3` occupancy triples are not the starting point.

The existing default remains `LeftFirst`. FCP-0003, page geometry, authoritative vectors, and epoch allocation are unchanged.

## Reproduction

Quick CI run:

```console
python3 tools/experiment_exp0003_frontier_closure.py --quick
```

Full evidence run:

```console
python3 tools/experiment_exp0003_frontier_closure.py
```

Use `--json` for machine-readable trial and aggregate output.
