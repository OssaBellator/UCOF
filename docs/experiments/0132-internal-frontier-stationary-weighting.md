# Experiment 0132 — Workload-weighted internal donor-cliff frontier

## Question

Experiment 0131 identified the exact local internal deletion geometry for the current research fanout (`M = 128`, `C = 255`): among 16,129 two-donor sibling states, 126 make unconditional `LeftFirst` select an `M+1` left donor even though the right donor is strictly fuller. Uniform state-space weighting gives those avoidable donor cliffs a share of only `126 / 16,129 = 0.7812015624%`.

This experiment asks a different question: **what weight do those 126 states receive under a stationary structural workload?**

The goal is to distinguish local combinatorial rarity from workload visitation frequency before any deletion-policy clause is frozen into EXP-0003 bytes.

## Method

The experiment uses a reduced parent-level child-count process rather than a full object writer.

Current research geometry:

```text
internal minimum M = 128
internal capacity C = 255
```

The initial state contains 32 internal nodes at approximately 70% fill. One structural cycle contains exactly:

1. one child insertion, representing a lower-level split; and
2. one child removal, representing a lower-level merge;

with the order randomized each cycle. The total number of child references therefore remains fixed while parent nodes split, borrow, merge, and move through the occupancy frontier.

Two arrival kernels test sensitivity to the parent-selection assumption:

- **child-proportional** — structural insertions and removals choose a parent in proportion to current child count;
- **gap-insert** — removals remain child-proportional while insertions use `occupancy + 1`, mirroring the gap weighting used in the leaf-level workload experiments.

Both `LeftFirst` and `FullerSiblingLeftTie` use the same half-full floor and merge direction. Only borrower selection differs.

The deterministic CI ensemble uses seeds `3,17,29`, 200,000 structural cycles per seed, and a 50,000-cycle burn-in. That yields 450,000 observed structural child removals for each kernel/policy pair.

## CI-reproduced structural-time results

| Kernel | Policy | Internal underflow / structural delete | Two-donor share of underflows | Generic donor divergence / two-donor | Avoidable left `M+1` cliff / two-donor | Enrichment vs uniform 0.7812% | Selected donor cliffs / borrows | Mean internal `n_M` before delete |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| child-proportional | LeftFirst | 3.3022% | 73.7012% | 45.7359% | **8.7107%** | **11.15×** | 9.8265% | 1.4715 |
| child-proportional | FullerSiblingLeftTie | 2.6387% | 85.5735% | 47.2198% | 4.9700% | 6.36× | 2.2751% | 1.1622 |
| gap-insert | LeftFirst | 3.1951% | 73.0282% | 42.8381% | **8.1714%** | **10.46×** | 9.1071% | 1.4093 |
| gap-insert | FullerSiblingLeftTie | 2.6656% | 82.0509% | 48.4556% | 6.0455% | 7.74× | 2.9919% | 1.1798 |

The exact CI counts are:

```text
child-proportional / LeftFirst
  structural deletes:              450000
  internal underflows:              14860
  borrows / merges:              14522 / 338
  one / two / zero donor:         3570 / 10952 / 338
  generic divergence opportunities: 5009
  avoidable left-cliff opportunities: 954
  selected donor cliffs:             1427
  selected avoidable left cliffs:     954

child-proportional / FullerSiblingLeftTie
  structural deletes:              450000
  internal underflows:              11874
  borrows / merges:              11692 / 182
  one / two / zero donor:         1531 / 10161 / 182
  generic divergence opportunities: 4798
  avoidable left-cliff opportunities: 505
  selected donor cliffs:              266
  selected avoidable left cliffs:       0

gap-insert / LeftFirst
  structural deletes:              450000
  internal underflows:              14378
  borrows / merges:              14077 / 301
  one / two / zero donor:         3577 / 10500 / 301
  generic divergence opportunities: 4498
  avoidable left-cliff opportunities: 858
  selected donor cliffs:             1282
  selected avoidable left cliffs:     858

gap-insert / FullerSiblingLeftTie
  structural deletes:              450000
  internal underflows:              11995
  borrows / merges:              11765 / 230
  one / two / zero donor:         1923 / 9842 / 230
  generic divergence opportunities: 4769
  avoidable left-cliff opportunities: 595
  selected donor cliffs:              352
  selected avoidable left cliffs:       0
```

## Result 1: uniform geometry badly understates boundary visitation

Experiment 0131's unweighted state-space share is only 0.7812%, but the stationary structural process places **8.17–8.71%** of LeftFirst two-donor underflows in the avoidable `(left=M+1, right>left)` set: roughly **10.5–11.2 times** the uniform weight.

The Fuller trajectory also visits such local configurations more often than uniform geometry predicts, but its borrower rule does not select the smaller `M+1` donor when a strictly fuller donor exists. Consequently `selected_avoidable_left_cliffs` is zero for Fuller in both kernels.

This demonstrates that combinatorial state-space frequency is not a usable proxy for repair-frontier visitation. Boundary states are dynamically enriched.

## Result 2: generic donor disagreement remains the wrong hazard variable

Generic donor-selection disagreement occurs in roughly 43–48% of realized two-donor internal underflows. The avoidable `M+1` donor-cliff subset is much narrower, roughly 5–9% depending on policy trajectory and arrival kernel.

That preserves the Experiment 0131 modeling conclusion: **donor-choice divergence itself is not the renewal hazard.** The useful transition statistic is the occupancy-sensitive donor-cliff opportunity, together with internal minimum-frontier mass.

## Result 3: Fuller changes the internal transition process, not the current repair reward class

Across both arrival kernels, FullerSiblingLeftTie has lower:

- mean internal minimum mass before deletion;
- internal-underflow rate per structural child removal; and
- selected donor-cliff share of borrows.

The immediate borrow-versus-merge reward class at a fixed local frontier remains policy-neutral, as established by Experiment 0131. The policy effect appears through changed next-state occupancy and therefore changed later visitation.

This is the same transition-versus-reward separation that Experiments 0128–0130 established against the persistent Rust writer.

## Two-timescale object-operation estimate

The structural-time process deliberately does not model ordinary object operations. To get an order-of-magnitude bridge, the experiment separately imports the observed leaf-merge rate from Experiment 0126:

```text
LeftFirst             leaf merge rate = 216 / 900000 = 0.0002400000 per object operation
FullerSiblingLeftTie  leaf merge rate = 140 / 900000 = 0.0001555556 per object operation
```

Multiplying those leaf-merge rates by the structural-time frequencies gives the following **closure estimates**, not direct writer measurements:

| Quantity | LeftFirst | FullerSiblingLeftTie |
|---|---:|---:|
| estimated internal underflow / object op | 7.67e-6 to 7.93e-6 | 4.10e-6 to 4.15e-6 |
| approximate spacing | one per 126k–130k ops | one per 241k–244k ops |
| estimated avoidable left-cliff opportunity / object op | 4.58e-7 to 5.09e-7 | 1.75e-7 to 2.06e-7 |
| approximate spacing | one per 1.97m–2.19m ops | one per 4.86m–5.73m ops |
| estimated selected donor cliff / object op | 6.84e-7 to 7.61e-7 | 9.20e-8 to 1.22e-7 |
| approximate spacing | one per 1.31m–1.46m ops | one per 8.22m–10.88m ops |

These estimates support two simultaneous conclusions:

1. internal donor-cliff dynamics are real and materially enriched **conditional on reaching an internal repair frontier**; and
2. recursive internal repairs remain rare in ordinary object-operation time, so the observed leaf-level visitation difference remains the primary source of current page-write savings.

## Modeling consequence

A reduced multi-level model should therefore keep these concepts separate:

```text
leaf minimum mass
  -> leaf merge arrival rate
  -> parent/internal structural event arrival
  -> internal minimum mass + donor-cliff opportunity
  -> recursive repair visitation
  -> page-write reward
```

The policy affects both leaf and internal transition kernels. Immediate reward is conditioned on the repair class; long-run reward differences arise because policy changes which states are visited later.

## Critical limitation: cross-level correlation is still unresolved

The object-operation conversion above multiplies a leaf-merge rate by an independently simulated internal structural-time distribution. That assumes, for this diagnostic, that the occupancy of the parent receiving a child removal is adequately represented by the stationary structural process.

The experiment **does not measure** whether real leaf merges are disproportionately routed into minimum, `M+1`, or otherwise unusual internal parents. A positive or negative cross-level correlation could shift the absolute recursive-underflow rates.

That is now the most specific remaining modeling uncertainty.

## Other limitations

- This is a reduced occupancy process, not a persistent-byte writer trace.
- The quick CI ensemble is finite and uses deterministic seeds; it is not a stationary proof.
- The two arrival kernels are sensitivity probes, not claims about a universal workload distribution.
- The leaf calibration comes from the current research geometry and workload model, not an accepted final EXP-0003 layout.
- Root exceptions and arbitrary-depth cascades are not modeled as a full tree population.
- No FCP-0003 disposition, default borrower policy, page geometry, authoritative vector, epoch allocation, or wire-format status changes here.

## Next experiment

The next useful evidence is not another unweighted or independently simulated frontier model. It should test the cross-level closure directly: **condition parent/internal occupancy on an actual lower-level merge in a real or structurally coupled tree trajectory**, and compare that conditional distribution with the stationary structural-time distribution used here.

Only after that correlation is bounded should the object-operation estimates above be treated as more than an order-of-magnitude bridge.
