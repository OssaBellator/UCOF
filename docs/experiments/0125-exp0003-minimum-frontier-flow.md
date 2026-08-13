# Experiment 0125: EXP-0003 minimum-frontier flow balance

- **Status:** deterministic finite-horizon flow diagnostic; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0116–0124
- **Tool:** `tools/experiment_exp0003_minimum_frontier_flow.py`

## Question

Experiment 0124 established an exact arrival law for deletion underflow in the fixed-cardinality random-key/random-gap process:

```text
lambda_underflow / operation
  = 0.5 * M / K * E[n_M before deletion]
```

where `n_M` is the number of leaves at the minimum legal occupancy `M`.

The fuller-sibling policy has lower `E[n_M]` and therefore a lower underflow arrival rate. Experiment 0125 asks the next causal question:

> Which concrete operations create and remove minimum-occupancy leaves, and which of those flows is changed directly by borrower choice?

## Exact minimum-mass flow classes

For the current EXP-0003 leaf geometry:

```text
capacity C = 254
minimum  M = 127
```

only five operation classes can change `n_M`.

### 1. Insertion from a minimum leaf

```text
M -> M+1
Delta n_M = -1
```

### 2. Ordinary deletion into the minimum

```text
M+1 -> M
Delta n_M = +1
```

### 3. Borrow from a barely eligible donor

An underflowed target begins the deletion at `M`, falls to `M-1`, and is restored to `M` by the borrow. That target therefore contributes no net minimum-mass change.

If the chosen donor was exactly `M+1`, however:

```text
donor: M+1 -> M
Delta n_M = +1
```

This is the policy-sensitive **donor cliff**.

### 4. Split a full leaf

The deterministic split is:

```text
C + 1 = 255 entries
-> 128 + 127
-> (M+1) + M

Delta n_M = +1
```

### 5. Merge two minimum siblings

A merge is reached only when the target started at `M` and no sibling can lend. Every existing sibling is legal before deletion, so the selected merge sibling is also exactly `M`.

After deleting one entry from the target:

```text
(M-1) + M -> 2M-1 = 253

Delta n_M = -2
```

## Conservation assertion

The executable measures `n_M` before and after **every observed operation** and asserts that the actual change equals the event-class contribution above.

It then requires the whole observed interval to telescope exactly:

```text
n_M(final) - n_M(initial)
  = delete_to_M
    + borrow_M+1_donor_to_M
    + splits
    - insert_from_M
    - 2 * merges
```

This makes the decomposition a conservation identity rather than a post-hoc correlation.

## Deterministic quick ensemble

The CI ensemble matches Experiment 0124:

```text
seeds             = 3, 17, 29
cycles / seed     = 200,000
burn-in / seed    = 50,000
observed ops      = 900,000 per policy
live keys K       = 5,690
```

### Exact aggregate flow

| Flow | Left-first | Fuller-sibling | `Delta n_M` sign |
|---|---:|---:|---:|
| insertion from `M` | 14,659 | 11,627 | -1 each |
| ordinary deletion `M+1 -> M` | 13,562 | 11,499 | +1 each |
| borrow donor `M+1 -> M` | **1,305** | **267** | +1 each |
| split creates `M` leaf | 218 | 140 | +1 each |
| merge events | 216 | 140 | -2 each |
| merge minimum mass removed | 432 | 280 | negative |

The left-first balance is:

```text
+13,562
+ 1,305
+   218
-14,659
-   432
--------
     -6
```

and the summed seed endpoints are also:

```text
n_M(final) - n_M(initial) = 0 - 6 = -6
```

The fuller-sibling balance is:

```text
+11,499
+   267
+   140
-11,627
-   280
--------
     -1
```

matching its summed endpoint change exactly:

```text
4 - 5 = -1
```

## The borrower-specific mechanism

Total borrow events are:

```text
left-first     = 13,935
fuller-sibling = 11,217
```

Borrow events that drain an `M+1` donor to `M` are:

```text
left-first     = 1,305
fuller-sibling =   267
```

Therefore the donor-cliff share of borrows is:

```text
left-first     = 1,305 / 13,935 = 9.365%
fuller-sibling =   267 / 11,217 = 2.380%
```

and the total number of donor-cliff events is 79.5% lower in the fuller-sibling trace.

This is important because a borrow from a donor above `M+1` does **not** create a new minimum leaf. The two borrow directions can have the same immediate immutable page-write reward while leaving very different future underflow hazard behind them.

## Locally avoidable donor cliffs

For every borrow that chooses an `M+1` donor, the tool also checks whether the other sibling was eligible and strictly fuller.

Under left-first:

```text
M+1 donor with a strictly fuller eligible alternative = 879
M+1 donor without a strictly fuller alternative       = 426
```

Thus:

```text
879 / 1,305 = 67.36%
```

of left-first's donor-cliff events occurred in a microstate where the fuller-sibling rule would choose the other donor.

Under `FullerSiblingLeftTie`:

```text
M+1 donor with a strictly fuller eligible alternative = 0
M+1 donor without a strictly fuller alternative       = 267
```

The zero is not a fitted result; it follows from the policy rule and is asserted by the executable.

The remaining `M+1` donor cases are unavoidable under the local choice rule: there is no strictly fuller eligible alternative (including equal/tied cases).

Because the two policies generate different histories, the `879` count must **not** be interpreted as an additive counterfactual decomposition of the total 1,038-event difference. It is instead a direct classification of left-first microstates where the alternative policy would make a different immediate donor choice.

## Connecting Experiments 0123–0125

The evidence chain now has three separately testable links.

### Experiment 0125: borrower choice changes frontier-mass flow

```text
left-first more often drains M+1 donors
-> more new minimum leaves are created by borrow
```

### Experiment 0124: minimum mass controls underflow arrival exactly

```text
lambda_underflow
  = 0.5 * M / K * E[n_M before deletion]
```

The quick ensemble has:

```text
E[n_M] left-first     = 1.416202222
E[n_M] fuller-sibling = 1.140926667
```

and correspondingly lower underflow arrival under fuller-sibling.

### Experiment 0123: changed visitation controls immutable reward

In the real persistent Rust trace matrix, both policies share the same observed two-class page-write reward map, but fuller-sibling visits the expensive 3-page class only 6 times instead of 44.

The combined causal architecture is therefore:

```text
borrower choice
  -> occupancy-frontier flow
  -> underflow/split arrival frequency
  -> structural-state visitation
  -> immutable page-write reward
```

No policy-specific "fuller is cheaper" reward constant is required.

## Reduced-state implication

A useful occupancy model no longer needs raw leaf triples everywhere. For the minimum frontier, the drift of `n_M` can be written in terms of:

```text
mass at M
mass at M+1
mass at C
leaf count
conditional probability that an underflow borrow uses an M+1 donor
conditional merge probability
```

The first four terms drive exact random-key/random-gap selection hazards. The last two are frontier-conditioned local quantities and are where sibling correlation/policy enters.

This suggests a tractable next model: derive and validate the expected drift equation for `n_M` and solve its near-stationary fixed point using measured or reduced-state conditional frontier terms.

## What this does not establish

Experiment 0125 does not prove a stationary distribution, global Markov property, or normative borrower policy.

In particular:

- the two policy traces have different histories;
- counts from one trace cannot be treated as paired counterfactual events in the other;
- parent/internal-node state is still needed for full immutable reward;
- workload-local Rust traces and the random-key leaf model are distinct evidence surfaces;
- external review/independent implementation remains outstanding.

## Decision impact

The experiment strengthens the mechanistic case for `FullerSiblingLeftTie` as the leading EXP-0003 candidate.

The policy's advantage is now traceable to a concrete state transition: it avoids draining barely eligible donors when a fuller lender is available, thereby creating fewer new minimum-occupancy leaves and reducing future underflow hazard.

This remains research evidence only. `LeftFirst` stays the repository default. FCP-0003 acceptance, epoch allocation, authoritative-vector status, and wire-format stability remain unchanged.

## Reproduction

Quick CI ensemble:

```console
python3 tools/experiment_exp0003_minimum_frontier_flow.py --quick
```

Longer evidence ensemble:

```console
python3 tools/experiment_exp0003_minimum_frontier_flow.py
```
