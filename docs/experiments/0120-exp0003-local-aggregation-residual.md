# Experiment 0120: EXP-0003 local aggregation residual

- **Status:** Exact local-kernel aggregation diagnostic; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0116–0119
- **Script:** `tools/experiment_exp0003_local_aggregation_residual.py`

## Question

Experiment 0118 showed that all `16,384` exact interior-underflow sibling pairs can be compressed to only `49` one-step reward/successor signatures at the chosen seven-band occupancy resolution.

But one-step equivalence is not Markov lumpability.

Do exact microstates placed in one of those compressed states still have similar transition behavior **one local operation later**?

If not, what minimal extra state makes the local rows close enough to support a practical approximate aggregation?

## Mathematical motivation

Finite Markov-chain lumpability formalizes when a partition of a large chain can be replaced by a smaller chain while preserving selected results. Buchholz distinguishes exact and ordinary lumpability, shows which stationary/transient quantities can be recovered exactly under those conditions, and extends the framework to near-lumpability and bounds on remaining results:

- Peter Buchholz, *Exact and ordinary lumpability in finite Markov chains*, Journal of Applied Probability 31(1) (1994), 59-75, DOI `10.2307/3215235`.

Barr and Thomas likewise give conditions under which a partition produces a smaller chain that retains the Markov property:

- D. R. Barr and M. U. Thomas, *An Eigenvector Condition for Markov Chain Lumpability*, Operations Research 25(6) (1977), 1028-1031, DOI `10.1287/opre.25.6.1028`.

More recent state-reduction work provides formal transient/stationary error bounds when aggregation is only approximate rather than exactly lumpable:

- Fabian Michel and Markus Siegle, *Formal Error Bounds for the State Space Reduction of Markov Chains*, arXiv:2403.07618 (2024).

Experiment 0120 does not claim that its local window is the full UCOF chain or that the reported total-variation residual is itself one of those published formal bounds. It uses the same principle operationally: **microstates in one proposed aggregate should have similar outgoing rows for the observable transition/reward process**.

## Exact starting microstates

As in Experiment 0118:

```text
capacity C = 254
minimum  M = 127
underflow  = 126
```

An interior underflow decision starts from:

```text
(left sibling occupancy, right sibling occupancy)
```

with both siblings in `127..=254`, giving:

```text
128^2 = 16,384 exact microstates
```

The target is fixed at `126` immediately before repair.

## Immediate repair

The exact current policy and experimental policy are applied first:

```text
left-first
fuller-sibling
```

After immediate repair, the local window contains either:

```text
three leaves after a borrow
```

or:

```text
two leaves after a merge
```

The seven occupancy bands are unchanged from Experiments 0116 and 0118.

## One-more-local-operation kernel

From the exact repaired local window:

```text
P(insertion) = 1/2
P(deletion)  = 1/2
```

Conditional on insertion, a local leaf is selected in proportion to its number of insertion gaps:

```text
weight = occupancy + 1
```

Conditional on deletion, a local leaf is selected in proportion to its number of keys:

```text
weight = occupancy
```

The exact local transition is then applied:

- ordinary insertion increments one leaf;
- insertion into a full leaf splits it;
- ordinary deletion decrements one leaf;
- deletion underflow of the middle leaf in a three-leaf window is repaired exactly because both siblings are present;
- deletion underflow at a local-window boundary is classified as an **escape** because an external sibling is required and the experiment refuses to invent hidden context.

This deliberately bounded kernel answers a narrow question:

> after immediate repair, how much predictive state is needed for the next operation **inside the observed local window**?

It is not a whole-tree stationary model.

## Residual definition

For every proposed macrostate `A`, let `P_x` be the exact one-local-operation outcome distribution for exact microstate `x` in `A`.

The experiment uses the uniformly weighted macro-average row:

```text
P_A = average_{x in A} P_x
```

and reports:

```text
max_{A} max_{x in A} TV(P_x, P_A)
```

plus the mean row TV weighted uniformly over all 16,384 exact microstates.

Exact lumpability for the corresponding full macro-transition process would require a stronger zero-residual condition. Here, nonzero residual is used as a concrete **state-refinement diagnostic**.

## Candidate partitions

### 1. Signature-only

Use Experiment 0118's one-step signature only:

```text
immediate action
immediate leaf pages emitted
leaf-count delta
post-repair occupancy bands
```

This gives 49 macrostates for each policy.

### 2. Edge-aware

Add, for every post-repair leaf:

```text
current occupancy band
would decrement by 1 cross a band edge?
would increment by 1 cross a band edge?
```

This retains exactly the information needed to know whether the next ordinary `+1` or `-1` operation changes the coarse occupancy class.

It gives 225 macrostates.

### 3. Edge plus comparison

Add one further statistic for a three-leaf repaired window:

```text
left outer occupancy < right outer occupancy
left outer occupancy = right outer occupancy
left outer occupancy > right outer occupancy
```

For left-first this comparison is mostly irrelevant to the next borrower decision.

For fuller-sibling it is directly policy-relevant because a future middle underflow compares the two eligible siblings.

This gives:

```text
left-first      233 macrostates
fuller-sibling  237 macrostates
```

## Exact result

| Policy | Partition | Macrostates | Worst row TV | Microstate-weighted mean row TV | Compression |
|---|---|---:|---:|---:|---:|
| left-first | signature-only | 49 | ~0.5058 | ~0.0510 | 334.4x |
| fuller-sibling | signature-only | 49 | ~0.5077 | ~0.0502 | 334.4x |
| left-first | edge-aware | 225 | ~0.0351 | ~0.00777 | 72.8x |
| fuller-sibling | edge-aware | 225 | ~0.1546 | ~0.0106 | 72.8x |
| left-first | edge-plus-comparison | 233 | ~0.0351 | ~0.00777 | 70.3x |
| fuller-sibling | edge-plus-comparison | 237 | ~0.0351 | ~0.00772 | 69.1x |

## Main finding: one-step equivalence is not enough

The 49-state Experiment 0118 partition has a worst local next-row TV of about:

```text
0.51
```

for both policies.

That is far too large to treat those states as a credible multi-step Markov aggregation.

The failure has a concrete cause: a broad occupancy band hides whether the next `+1` or `-1` operation crosses a band boundary. Two pages that have the same current band and same immediate repair signature can therefore send substantial probability mass to different coarse successors one operation later.

This is exactly why Experiment 0118 explicitly stopped short of claiming lumpability.

## Edge-awareness fixes most of left-first

Adding only the `+1/-1 crosses band edge` flags reduces left-first's worst local row residual from roughly:

```text
0.506 -> 0.035
```

while retaining roughly a `73x` compression from exact sibling pairs.

The comparison sign changes essentially nothing for left-first:

```text
edge-aware worst TV          ~= 0.0351
edge-plus-comparison worst TV~= 0.0351
```

This matches the policy rule: left-first does not compare lender magnitudes once the left sibling is eligible.

## Fuller-sibling needs the comparison statistic

For fuller-sibling, edge-awareness alone is not enough:

```text
signature-only  ~= 0.508 worst TV
edge-aware      ~= 0.155 worst TV
```

The remaining large residual comes from exact left-vs-right ordering inside otherwise identical edge/band classes.

When the comparison sign is retained:

```text
edge-plus-comparison ~= 0.0351 worst TV
```

So a **single policy-sufficient ordering statistic** removes most of the remaining local aggregation error.

This independently reinforces Experiments 0116–0118:

- the state needed for left-first is largely eligibility/frontier geometry;
- fuller-sibling requires additional joint ordering state because its rule itself is a comparison predicate.

## Why `0.035` is not yet the final error bound

The remaining local row residual is not automatically an acceptable whole-tree error.

Three major gaps remain:

1. **External context escapes.** Boundary underflow is deliberately left unresolved here.
2. **Recursive parent state.** Leaf-boundary changes can propagate immutable repair upward.
3. **Long-horizon accumulation.** Small one-step row errors can accumulate differently depending on mixing, recurrence, and stationary mass.

The recent formal-error literature is useful precisely here: once the actual reduced transition matrix is defined, stepwise/transient and stationary residual bounds can be attached rather than treating a local TV number as self-justifying.

## Connection to Experiment 0119

Experiment 0119 found three kinds of real Rust workload behavior:

```text
policy-identical histories and cost
byte-distinct histories with equal cost
byte-distinct histories with lower fuller-sibling cost
```

Experiment 0120 explains why the eventual model must attach **rewards to transitions**, not merely preserve occupancy or root identity.

A reduced state that predicts the next coarse occupancy but loses whether a boundary is crossed, which lender is selected, or how many immutable pages are emitted is insufficient for the FCP decision.

The right model target is therefore a **Markov reward aggregation** whose state keeps:

```text
frontier eligibility
band-edge distance sufficient for next +/-1 class change
fuller-sibling comparison sign when relevant
parent-boundary/rewrite reward state
```

and whose residual is explicitly validated against the Rust trace matrix.

## Next step

The next model should add parent-boundary state and measured Rust rewards to the `edge-plus-comparison` frontier partition.

Then:

```text
1. estimate/derive macro transition rows;
2. measure within-macro row residuals;
3. attach leaf + parent page-write/byte rewards;
4. solve the stationary/recurrent reward model;
5. compare predicted policy deltas with Experiments 0114 and 0119;
6. refine only the macrostates whose residual can change the policy decision.
```

That is a much more tractable path than the original raw occupancy cube while remaining explicit about approximation error.

## Decision impact

Experiment 0120 does **not** select a normative deletion policy.

It improves the model architecture:

- the 49-state one-step partition is rejected as a multi-step state;
- band-edge awareness is necessary;
- fuller-sibling additionally needs an ordering statistic;
- approximately 230–240 local frontier states can reduce the worst one-more-operation row residual below `0.04` while retaining about `70x` compression.

`LeftFirst` remains the repository default. No FCP acceptance, epoch allocation, authoritative-vector change, or stable wire-format claim is made.

## Reproduction

```console
python3 tools/experiment_exp0003_local_aggregation_residual.py
python3 tools/experiment_exp0003_local_aggregation_residual.py --json
```
