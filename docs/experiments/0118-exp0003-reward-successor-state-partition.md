# Experiment 0118: EXP-0003 reward/successor state partition

- **Status:** Exact one-step state enumeration; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0116 and 0117
- **Script:** `tools/experiment_exp0003_reward_successor_partition.py`

## Question

Experiments 0116–0117 showed that underflow-frontier state matters, but a raw exact pair of sibling occupancies has

```text
128 * 128 = 16,384
```

legal interior microstates at the first EXP-0003 Draft geometry.

How many of those microstates are actually distinguishable by the **immediate repair action, immutable leaf-write reward, leaf-count change, and post-repair occupancy class**?

This experiment answers that question exactly by exhaustive enumeration rather than sampling.

## Geometry and microstate

Use:

```text
leaf capacity C = 254
minimum       M = 127
underflow       = 126
```

At an interior underflow, the exact microstate is:

```text
(left sibling occupancy, right sibling occupancy)
```

where each sibling lies in `M..=C`.

Therefore:

```text
exact microstates = 128^2 = 16,384
```

The target occupancy is fixed at `M-1` at this decision epoch.

## Policies

The enumerator compares:

```text
left-first
fuller-sibling
```

Both retain the same half-full threshold and left-merge fallback.

For `fuller-sibling`, left wins an exact occupancy tie.

## One-step reward/successor signature

Every exact microstate is mapped to:

```text
(
    repair action,
    leaf pages emitted,
    leaf-count delta,
    post-left occupancy band,
    post-target occupancy band or absent,
    post-right occupancy band,
)
```

Immediate rewards are:

```text
borrow-left  -> 2 emitted leaf pages, leaf delta  0
borrow-right -> 2 emitted leaf pages, leaf delta  0
merge-left   -> 1 emitted leaf page, leaf delta -1
```

For the interior half-full case, merge occurs only when both siblings are exactly `M`, so:

```text
merged occupancy = M + (M-1) = 253
```

## Candidate occupancy resolutions

The experiment evaluates five successor occupancy partitions over the exact range `M..=C`.

### Eligibility-only

```text
M
M+1 .. C
```

### 3-band

```text
M
M+1 .. M+30
M+31 .. C
```

### 5-band

A progressively finer near-frontier/interior split.

### 7-band

The same resolution used by Experiment 0116:

```text
M
M+1
M+2 .. M+7
M+8 .. M+31
M+32 .. M+63
M+64 .. C-1
C
```

### 9-band

A finer near-frontier hierarchy while retaining `C` exactly.

## Exact result

| Scheme | Policy | Input cells | Cells needing refinement | Max signatures/cell | Refined input states | Unique reward/successor signatures | Compression from 16,384 |
|---|---|---:|---:|---:|---:|---:|---:|
| eligibility-only | left-first | 4 | 3 | 2 | 7 | 7 | 2340.6x |
| eligibility-only | fuller-sibling | 4 | 3 | 3 | 8 | 8 | 2048.0x |
| 3-band | left-first | 9 | 8 | 2 | 17 | 13 | 1260.3x |
| 3-band | fuller-sibling | 9 | 8 | 3 | 19 | 15 | 1092.3x |
| 5-band | left-first | 25 | 18 | 2 | 43 | 31 | 528.5x |
| 5-band | fuller-sibling | 25 | 21 | 3 | 49 | 35 | 468.1x |
| 7-band | left-first | 49 | 32 | 2 | 81 | 49 | 334.4x |
| 7-band | fuller-sibling | 49 | 32 | 3 | 85 | 49 | 334.4x |
| 9-band | left-first | 81 | 60 | 2 | 141 | 81 | 202.3x |
| 9-band | fuller-sibling | 81 | 60 | 3 | 147 | 81 | 202.3x |

All schemes have only three immediate reward classes because immediate action itself is one of:

```text
borrow-left
borrow-right
merge-left
```

The additional states preserve successor occupancy information needed by a future transition model.

## Main finding: a raw exact pair is unnecessary for one-step rewards

At the 7-band successor resolution, all 16,384 exact sibling pairs collapse to only:

```text
49 unique reward/successor signatures
```

for either policy.

That is approximately:

```text
16,384 / 49 ~= 334.4x
```

state compression for this one-step observable.

This is a much smaller starting point than a raw exact occupancy-pair chain.

## But the naive 7x7 input grid is not exact

The fact that there are 49 input band pairs and 49 unique one-step signatures does **not** mean those partitions coincide.

For both policies:

```text
32 of 49 input band cells contain more than one exact reward/successor signature
```

If input band identity is retained and each mixed cell is refined by signature:

```text
left-first      -> 81 refined states
fuller-sibling  -> 85 refined states
```

So fixed hand-selected occupancy bands are not an exact one-step abstraction by themselves.

### Why cells split

Borrowing decrements the lender.

Two lenders that begin in the same broad occupancy band can therefore land on different successor bands depending on whether the decrement crosses a band boundary.

For fuller-sibling there is an additional distinction: when both siblings can lend, the exact comparison

```text
left >= right
```

versus

```text
right > left
```

can select different donors even when both occupancies begin inside the same coarse band.

This is why the maximum number of signatures in one input cell is:

```text
left-first      = 2
fuller-sibling  = 3
```

at the 7-band resolution.

## Fidelity/state-count trade-off

Successor fidelity has a direct and measurable state cost.

At very coarse eligibility-only resolution, the exact one-step partition requires only 7–8 signatures.

At the 7-band resolution it requires 49.

At the 9-band resolution it requires 81.

That makes occupancy resolution a model-selection parameter rather than an arbitrary formatting choice.

The right resolution is the coarsest one whose transition and immutable-write reward errors are below the threshold that can change the FCP decision.

## Relation to lumpability

Finite Markov-chain lumpability provides the right mathematical language for the next step.

Exact lumpability requires microstates placed in one macrostate to send the same aggregate transition probability into every macrostate. Ordinary/strong variants differ in the initial-distribution and transient/stationary properties they preserve.

Useful references include:

- Peter Buchholz, *Exact and ordinary lumpability in finite Markov chains*, Journal of Applied Probability 31(1) (1994), 59-75, DOI `10.2307/3215235`.
- D. R. Barr and M. U. Thomas, *An Eigenvector Condition for Markov Chain Lumpability*, Operations Research 25(6) (1977), 1028-1031, DOI `10.1287/opre.25.6.1028`.

Experiment 0118 does **not** claim exact Markov lumpability.

It proves only one-step reward/successor equivalence at a selected successor-band resolution.

Two exact occupancy pairs with the same signature here may still have different probabilities of receiving the next insertion/deletion or reaching another macrostate later.

## Next model: transition residual

The next experiment should treat Experiment 0118's one-step signatures as candidate macrostates and estimate/derive their outgoing transition laws.

For every candidate macrostate, measure a lumpability-style residual such as:

```text
max over microstates x,y in macrostate A:
    TV(
        P(next macrostate | x),
        P(next macrostate | y)
    )
```

and separately measure reward spread for:

```text
leaf pages written
parent-boundary change
recursive repair
retained bytes
```

If residuals are too large, refine only the offending macrostate.

This gives a principled partition-refinement loop:

```text
exact microstates
    -> one-step reward/successor partition
    -> estimate transition residuals
    -> refine high-residual lumps
    -> solve stationary Markov reward model
    -> validate against Rust traces
```

## Connection to Experiments 0116–0117

The evidence chain is now:

1. **0115:** global iid sibling occupancy is quantitatively inaccurate.
2. **0116:** most of the missing state appears at the target underflow frontier rather than in generic adjacency.
3. **0117:** underflow-conditioned sibling marginals explain almost all immediate left-first outcome error, but fuller-sibling retains material joint/order dependence.
4. **0118:** exact one-step reward/successor equivalence can compress 16,384 frontier microstates to tens of candidate states, but naive occupancy bands require deterministic refinement.

Together these results define a tractable route to the correlated Markov reward model without committing to a raw occupancy cube.

## Decision impact

No normative deletion behavior changes here.

`LeftFirst` remains the repository default. `FullerSiblingLeftTie` remains an experimental EXP-0003 candidate supported by prior write-amplification evidence.

Experiment 0118 only improves the state representation used to compare those choices mathematically.

It does not accept FCP-0003, allocate an epoch, change authoritative vectors, or stabilize a wire format.

## Reproduction

```console
python3 tools/experiment_exp0003_reward_successor_partition.py
python3 tools/experiment_exp0003_reward_successor_partition.py --json
```
