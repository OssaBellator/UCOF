# EXP-0003 deletion borrower policy — decision packet

**Status:** review recommendation; no maintainer decision recorded  
**Date:** 2026-08-13  
**Related:** FCP-0003, issues #13, #16, #76, #10  
**Evidence window:** Experiments 0110–0134

## 1. Decision requested

FCP-0003 currently proposes deterministic persistent deletion repair that prefers a lendable left sibling before a lendable right sibling.

The repository now has enough implementation, byte-vector, workload, mathematical, and bounded-source evidence to stop expanding the deletion-policy experiment family and make an explicit review decision.

The two review alternatives are:

### A. Retain `LeftFirst`

When a non-root node underflows:

1. borrow from the left sibling when it can lend;
2. otherwise borrow from the right sibling when it can lend;
3. otherwise retain the existing deterministic merge fallback order.

### B. Revise to `FullerSiblingLeftTie`

When a non-root node underflows:

1. determine which immediate siblings can lend;
2. if exactly one can lend, borrow from it;
3. if both can lend, borrow from the sibling with greater occupancy;
4. if both have equal occupancy, borrow from the left sibling;
5. if neither can lend, retain the existing deterministic merge fallback order.

## 2. Recommendation

**Recommended review outcome: revise the proposed EXP-0003 borrower rule to `FullerSiblingLeftTie`, while preserving the current deterministic merge fallback order and left tie-break.**

This is a recommendation for an explicit maintainer decision. It does **not** accept FCP-0003, allocate `UCOF-EXP-0003`, change the current default Rust writer, or make the existing candidate vectors authoritative.

The recommendation rests primarily on an exact local balancing property and real persistent-writer evidence. The stochastic and reduced-state models support the direction but are deliberately not treated as proof of a universal performance advantage.

## 3. Exact policy delta

Let `M` be the non-root minimum occupancy after repair eligibility is evaluated. Let `L` and `R` be the occupancies of the immediate left and right siblings before borrowing.

A sibling can lend iff its occupancy is greater than `M`.

The two policies differ only when all of the following hold:

```text
L > M
R > M
R > L
```

In every other local borrow state they choose the same donor:

- no eligible donor: both proceed to the same merge class;
- exactly one eligible donor: both choose it;
- both eligible and `L > R`: both choose left;
- both eligible and `L == R`: both choose left because Fuller uses a left tie-break.

This small state difference is byte-significant because the selected donor page is rewritten and authenticated into the persistent successor.

## 4. Geometry-independent local balancing lemma

Consider the only divergent case, `R > L > M`.

After borrowing from the left sibling, the sibling occupancy pair is:

```text
LeftFirst: (L - 1, R)
```

After borrowing from the fuller right sibling, it is:

```text
Fuller:    (L, R - 1)
```

Both pairs have the same total occupancy `L + R - 1`.

Because `R > L`:

```text
min(L, R - 1) >= min(L - 1, R)
max(L, R - 1) <= max(L - 1, R)
```

with a strict increase in the minimum whenever the left donor would fall onto a lower occupancy boundary not also reached by the right donor.

For the most important half-full boundary:

```text
L = M + 1
R > M + 1
```

LeftFirst creates a new minimum-occupancy sibling by changing `M+1 -> M`, while Fuller leaves that sibling at `M+1` and takes one entry from the strictly fuller right sibling instead.

Thus, conditional on the same underflow frontier and same borrow class, Fuller weakly maximizes the minimum occupancy of the two siblings and weakly reduces their occupancy spread whenever the policies differ.

This statement does not depend on the current Rust capacities or the first EXP-0003 Draft capacities.

## 5. Immediate repair class and reward are policy-neutral at a fixed frontier

Experiments 0118, 0121, 0122, 0130, and 0131 establish an important separation:

- borrower policy can change **which** eligible sibling is rewritten;
- it does not change whether an eligible donor exists at that fixed local frontier;
- therefore it does not change borrow-versus-merge classification for the same sibling occupancies;
- the same structural borrow context has the same immediate page-write reward under either donor direction;
- persistent bytes and authenticated snapshot identity can nevertheless differ.

Experiment 0121 pins one exact `[94,93,101]` leaf fixture where both policies:

- delete the same object;
- end with the same logical active set;
- write 3 pages and reuse 1 page;
- produce different persistent file SHA-256 and snapshot digests;
- converge through `rewrite_all` to identical fresh canonical bytes.

Experiment 0130 provides the same separation one level above leaves. Starting from root-child occupancies:

```text
[129, 128, 255]
```

a leaf merge makes the middle internal node underflow. Both policies write 4 pages and preserve the same logical locator set, but their next internal occupancy states differ:

```text
LeftFirst             -> [128, 128, 255]
FullerSiblingLeftTie  -> [129, 128, 254]
```

The difference is transition state, not current repair-class reward.

## 6. Real persistent-writer evidence

### 6.1 Single long constant-set trace — Experiment 0114

A 96-cycle delete-and-reinsert trace restores the exact same 288-object logical set after every cycle.

Observed cumulative accounting:

```text
LeftFirst             418 pages written, 6,896,224 bytes appended
FullerSiblingLeftTie  387 pages written, 6,388,320 bytes appended
```

Fuller wrote 31 fewer pages, about 7.42% fewer, and appended about 7.36% fewer bytes in that trace. Both histories canonicalized to identical fresh bytes.

### 6.2 Five structured traces — Experiment 0119

Five 48-cycle constant-content traces deliberately include workloads where the policies are neutral.

Aggregate result:

```text
LeftFirst             1,004 pages written
FullerSiblingLeftTie    966 pages written
```

Fuller wrote 38 fewer pages, about 3.78% fewer, and appended about 3.76% fewer bytes.

The workload breakdown matters:

- whole-set LCG: Fuller materially lower;
- left-leaf hot: byte- and cost-identical;
- middle-leaf hot: byte-distinct but cost-identical;
- right-leaf hot: byte- and cost-identical;
- left/middle boundary hot: Fuller materially lower.

The repository therefore does **not** claim a universal percentage saving. The measured benefit appears when repeated operations expose asymmetric two-sided repair choices; several local workloads are neutral.

### 6.3 Exact reward decomposition — Experiment 0123

Across 480 operations per policy in the five-workload matrix:

```text
LeftFirst:  436 two-page + 44 three-page transitions = 1,004 pages
Fuller:     474 two-page +  6 three-page transitions =   966 pages
```

All 38 saved pages come from changed visitation of the expensive 3-page class.

For this finite-trace projection:

```text
visitation/history term = 38 pages
direct reward-map term  = 0 pages
```

Two of the avoided expensive transitions occur on later reinsertion even though insertion code is unchanged. That is direct implementation evidence that borrower choice changes future persistent-state cost rather than merely changing the immediate deletion.

## 7. Real frontier mechanism — Experiment 0128

The Rust trace matrix has a read-only inspector that uses the same borrower-selection helper as the writer.

Across 240 deletions per policy:

```text
                                      LeftFirst  Fuller
underflow frontiers                         44       6
borrow frontiers                            42       6
merge frontiers                              2       0
M+1 donor-cliff borrows                     41       0
locally avoidable donor-cliff borrows       21       0
```

For these depth-1 traces, borrow frontiers correspond operation-by-operation to the 3-page deletion class.

Thus 41 of 42 LeftFirst borrows drain a barely eligible `M+1` donor onto the minimum frontier. Twenty-one of those choices have another eligible sibling that is strictly fuller. Fuller records zero donor-cliff borrows in this matrix.

This is the concrete implementation mechanism behind the visitation result.

## 8. Exact internal geometry — Experiments 0130–0131

For current research internal geometry `M=128`, `C=255`, Experiment 0131 enumerates all 16,384 ordered sibling occupancy pairs after an internal node underflows.

Exact counts:

```text
merge states                         1
single-donor states                254
two-donor states                16,129
policy-divergent two-donor       8,001
avoidable Left M+1 donor cliffs    126
```

Nearly half of unweighted two-donor states select a different donor, but only about 0.7812% of the two-donor geometry changes minimum-frontier mass through the specific avoidable donor-cliff mechanism.

This distinction is important: generic donor disagreement is not a proxy for future hazard. Workload visitation must weight the narrower state subset.

## 9. Mathematical workload evidence

### 9.1 Mixed leaf restructuring — Experiment 0110

Under the first-Draft 16 KiB leaf geometry (`C=254`, `M=127`) in the documented five-seed mixed insert/delete ensemble:

```text
half-left-first       mean fill ~62.31%, restructure/op ~1.490%
half-fuller-sibling   mean fill ~61.85%, restructure/op ~1.252%
```

The model therefore reports about 15.9% lower leaf restructuring under Fuller while changing mean fill by less than half a percentage point.

This is stochastic leaf-level evidence, not a full-tree equilibrium proof.

### 9.2 Exact minimum-frontier hazard and flow — Experiments 0124–0126

For random-key deletion, the underflow-arrival law is exact for a fixed occupancy state:

```text
P(underflow on delete | state) = M * n_M / live_keys
```

where `n_M` is the number of minimum-occupancy leaves.

Experiment 0125 classifies every realized change in `n_M` and conserves it operation-by-operation. In its 900,000-operation-per-policy quick ensemble:

```text
borrow from M+1 donor:
  LeftFirst  1,305
  Fuller       267
```

LeftFirst has 879 donor-cliff events where another eligible sibling is strictly fuller; Fuller has zero by construction.

Experiment 0126 reduces the exact one-step expected `Delta n_M` reward to six summary counts. This is a one-step sufficient statistic, not a closed Markov-chain claim.

### 9.3 Fixed-point closure — Experiment 0127

A phase-blind zero-drift closure predicts the midpoint of pre-insert/pre-delete minimum mass to about 0.2% in the documented quick ensemble, while retaining about a 1% operation-phase residual.

This gives useful mechanism evidence but is not needed to justify the local policy rule.

## 10. Internal structural-time and cross-level uncertainty

Experiment 0132 weights internal donor-cliff geometry in an independently modeled internal structural process. It finds donor-cliff states are much more concentrated near the repair frontier than an unweighted census suggests.

Experiment 0133 then couples leaf split/merge events to their actual parent nodes and demonstrates that the conversion from leaf-merge rate to recursive internal-repair rate is **geometry- and policy-sensitive**.

In its deliberately frontier-heavy quick stress ensemble, actual recursive-underflow frequency divided by a child-proportional independent-parent closure is approximately:

```text
current Rust geometry:
  LeftFirst  0.724
  Fuller     0.713

first EXP-0003 Draft geometry:
  LeftFirst  0.548
  Fuller     0.964
```

These are not proposed correction constants. They demonstrate that object-time percentages from independent marginal models must not be promoted into universal cost claims.

This is a reason to give greater decision weight to exact local invariants and real writer measurements than to extrapolated stationary percentages.

## 11. Source-backed information cost — Experiment 0134

### 11.1 Why LeftFirst can read less in the current planner

The slice writer already loads both siblings before choosing a donor.

The current strongly-versioned source deletion planner can instead:

1. authenticate/load the left sibling;
2. if it can lend, perform the left borrow without loading the right sibling;
3. load the right sibling only when the left cannot lend.

A fuller-sibling comparison needs authenticated right occupancy whenever:

```text
left sibling exists and can lend
right sibling exists
```

unless that right-page information is already cached or otherwise authenticated.

### 11.2 Exact bounded-page cost

On the pinned `[94,93,101]` fixture, with the current source stress cap of 257 bytes per `read_exact_at` call, the existing LeftFirst source plan costs:

```text
15,096 read operations
3,661,003 bytes read
3,660,235 bytes hashed
30,202 strong-version checks
```

Authenticating the right 16 KiB sibling that Fuller needs adds exactly:

```text
64 bounded reads
16,384 bytes read
16,384 bytes hashed
128 strong-version checks
```

The marginal shares are about 0.42–0.45% of this current source-plan accounting, largely because the planner first strictly validates the source and does not cache those page reads into the planning pass.

Those percentages are **not** provider-cost claims.

### 11.3 General bounded-read expression

For page size `P` and source request cap `B`, an uncached additional sibling authentication under the current read/check pattern costs:

```text
reads          = ceil(P / B)
bytes read     = P
bytes hashed   = P
version checks = 2 * ceil(P / B)
```

If a maintained adapter permits one complete 16 KiB page per request, the logical information requirement is one additional page read, not 64 network round trips.

### 11.4 Frequency in the real trace matrix

The five-workload trace matrix now records the source-information opportunity condition.

On each policy's own observed trajectory:

```text
trace                         LeftFirst  Fuller
whole-set LCG                         8       1
left-leaf hot                         0       0
middle-leaf hot                       1       1
right-leaf hot                        0       0
left/middle boundary hot             12       1
TOTAL                                21       3
```

The condition occurs on `21 / 240 = 8.75%` of deletions on the measured LeftFirst trajectories.

In this matrix those 21 opportunities happen to coincide with the 21 locally avoidable donor-cliff events. That is not a mathematical identity for all valid trees.

Applying the 257-byte-cap page probe to all 21 LeftFirst-trajectory opportunities gives a one-step information **exposure** of:

```text
1,344 bounded reads
344,064 bytes read
344,064 bytes hashed
2,688 version checks
```

This must not be interpreted as the cost of switching the complete trace to Fuller. The policy changes persistent state after divergence, so later opportunities are endogenous to the selected policy.

## 12. Read/write cost should not be collapsed into raw bytes

The five real persistent traces show a 38-page write difference:

```text
38 * 16,384 = 622,592 page bytes
```

The same LeftFirst trajectories expose up to 344,064 bytes of additional uncached sibling-page reads under the 257-byte stress-cap calculation.

It would be incorrect to subtract those byte counts directly.

A deployment-aware incremental objective has the form:

```text
Delta C
  = p_extra_sibling * C_read_page
    - E[future_pages_saved] * C_write_page
```

where `C_read_page` includes request latency, transfer, hashing, version checks, caching, retry and provider behavior, while `C_write_page` includes append and durability cost.

Issue #10 remains responsible for maintained HTTP/cloud measurements. The byte-level deletion rule should not silently assume that the present research planner's caching or request-granularity strategy is permanent.

## 13. Why the recommendation favors Fuller despite the source-read downside

### 13.1 The local property is exact and geometry-independent

When the policies differ, Fuller preserves a higher minimum sibling occupancy and reduces the sibling occupancy spread. LeftFirst has no corresponding local occupancy advantage beyond enabling an earlier source-planner short-circuit.

### 13.2 Immediate structural cost is not increased in the implemented writer

At a fixed repair frontier, borrower direction does not change borrow-versus-merge class. The pinned leaf and internal fixtures show equal immediate page-write rewards while persistent next state differs.

### 13.3 Real persistent traces are neutral or favorable in the tested matrix

No structured trace in Experiment 0119 has a larger page-write count under Fuller. Some traces are exactly neutral; the asymmetric two-sided workloads are lower.

This is evidence, not a universal theorem, but it is stronger than relying only on a stochastic model.

### 13.4 The source downside is bounded and architectural

The current source planner can save one sibling-page authentication in some LeftFirst cases. That is a real cost and must be preserved in the review record.

However:

- it does not exist in the current slice writer because both siblings are already loaded;
- it can be one page request rather than 64 bounded calls when the adapter permits a full page;
- it can disappear when the sibling page is already authenticated/cached;
- its provider latency/cost remains an implementation/deployment property under #10;
- retaining LeftFirst would permanently encode the short-circuit preference into byte-significant mutation history even if future source planning has different caching/access behavior.

For a disposable interoperability epoch, the recommendation is therefore to select the policy with the stronger local occupancy property and measured write-side behavior, while keeping remote-source cost as a qualified implementation concern rather than making the present planner's short-circuit the normative tree rule.

## 14. Proposed normative algorithm if the recommendation is accepted

For every non-root node whose occupancy falls below the required minimum `M` after deletion or lower-level child removal:

```text
left_eligible  = left sibling exists  and left_occupancy  > M
right_eligible = right sibling exists and right_occupancy > M

if left_eligible and right_eligible:
    if left_occupancy >= right_occupancy:
        borrow one entry/child from left
    else:
        borrow one entry/child from right
else if left_eligible:
    borrow one entry/child from left
else if right_eligible:
    borrow one entry/child from right
else:
    apply the accepted deterministic merge fallback order
```

The left tie-break is normative and prevents implementation-dependent choice.

The same borrower rule should apply at leaf and internal levels unless a later accepted FCP explicitly distinguishes them.

## 15. What does not change under this recommendation

The recommendation does not by itself change:

- the half-full minimum occupancy proposal;
- split policy;
- merge fallback direction/order;
- root exceptions;
- batch canonicalization;
- object identifier width/scope;
- page size or entry widths;
- catalog/capability semantics;
- history/recovery assurance boundaries;
- source version/freshness semantics;
- any permanent registry assignment.

## 16. Required work after an explicit maintainer decision

If `FullerSiblingLeftTie` is accepted for the proposed EXP-0003 experiment:

1. update FCP-0003's deletion repair clause;
2. update `spec/experimental/UCOF-EXP-0003.md` to the same exact rule;
3. keep existing LeftFirst research outputs explicitly historical/non-authoritative;
4. migrate the reference writer's intended EXP-0003 path from default LeftFirst to the accepted rule only when the epoch byte package is being aligned;
5. add a policy-aware source-planning path that produces the accepted bytes under bounded reads/version checks;
6. independently implement the accepted rule for interoperability evidence;
7. regenerate authoritative EXP-0003 deletion, recursive repair, mixed-batch, history, and recovery vectors after the final page/entry geometry is fixed;
8. keep provider-level read-cost qualification under #10 rather than treating current bounded calls as provider round trips.

If `LeftFirst` is retained instead, the disposition should explicitly record the trade-off:

- simpler/short-circuit source planning is being preferred over Fuller's local sibling-balancing property and the observed write-visitation evidence;
- Fuller candidate vectors and experiments remain rejected-alternative evidence rather than silently disappearing.

## 17. Why not defer the byte rule indefinitely to provider benchmarks

Real provider measurements matter for operational performance, but deletion borrower choice is a deterministic byte-significant transition rule needed before authoritative EXP-0003 mutation vectors can exist.

Issue #10 is also a Phase 3 exit gate, not a reason to leave every byte-level policy ambiguous until a production adapter exists.

The current evidence is sufficient to make a reviewable experimental-epoch choice while preserving an explicit compatibility non-promise. If later provider evidence reveals a severe operational flaw, EXP-0003 remains disposable and the finding can justify another incompatible experimental epoch.

## 18. Evidence weighting used by this recommendation

From highest to lowest decision weight:

1. **exact structural facts** — eligibility partition, local balancing lemma, policy-neutral borrow/merge class;
2. **real implementation facts** — byte divergence, equal immediate reward fixtures, persistent trace page/write accounting, frontier instrumentation, bounded source accounting;
3. **deterministic enumerations** — exact internal state census and reward-preserving partitions;
4. **stochastic/reduced-state models** — occupancy flow, hazards, fixed points, structural-time weighting;
5. **cross-level extrapolations** — useful sensitivity evidence only, explicitly not universal rates.

The recommendation does not depend on trusting the least certain layer.

## 19. Maintainer disposition block

No box is checked by this packet.

- [ ] **Retain LeftFirst** for the proposed EXP-0003 experiment.
- [ ] **Revise to FullerSiblingLeftTie** for the proposed EXP-0003 experiment.
- [ ] **Defer** with a named blocking measurement and explicit reason why the existing evidence is insufficient.

When a maintainer decision is made, the disposition should record:

- the selected rule;
- the principal accepted trade-off;
- whether provider measurements are a pre-allocation or Phase-3-exit requirement;
- the exact FCP/spec/vector edits required;
- compatibility and migration non-promises for earlier research bytes.

## 20. Boundary

Merging this review packet would mean only that the deletion-policy evidence has been consolidated into a decision-ready recommendation.

It would **not**:

- accept the recommendation;
- accept FCP-0003;
- allocate `UCOF-EXP-0003`;
- change current default mutation bytes;
- make candidate review vectors authoritative;
- resolve #13 or #16;
- remove #10, #11, or #12 as Phase 3 gates.

The purpose is to stop evidence accumulation from becoming a substitute for a governance decision.
