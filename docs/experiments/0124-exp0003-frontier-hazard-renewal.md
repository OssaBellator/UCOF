# Experiment 0124: EXP-0003 frontier hazard and renewal diagnostic

- **Status:** deterministic finite-horizon research diagnostic; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0110, 0116–0123
- **Tool:** `tools/experiment_exp0003_frontier_hazard.py`

## Question

Experiment 0123 showed that the Rust policy advantage is a **visitation/history effect** in the measured constant-content traces: both policies use the same observed 2-page/3-page reward map, but `FullerSiblingLeftTie` visits the expensive class much less often.

What state controls arrival at the repair frontier in the random-key/random-gap occupancy model?

A first prototype attempted to use only the post-repair local sibling state as an embedded Markov chain. That is not a safe closure assumption: the next repair can occur elsewhere in the tree, so a local post-repair window need not predict the next global frontier event.

Experiment 0124 therefore starts one level earlier and asks for the **conditional arrival hazard** of two structural frontiers:

1. deletion underflow;
2. insertion split.

The resulting identities are exact for the leaf-level random-key/random-gap process and do not depend on the borrower policy.

## Exact underflow hazard

Let:

```text
M       = minimum legal leaf occupancy
K       = number of live keys
n_M     = number of leaves currently at occupancy M
```

A uniformly random deletion chooses a key from a minimum leaf with probability

```text
M * n_M / K
```

and deleting from such a leaf produces the underflow frontier.

Therefore:

```text
P(underflow on next deletion | state)
    = M * n_M / K
```

The mixed process has exactly one insertion and one deletion per cycle, so expressed per operation:

```text
underflow hazard / operation
    = 0.5 * M * E[n_M before deletion] / K
```

The experiment asserts this identity to machine precision for every aggregate it emits.

This is the first clean bridge from Experiment 0123's reward-visitation result to a state variable:

> the borrower rule does not change the conditional underflow formula; it changes how much process mass accumulates at `occupancy == M`.

## Exact split hazard

Let:

```text
C       = leaf capacity
n_C     = number of full leaves
L       = current leaf count
```

A uniformly random insertion gap weights a leaf with occupancy `x` by `x + 1` gaps. The total gap mass is:

```text
K + L
```

and every full leaf contributes `C + 1` gaps.

Therefore:

```text
P(split on next insertion | state)
    = (C + 1) * n_C / (K + L)
```

Again, the borrower policy is absent from the conditional formula. It can affect split frequency only by changing the occupancy/leaf-count process that supplies `n_C` and `L`.

## Three-layer model architecture

The experiments now support a cleaner separation of state responsibilities.

### Layer 1: global frontier arrival

Global occupancy mass controls how often the process reaches a structural frontier:

```text
minimum-leaf mass -> deletion-underflow hazard
full-leaf mass    -> insertion-split hazard
```

Experiment 0124 measures this layer.

### Layer 2: repair outcome

Conditioned on an underflow, sibling occupancy correlation determines whether repair is:

```text
borrow-left
borrow-right
merge
```

Experiments 0116–0118 showed why the global iid occupancy marginal is not sufficient here and why fuller-sibling needs pair/order information that left-first largely does not.

### Layer 3: immutable reward

Conditioned on a structural transition, parent/root context determines immutable page-write reward.

Experiment 0122 pins those structural rewards, including the fact that the same leaf merge can emit different numbers of pages depending on root collapse or recursive parent repair.

Experiment 0123 then showed that policy differences should be modeled through changed state visitation rather than by assigning `fuller-sibling` a cheaper intrinsic reward constant.

## Renewal-style holding time

The tool records the number of operations between consecutive observed underflow frontiers.

This is a useful holding-time statistic, but the experiment intentionally does **not** claim iid renewal intervals. The hazard changes with the evolving occupancy state, so the correct future object is a state-dependent semi-Markov/Markov-reward process or an explicitly bounded approximation.

For a long finite trace with underflow rate `lambda`, the empirical reciprocal rate

```text
1 / lambda
```

and the mean complete inter-underflow holding time should be close apart from first/last censoring and finite-sample effects.

A successful borrower policy can therefore reduce repair frequency by increasing the mean time spent away from the underflow frontier.

## Deterministic quick ensemble

CI uses:

```text
seeds             = 3, 17, 29
cycles / seed     = 200,000
burn-in / seed    = 50,000
observed ops      = 900,000 per policy
capacity C        = 254
minimum M         = 127
live keys K       = 5,690
```

The deterministic ensemble is expected to produce the following scale and ordering; CI verifies the hazard residuals and policy inequalities rather than pinning these Monte Carlo values as normative constants.

| Quantity | Left-first | Fuller-sibling |
|---|---:|---:|
| mean minimum leaves before delete | ~1.416 | ~1.141 |
| observed underflows | 14,151 | 11,357 |
| predicted underflows | ~14,224 | ~11,459 |
| underflow rate / operation | ~0.01572 | ~0.01262 |
| predicted underflow rate / operation | ~0.01580 | ~0.01273 |
| mean complete underflow holding ops | ~63.58 | ~79.23 |
| mean full leaves before insert | ~0.0120 | ~0.0071 |
| observed splits | 218 | 140 |
| predicted splits | ~240.8 | ~141.4 |

The underflow hazard is much better sampled than the split hazard. CI therefore requires underflow closure within 3% but allows 20% relative residual for the rarer split count in the quick ensemble.

## Policy mechanism

The key causal statement is narrower than "fuller sibling is cheaper":

```text
same conditional frontier law
+ different occupancy visitation
= different frontier arrival frequency
```

For underflow specifically:

```text
lambda_underflow
  = 0.5 * M / K * E[n_M before deletion]
```

so the policy difference can be written directly as:

```text
Delta lambda_underflow
  = 0.5 * M / K
    * Delta E[n_M before deletion]
```

No policy-specific fitted coefficient is needed.

That is precisely the kind of transition-law/visitation term anticipated by Experiment 0123's Markov-reward decomposition.

## What this does not solve

Experiment 0124 does not provide a complete stationary UCOF Markov chain.

It still leaves several distinct problems:

- deriving or estimating the stationary distribution of the global occupancy state;
- predicting frontier-conditioned sibling pairs from that state;
- composing borrow/merge decisions with parent/root structural context;
- propagating leaf-level restructuring into recursive internal-node rewards;
- bounding aggregation error when occupancy histograms are compressed;
- validating the resulting reward prediction against the real persistent Rust traces.

The important reduction is that these can now be addressed as separate conditional layers instead of forcing every variable into one raw leaf-triple state.

## Next model

A minimal next analytical model should represent global occupancy by the hazard-relevant masses plus enough surrounding occupancy information to predict their evolution, for example:

```text
mass at M
mass immediately above M
mass at C
leaf count / mean fill
frontier sibling-order statistic when underflow occurs
parent structural reward class
```

The model should then test whether it can reproduce:

1. Experiment 0124 underflow/split arrival rates;
2. Experiments 0116–0117 borrow/merge outcome frequencies;
3. Experiment 0123 expensive page-write visitation;
4. Experiment 0119 workload dependence in the real Rust implementation.

## Decision impact

The result strengthens the mechanistic case for keeping `FullerSiblingLeftTie` as the leading EXP-0003 candidate: in the deterministic random-key model it moves occupancy mass away from both the underflow and full-leaf frontiers, increasing the time between underflows and reducing split arrival.

It remains research evidence only.

`LeftFirst` remains the repository default. FCP-0003 acceptance, epoch allocation, authoritative-vector status, and wire-format stability remain unchanged.

## Reproduction

Quick CI ensemble:

```console
python3 tools/experiment_exp0003_frontier_hazard.py --quick
```

Longer evidence ensemble:

```console
python3 tools/experiment_exp0003_frontier_hazard.py
```
