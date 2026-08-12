# Experiment 0110: EXP-0003 mixed leaf restructuring

- **Status:** Reproducible stochastic evidence
- **Date:** 2026-08-13
- **Related:** FCP-0003, issues #16 and #76, Experiment 0109
- **Script:** `tools/experiment_exp0003_mixed_leaf_restructuring.py`

## Question

The current EXP-0003 occupancy companion proposes a half-full non-root floor and byte-significant deletion repair:

```text
left borrow -> right borrow -> left merge -> right merge
```

What does that policy buy in leaf occupancy, and what does it cost in immutable-page restructuring under a balanced insert/delete workload?

A second question is whether the unconditional left-first **borrow** preference is doing useful work, or whether a deterministic fuller-sibling preference can reduce restructuring while retaining a precise tie-break.

This experiment is a leaf-level stochastic stress model. It is not an equilibrium proof, not a full B+tree simulator, and not a format decision.

## Why measure restructuring explicitly?

The classical utilization objective is incomplete for immutable UCOF pages.

Johnson and Shasha analytically studied B-trees under mixed insert/delete workloads and found that deletion policy changes both utilization and restructuring frequency; in their model, merge-at-half produced somewhat better utilization but much more restructuring than free-at-empty:

- Theodore Johnson and Dennis Shasha, *B-trees with inserts and deletes: Why free-at-empty is better than merge-at-half*, Journal of Computer and System Sciences 47(1), 45–76 (1993), DOI `10.1016/0022-0000(93)90020-W`.

Their exact percentages are **not** transplanted into UCOF. Their tree, workload, and in-place cost model differ. The useful lesson is the objective: occupancy must be evaluated together with structural-change rate.

For UCOF this concern is stronger because a borrow rewrites an unchanged sibling into a new immutable page, while a split or merge also changes the parent child set and can recurse upward.

Fringe analysis provides a more formal route beyond Monte Carlo. Eisenbarth et al. model lower B-tree states with transition matrices and derive expected split quantities:

- Friedrich Eisenbarth, Nivio Ziviani, Gaston Gonnet, Kurt Mehlhorn, and Derick Wood, *The theory of fringe analysis and its application to 2-3 trees and B-trees*, Information and Control 55 (1982), 125–174, DOI `10.1016/S0019-9958(82)90534-4`.

Experiment 0110 is deliberately a simpler precursor: establish the important state variables and candidate policies before building a full transition/fringe model.

## Exact UCOF rule retained

The simulation uses the current first-Draft 16 KiB leaf geometry:

```text
page size       = 16,384
page header     = 80
leaf locator    = 64
leaf capacity C = 254
half-full M     = 127
```

Insertion overflow uses the proposed byte-significant split exactly:

```text
255 -> 128, 127
```

For the `half-left-first` policy, deletion repair is exactly the proposed leaf order:

1. borrow left if the left sibling can remain at or above `M`;
2. otherwise borrow right;
3. otherwise merge left when present;
4. otherwise merge right.

Recursive internal repair is outside this experiment.

## Workload model

Each trial begins with 32 leaves at approximately 70% fill. For this geometry that is 5,690 live keys.

One cycle contains exactly:

- one insertion; and
- one deletion;

with randomized operation order. The live key count therefore returns to exactly 5,690 after every cycle.

### Insertion target

A leaf containing `n` keys has `n + 1` insertion gaps. The model chooses a target leaf proportional to `n + 1`.

This is the leaf-level marginal of uniformly choosing one insertion gap from the active ordered key set.

### Deletion target

A leaf containing `n` keys has `n` deletable keys. The model chooses a target leaf proportional to `n`.

This is the leaf-level marginal of uniformly choosing one existing key for deletion.

The weighted choices use rejection sampling so no dynamic weighted-index implementation affects the model.

## Compared policies

### `free-empty-reference`

No positive occupancy floor is enforced in this leaf-only reference. A leaf is removed only when deletion makes it empty.

This is inspired by the Johnson-Shasha comparison but is **not** claimed to reproduce their complete B-tree policy or stationary distribution.

### `quarter-left-first`

Repair below

```text
ceil(C / 4) = 64
```

using the same UCOF left-borrow/right-borrow/left-merge/right-merge ordering.

This is a sensitivity point, not a proposed standard.

### `half-left-first`

The current proposed EXP-0003 rule:

```text
ceil(C / 2) = 127
```

with unconditional left-first borrowing.

### `half-fullest-borrow`

Keep the same half-full floor and the same merge rule, but change **only lender selection**:

```text
if both siblings can lend:
    borrow from the fuller sibling
    use left only as the exact tie-break
else:
    borrow from the eligible sibling
```

Merge direction remains left-first. This isolates the effect of lender choice rather than redesigning the whole repair algorithm.

## Evidence configuration

The recorded full ensemble uses:

```text
seeds             = 3, 17, 29, 43, 71
cycles / seed     = 800,000
burn-in / seed    = 200,000 cycles
operations/cycle  = 2
```

The script also exposes `--quick` for a shorter deterministic CI ensemble.

Because balanced occupancy processes can mix slowly, especially at larger capacities, the reported values are finite-horizon ensemble estimates. Standard deviation across seeds is included for fill and leaf count.

## Results

| Policy | Mean fill | Fill stddev | Split/op | Borrow/op | Merge/free per op | Restructure/op | Leaf pages emitted/op |
|---|---:|---:|---:|---:|---:|---:|---:|
| free-empty reference | 0.36975 | 0.00792 | 0.0000275 | 0 | 0.00000588 | 0.0000334 | 1.00002 |
| quarter, left-first | 0.45291 | 0.00585 | 0.0000246 | 0.0038348 | 0.0000126 | 0.0038720 | 1.00386 |
| half, left-first | 0.62310 | 0.00211 | 0.0002201 | 0.0144613 | 0.0002174 | 0.0148988 | 1.01468 |
| half, fullest eligible borrower | 0.61851 | 0.00659 | 0.0001339 | 0.0122581 | 0.0001319 | 0.0125239 | 1.01239 |

`Restructure/op` counts leaf split, borrow, merge, or free events. It deliberately does not count ordinary one-page replacement as restructuring.

### Half-full buys substantial density

In this stress envelope, the current half-full rule keeps mean leaf utilization near 62%, compared with roughly 45% for the quarter-floor sensitivity point.

That is a real space/locality benefit.

### It also increases immutable restructuring

The current half-left-first rule restructures a leaf on roughly 1.49% of operations in this finite-horizon ensemble, versus roughly 0.39% for the quarter-floor sensitivity point.

At leaf level alone, the half-left-first policy emits approximately

```text
1.014681 * 16,384 ~= 16,625 bytes/operation
```

of new leaf pages, before counting parent/path pages that persistent UCOF must also emit.

This lower-bound byte difference looks small for one leaf operation, but borrow/merge/split events are precisely the cases most likely to cause extra parent work or recursive repair.

## Finding: unconditional left-first borrowing creates a directional bias

For the current half-left-first rule:

```text
left borrow rate  ~= 0.0128053 / op
right borrow rate ~= 0.0016560 / op
left share        ~= 88.5% of borrows
```

Some left bias is expected because boundary leaves have asymmetric neighbors, but the magnitude here is primarily induced by policy: whenever both siblings can lend, left wins before their occupancy is compared.

The fullest-eligible alternative produces:

```text
left borrow rate  ~= 0.0063939 / op
right borrow rate ~= 0.0058643 / op
left share        ~= 52.2% of borrows
```

while remaining deterministic through a left tie-break.

This matters because a byte-significant directional preference is not free: it changes which sibling page is rewritten and therefore changes future occupancy, split history, page identity, and snapshot identity.

## Finding: fuller-sibling borrowing is a serious review candidate

Holding the half-full floor and merge direction constant, fuller-sibling borrowing changes this ensemble from:

```text
half-left-first       restructure/op ~= 0.0148988
half-fullest-borrow   restructure/op ~= 0.0125239
```

or about a **15.9% reduction** in modeled leaf restructuring.

It also reduces modeled split and merge rates by roughly 39% each in this particular ensemble, with a modest mean-fill reduction from about 62.31% to 61.85%.

These are stochastic results, not a theorem. But they are large enough that the current unconditional left-first borrow rule should not be frozen merely because it is already implemented.

## Important limitations

### This is leaf-only

The model does not simulate:

- internal node occupancy;
- parent separator/reference rewrites;
- recursive internal borrow/merge;
- root collapse/growth;
- byte offsets/digests;
- page reuse from a real persistent history;
- catalog or snapshot interaction.

Consequently, emitted-page and restructuring cost is a **lower bound / directional indicator**, not a full persistent-write estimate.

### This is finite-horizon Monte Carlo

The reported fill values are not claimed stationary occupancies. Balanced birth/death processes can mix slowly, and policy changes alter leaf population itself.

The fixed seeds make the evidence reproducible, not exact.

### Random key/gap workload is only one regime

Real workloads can be clustered, append-heavy, delete-heavy, batched, or semantically correlated. The model should eventually grow workload families rather than trying to make one random process universal.

## Implication for #16

Issue #16 should now review **two separable decisions**:

1. the non-root occupancy floor; and
2. deterministic sibling selection during repair.

The current evidence does not justify replacing half-full occupancy. It does justify reopening unconditional left-first borrowing before authoritative EXP-0003 mutation vectors make it expensive to change.

A reasonable next review candidate is:

```text
half-full minimum
fuller eligible sibling borrows
left tie-break
left merge when no sibling can lend
```

That preserves a precise deterministic transition while using local occupancy information to reduce restructuring in this model.

## Next mathematical step

Replace the leaf Monte Carlo with a fringe/mean-field model that tracks occupancy-state populations and structural transitions. The state vector should include at least:

```text
leaf occupancy distribution
sibling occupancy pairs near the repair frontier
split rate
left/right borrow rate
left/right merge rate
leaf-count drift
parent-boundary changes
```

A transition-matrix or mean-field model would provide expected rates without waiting for slow Monte Carlo mixing and would scale naturally to the 4/16/64 KiB capacities from Experiment 0109.

## Related new research direction: history-independent partitioning

Bender, Farach-Colton, Goodrich, and Komlós published a history-independent dynamic-partitioning primitive in 2026 and apply it to B-trees:

- Michael A. Bender, Martín Farach-Colton, Michael T. Goodrich, and Hanna Komlós, *History-Independent Dynamic Partitioning with Applications to B-Trees, Skip Lists and Fusion Trees*, ACM Transactions on Database Systems, DOI `10.1145/3810240` (2026).

This is directly relevant to FCP-0003's unresolved distinction between canonical fresh-rewrite identity and history-sensitive persistent transition identity. It does **not** automatically fit UCOF: UCOF also wants immutable page reuse, authenticated page identity, bounded range reads, and deterministic cross-language bytes.

But the result changes the research question from “history-independent dynamic partitioning is probably too expensive” to the more useful question:

> can a history-independent partition rule be adapted to authenticated immutable pages while retaining enough physical reuse to beat canonical full rewrite?

That deserves a separate experiment before scoped determinism is frozen.

## Reproduction

Full recorded ensemble:

```console
python3 tools/experiment_exp0003_mixed_leaf_restructuring.py
```

Short deterministic CI ensemble:

```console
python3 tools/experiment_exp0003_mixed_leaf_restructuring.py --quick
```

Machine-readable output:

```console
python3 tools/experiment_exp0003_mixed_leaf_restructuring.py --json
```
