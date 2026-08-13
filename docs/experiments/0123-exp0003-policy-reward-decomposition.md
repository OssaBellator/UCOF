# Experiment 0123: EXP-0003 policy page-write reward decomposition

- **Status:** Pinned finite-trace Markov-reward diagnostic; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0119–0122
- **Trace:** `crates/ucof-experiments/examples/exp0003_delete_policy_trace_matrix.rs`
- **Verifier:** `tools/verify_exp0003_policy_reward_decomposition.py`
- **Pinned manifest:** `tests/vectors/exp-0003-policy-reward-decomposition/manifest.csv`

## Question

Experiment 0119 found 38 fewer page writes for `FullerSiblingLeftTie` across five structured constant-content workloads, but that aggregate does not say **why**.

Is the alternative policy cheaper because:

1. the same structural transition has a different page-write reward; or
2. the policy changes persistent history so that later operations visit expensive structural states less often?

Experiments 0121 and 0122 already suggest the second mechanism is important:

- the minimal byte-significant borrower-choice fixture writes three pages under either policy;
- depth-1 left and right borrow have the same three-page implementation reward;
- parent/root context, not borrower direction by itself, determines the page-write reward.

Experiment 0123 instruments the real Rust trace matrix with per-operation page-write histograms and decomposes the observed finite-trace reward difference.

## Markov-reward framing

For a stationary Markov reward process with transition law `P`, stationary distribution `pi(P)`, and reward vector `r`, the long-run reward is conventionally written

```text
J(P,r) = pi(P) r
```

A policy change can therefore affect reward through two conceptually different routes:

```text
changed state visitation / stationary mass
changed reward attached to states or transitions
```

A useful finite-trace analogue replaces the stationary distribution by the observed empirical class frequencies:

```text
J_hat = sum_k mu_hat[k] r[k]
```

This experiment uses that empirical identity only. It does **not** claim that the 48-cycle workloads are stationary samples or that their empirical frequencies are the stationary distribution of a complete UCOF Markov chain.

The decomposition is nevertheless valuable because it checks whether the implementation evidence has the same architecture expected from a future Markov reward model.

## Instrumentation

For every one of the five Experiment 0119 workloads and each policy, the Rust trace now records separate histograms for:

```text
delete pages_written
insert pages_written
```

Each workload contains 48 delete/reinsert cycles, therefore:

```text
48 deletions + 48 insertions = 96 operations per workload
5 workloads                  = 480 operations per policy
```

The Rust executable asserts that every histogram contains exactly 48 operations for its operation/workload cell and that the weighted histogram reconstructs the previously published aggregate page totals.

The independent verifier reruns the same Rust executable, extracts the histogram rows, and requires an exact byte-for-byte match with the checked-in CSV.

## Exact histogram

The complete pinned manifest is:

```text
trace,policy,operation,pages_written,count
whole-set-lcg,left-first,delete,2,30
whole-set-lcg,left-first,delete,3,18
whole-set-lcg,left-first,insert,2,46
whole-set-lcg,left-first,insert,3,2
whole-set-lcg,fuller-sibling,delete,2,45
whole-set-lcg,fuller-sibling,delete,3,3
whole-set-lcg,fuller-sibling,insert,2,48
left-leaf-hot,left-first,delete,2,48
left-leaf-hot,left-first,insert,2,48
left-leaf-hot,fuller-sibling,delete,2,48
left-leaf-hot,fuller-sibling,insert,2,48
middle-leaf-hot,left-first,delete,2,47
middle-leaf-hot,left-first,delete,3,1
middle-leaf-hot,left-first,insert,2,48
middle-leaf-hot,fuller-sibling,delete,2,47
middle-leaf-hot,fuller-sibling,delete,3,1
middle-leaf-hot,fuller-sibling,insert,2,48
right-leaf-hot,left-first,delete,2,48
right-leaf-hot,left-first,insert,2,48
right-leaf-hot,fuller-sibling,delete,2,48
right-leaf-hot,fuller-sibling,insert,2,48
left-middle-boundary-hot,left-first,delete,2,25
left-middle-boundary-hot,left-first,delete,3,23
left-middle-boundary-hot,left-first,insert,2,48
left-middle-boundary-hot,fuller-sibling,delete,2,46
left-middle-boundary-hot,fuller-sibling,delete,3,2
left-middle-boundary-hot,fuller-sibling,insert,2,48
```

No 1-page, 4-page, or other page-write class occurs in these five depth-1 constant-content traces.

## Aggregate reward classes

Across all 480 operations under each policy:

### Left-first

```text
2-page transitions = 436
3-page transitions =  44

pages = 436*2 + 44*3
      = 1,004

mean pages / operation
      = 1,004 / 480
      = 2.091666667
```

### Fuller-sibling

```text
2-page transitions = 474
3-page transitions =   6

pages = 474*2 + 6*3
      = 966

mean pages / operation
      = 966 / 480
      = 2.0125
```

Therefore:

```text
page saving = 1,004 - 966 = 38 pages
```

which reproduces Experiment 0119 exactly.

## The reward decomposition is exact for this empirical class model

Both policies use the same observed reward map:

```text
r(2-page class) = 2
r(3-page class) = 3
```

There is no policy-specific reward value attached to a class.

For `N=480` operations, write the reward as a two-page baseline plus one extra page whenever the expensive class is visited:

```text
pages = 2N + count(3-page transitions)
```

Then:

```text
left-first = 960 + 44 = 1,004
fuller     = 960 +  6 =   966
```

and therefore:

```text
visitation-frequency term = 44 - 6 = 38 pages saved
direct reward-map term    = 0 pages
total observed saving     = 38 pages
```

So **100% of the page-write advantage in this finite-trace two-class decomposition is explained by changed visitation frequency of the same expensive reward class**.

This is the implementation-level behavior that the future stationary Markov reward model needs to reproduce.

## Most of the difference occurs on deletion, but history also changes reinsertion

Break the 3-page transitions down by operation:

### Deletion

```text
left-first      = 42 expensive deletions
fuller-sibling  =  6 expensive deletions
saving          = 36 pages
```

### Reinsertion

```text
left-first      = 2 expensive insertions
fuller-sibling  = 0 expensive insertions
saving          = 2 pages
```

Thus:

```text
36 / 38 = 94.74% of the page saving occurs directly on deletion
 2 / 38 =  5.26% appears on the following insertion histories
```

That second term is small but conceptually important.

The policies run the **same logical delete/reinsert operation sequence**. The different borrower choice changes the persistent tree state, which can then change the cost class of a later insertion even though insertion policy itself is unchanged.

This is direct evidence for the perturbation/visitation interpretation:

> changing a deletion transition rule changes future state visitation and therefore future rewards, including rewards of operations whose own algorithm did not change.

## Workload decomposition

The expensive 3-page transition count by workload is:

| Workload | Left-first | Fuller-sibling | Difference |
|---|---:|---:|---:|
| whole-set LCG | 20 | 3 | 17 |
| left-leaf hot | 0 | 0 | 0 |
| middle-leaf hot | 1 | 1 | 0 |
| right-leaf hot | 0 | 0 | 0 |
| left/middle boundary hot | 23 | 2 | 21 |
| **Total** | **44** | **6** | **38** |

This reconstructs every Experiment 0119 page delta:

```text
whole-set saving        = 17 pages
left-hot saving          =  0 pages
middle-hot saving        =  0 pages
right-hot saving         =  0 pages
boundary-hot saving      = 21 pages
```

The neutral workloads are therefore not hiding offsetting positive/negative reward changes. They simply visit the same page-write classes with the same frequencies under both policies.

## Byte-growth consequence

Experiment 0122 established the current deletion microformat's affine page-to-byte reward, and Experiment 0119 uses identical operation counts and payloads under both policies.

For the policy comparison, fixed per-transition tails and identical payload contributions cancel, leaving the observed byte difference equal to the page-write difference times page size:

```text
38 * 16,384 = 622,592 bytes
```

which exactly matches the Experiment 0119 aggregate append-growth difference:

```text
16,568,816 - 15,946,224 = 622,592
```

Likewise:

```text
whole-set: 17 * 16,384 = 278,528 bytes
boundary:  21 * 16,384 = 344,064 bytes
```

matching those workload-level byte deltas exactly.

For these equal-operation constant-content comparisons, the future model can therefore target **page-write reward** first and derive the policy byte-growth delta mechanically.

## Relation to perturbation theory

The eventual analytical target is a stationary Markov reward model rather than this empirical histogram.

The useful policy-difference decomposition is conceptually:

```text
J_F - J_L
    = (pi_F - pi_L) r_L
      + pi_F (r_F - r_L)
```

where the first term represents changed stationary visitation caused by the transition-law perturbation and the second represents a changed reward mapping.

Equivalent decompositions can use the other policy as the reference reward vector.

Experiment 0123 says that, for the observed two-class page-write projection of these Rust traces:

```text
empirical visitation term = all 38 saved pages
empirical direct reward-map term = 0
```

That does not prove the direct reward term is zero in a richer state space. Parent/root context from Experiment 0122 remains part of the true reward state, and a policy can change the distribution of that context.

Instead, it validates the design principle:

```text
model transition/state visitation explicitly;
do not encode "fuller sibling is cheaper" as a reward constant.
```

## Next model

The next mathematical model should combine:

1. Experiment 0120's policy-aware frontier aggregate state;
2. Experiment 0122's parent/root structural reward classes;
3. transition probabilities estimated or derived for those aggregate states;
4. a stationary distribution or recurrent reward solution;
5. a perturbation decomposition of the predicted policy delta;
6. explicit aggregation/error residuals;
7. validation against this histogram and the cumulative Experiments 0114/0119 traces.

A successful model should reproduce not only the sign of the fuller-sibling advantage but the workload dependence:

```text
zero on left/right hot traces
zero despite byte divergence on middle-hot
positive on whole-set and boundary-hot
```

## Decision impact

Experiment 0123 strengthens the case for `FullerSiblingLeftTie` as an EXP-0003 candidate while narrowing the causal claim.

The observed benefit is not that a fuller-sibling borrow intrinsically writes fewer pages than a left borrow. The minimal candidate vector and parent reward catalog show equal immediate borrow reward in the same structural context.

The benefit in the measured workloads comes from **changing persistent history so expensive structural transitions are visited less often**.

That is a stronger modeling basis but still not a normative policy selection.

`LeftFirst` remains the repository default. FCP-0003, epoch allocation, authoritative-vector status, and wire-format stability remain unchanged.

## Reproduction

Verify the pinned histogram and decomposition:

```console
python3 tools/verify_exp0003_policy_reward_decomposition.py
```

The verifier reruns the Rust trace matrix, requires an exact manifest match, and checks every arithmetic/decomposition identity above.
