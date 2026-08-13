# Experiment 0133 — EXP-0003 cross-level leaf-merge correlation

**Status:** non-normative research evidence  
**Date:** 2026-08-13  
**Related:** Experiments 0129–0132, issues #13, #16, #76

## Question

Experiment 0132 estimates recursive internal-repair frequency with a two-timescale closure:

```text
leaf-merge arrival rate
  × independently modeled internal-underflow probability per child removal.
```

That multiplication assumes the parent occupancy seen by a real leaf merge is adequately represented by a child-proportional random removal from the independently modeled internal process.

This experiment asks whether that cross-level selection assumption is actually safe.

## Coupled model

The diagnostic keeps an ordered list of leaf occupancies inside each actual parent internal node.

For one object operation:

- insertion chooses a leaf by insertion-gap weight;
- deletion chooses a leaf by stored-key weight;
- a leaf split inserts one child into the same parent;
- a leaf merge removes one child from the same parent;
- parent overflow is split immediately;
- parent underflow after a leaf merge is repaired using the selected candidate borrower policy.

Object count is conserved over each insert/delete cycle and every sampled state is required to satisfy the configured leaf and internal occupancy bounds.

At every realized leaf merge the experiment records whether the parent is exactly at its minimum occupancy before child removal. That condition means the leaf merge immediately causes recursive internal underflow.

The actual event indicator is therefore:

```text
I_recursive = 1(parent_occupancy_before_child_removal == internal_minimum)
```

At the same event instant, the child-proportional independence closure predicts:

```text
p_independent
  = internal_minimum * count(parents == internal_minimum)
    / total_leaf_count
```

The reported cross-level selection factor is:

```text
actual recursive share of leaf merges
-------------------------------------
mean child-proportional probability at those merge times
```

A factor of one would mean this particular parent-state statistic closes under the child-proportional approximation in the sampled ensemble. A factor different from one demonstrates cross-level selection bias.

## Geometry sensitivity

Two geometries are kept separate deliberately.

### Current Rust research microformat

```text
leaf:     C = 185, M = 93
internal: C = 255, M = 128
```

### First EXP-0003 Draft geometry

```text
leaf:     C = 254, M = 127
internal: C = 226, M = 113
```

Recent real-writer experiments use the first geometry, while the self-contained first EXP-0003 Draft proposes the second. This experiment must not silently transfer a correlation factor between them.

## Stress initialization

The quick CI ensemble uses seeds `3,17,29`, 80,000 cycles per seed and a 15,000-cycle burn-in.

The initial parent occupancies are intentionally frontier-heavy:

```text
[M, M, M, M, M+1, M+1, M+2, M+4]
```

Leaf occupancies are deterministic-seed draws with a center near the previous ~62% mixed-workload reference region.

This initialization is chosen to produce enough recursive events in bounded CI. It means the experiment is a **frontier-stress correlation diagnostic**, not a claim that the sampled distribution is a production or stationary UCOF workload.

## CI-reproduced result

The semantic head reproduced:

| Geometry | Policy | Leaf merges | Recursive internal underflows | Actual recursive share | Child-proportional share | Selection factor | Leaf merge / object op | Recursive underflow / object op |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Rust research | LeftFirst | 203 | 27 | 13.3005% | 18.3765% | 0.723778 | 0.000520513 | 0.000069231 |
| Rust research | FullerSiblingLeftTie | 107 | 13 | 12.1495% | 17.0313% | 0.713363 | 0.000274359 | 0.000033333 |
| first EXP-0003 Draft | LeftFirst | 75 | 6 | 8.0000% | 14.5950% | 0.548132 | 0.000192308 | 0.000015385 |
| first EXP-0003 Draft | FullerSiblingLeftTie | 47 | 9 | 19.1489% | 19.8718% | 0.963621 | 0.000120513 | 0.000023077 |

The current-Rust stress process therefore sees only about 71–72% as many recursive internal underflows as the child-proportional closure predicts at the same leaf-merge times.

The first-Draft geometry is more policy-sensitive in this finite sample: LeftFirst is about 55% of the independent prediction, while the fuller-sibling trajectory is close to the independent closure.

## Interpretation

The main result is **not** a correction coefficient.

It is a falsification of the stronger assumption that the two-timescale product can be treated as geometry- and policy-independent merely because both marginal processes are individually well behaved.

A real leaf merge is selected by a lower-level occupancy event. That event is coupled to the distribution and history of leaves inside its parent. Parent occupancy at leaf-merge arrival can therefore differ materially from the occupancy seen by a child-proportional random parent removal.

The result also demonstrates why current-Rust policy evidence cannot automatically determine proposed EXP-0003 policy frequencies: changing leaf/internal capacities changes both the lower-level event process and the parent-state selection law.

## Relation to Experiment 0132

Experiment 0132 remains useful as a reduced **structural-event-time** model. Its internal donor-cliff enrichment result does not depend on this experiment.

However, its object-operation bridge:

```text
leaf merge rate × independent internal structural frequency
```

must remain explicitly approximate. Experiment 0133 shows that cross-level conditioning can move the recursive-underflow factor materially under a deliberately discriminating workload.

The object-time estimates from Experiment 0132 must therefore not be promoted into normative cost claims without either:

1. a coupled long-run model whose cross-level distribution is validated; or
2. a real persistent depth-2 workload trace that directly measures recursive arrival frequency.

## What this does not prove

This experiment does not establish:

- stationary recursive-repair frequency;
- a universal multiplicative correction for Experiment 0132;
- production workload probabilities;
- that one borrower policy is universally cheaper;
- that the first EXP-0003 Draft geometry should be retained;
- an accepted FCP-0003 deletion rule.

The first-Draft event counts are particularly small, so the difference between the two policy factors is evidence of sensitivity, not a precise stationary estimate.

## Reproduction

Quick CI path:

```console
python3 tools/experiment_exp0003_cross_level_merge_correlation.py --quick
```

A larger deterministic ensemble is available without `--quick`.

The CI script also requires:

- exact object-count conservation;
- legal leaf/internal occupancy;
- leaf underflows to partition exactly into borrow or merge;
- recursive internal underflows to partition exactly into internal borrow or merge;
- recursive events in every geometry/policy aggregate;
- at least one aggregate to differ from the child-proportional closure by at least 15%.

## Next evidence

The next useful model should not add another independent marginal process. It should either converge this **coupled two-level system** toward a stable long-run occupancy distribution with convergence/error diagnostics, or instrument a validated persistent Rust tree to sample parent occupancy specifically at real leaf-merge arrivals.

That is the remaining bridge from local donor-policy mechanics to credible multi-level object-time cost prediction.
