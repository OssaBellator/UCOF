# Experiment 0119: EXP-0003 deletion-policy constant-set trace matrix

- **Status:** Reproducible Rust workload-matrix evidence; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0110, 0112, 0114, and 0116–0118
- **Executable:** `crates/ucof-experiments/examples/exp0003_delete_policy_trace_matrix.rs`

## Question

Experiment 0114 showed lower page-write and append growth for `FullerSiblingLeftTie` over one deterministic 96-cycle delete/reinsert sequence.

Does that advantage survive different locality patterns, or was it an artifact of one whole-set LCG trace?

Experiment 0119 broadens the real Rust persistent evidence while retaining the strongest control from Experiment 0114: **every cycle restores exactly the same logical current object set and payload mapping**.

Therefore any difference in persistent bytes, page writes, page reuse, or retained file size remains a transition-history effect rather than a content difference.

## Fixture

All traces begin from the same exact three-leaf comparison fixture used by Experiments 0112 and 0114.

The active logical set is:

```text
ObjectId 92 ..= 379
count = 288
```

The initial leaf occupancies are the comparison geometry produced by that fixture, and both policies receive the same ObjectId sequence within each workload.

Every cycle performs:

```text
1. delete one selected active ObjectId
2. reinsert that exact ObjectId with the same kind and payload
```

Each trace contains 48 complete cycles.

## Policies

### Current/default

```text
LeftFirst
```

### Experimental

```text
FullerSiblingLeftTie
```

Only eligible-sibling selection differs. The half-full minimum, merge fallback, insertion implementation, recursive repair machinery, and persistent format remain the same.

## Workload matrix

Five deterministic traces exercise different locality patterns:

| Trace | Selection pool | Purpose |
|---|---|---|
| `whole-set-lcg` | `92..=379` | broad whole-set recurrence |
| `left-leaf-hot` | `92..=185` | concentrated left-side updates |
| `middle-leaf-hot` | `186..=278` | concentrated middle updates |
| `right-leaf-hot` | `279..=379` | concentrated right-side updates |
| `left-middle-boundary-hot` | `176..=195` | repeated pressure around the first leaf boundary |

Each uses its own fixed LCG seed. The first selected ObjectId is explicit so important comparison states are reached reproducibly.

## CI result

The Rust CI reproduction reported:

| Trace | Policy | Delete pages written | Insert pages written | Total pages written | Delete pages reused | Insert pages reused | Total pages reused | Bytes appended | Final file bytes |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| whole-set-lcg | left-first | 114 | 98 | 212 | 76 | 94 | 170 | 3,497,264 | 6,880,859 |
| whole-set-lcg | fuller-sibling | 99 | 96 | 195 | 93 | 96 | 189 | 3,218,736 | 6,602,331 |
| left-leaf-hot | left-first | 96 | 96 | 192 | 96 | 96 | 192 | 3,169,584 | 6,553,179 |
| left-leaf-hot | fuller-sibling | 96 | 96 | 192 | 96 | 96 | 192 | 3,169,584 | 6,553,179 |
| middle-leaf-hot | left-first | 97 | 96 | 193 | 95 | 96 | 191 | 3,185,968 | 6,569,563 |
| middle-leaf-hot | fuller-sibling | 97 | 96 | 193 | 95 | 96 | 191 | 3,185,968 | 6,569,563 |
| right-leaf-hot | left-first | 96 | 96 | 192 | 96 | 96 | 192 | 3,169,584 | 6,553,179 |
| right-leaf-hot | fuller-sibling | 96 | 96 | 192 | 96 | 96 | 192 | 3,169,584 | 6,553,179 |
| left-middle-boundary-hot | left-first | 119 | 96 | 215 | 73 | 96 | 169 | 3,546,416 | 6,930,011 |
| left-middle-boundary-hot | fuller-sibling | 98 | 96 | 194 | 94 | 96 | 190 | 3,202,352 | 6,585,947 |

Across the five controlled traces:

```text
left-first total pages written      = 1,004
fuller-sibling total pages written  =   966
reduction                           =    38 pages
                                    =  3.78%

left-first bytes appended           = 16,568,816
fuller-sibling bytes appended       = 15,946,224
reduction                           =    622,592 bytes
                                    =  3.76%
```

Those aggregate percentages are descriptive only: the five traces are deliberately structured evidence cases, not samples from a claimed production workload distribution.

## Result by workload

### Whole-set recurrence: fuller-sibling is cheaper

For `whole-set-lcg`:

```text
pages written:
    left-first      212
    fuller          195
    reduction        17 = 8.02%

bytes appended:
    left-first      3,497,264
    fuller          3,218,736
    reduction         278,528 = 7.96%
```

Fuller-sibling had the smaller retained file after 47 of 48 complete cycles; one cycle had equal size.

Both persistent histories were byte-distinct after every deletion and every complete cycle.

### Left-leaf locality: policies are exactly identical

For `left-leaf-hot`, every measured value is equal and the two persistent histories never diverge:

```text
delete divergent cycles = 0 / 48
cycle divergent cycles  = 0 / 48
```

This is an important negative result.

The experimental policy has no inherent cost advantage when the operation sequence never creates a decision where its different borrower rule matters.

### Middle-leaf locality: bytes diverge but cost is equal

For `middle-leaf-hot`, the persistent bytes differ after every deletion and every complete cycle:

```text
delete divergent cycles = 48 / 48
cycle divergent cycles  = 48 / 48
```

Yet page-write, page-reuse, append-byte, and final-size metrics are exactly equal between the two policies.

This separates two concepts that should not be conflated:

```text
byte-significant transition choice != automatic write-amplification difference
```

A policy can choose a different valid persistent representation while incurring the same immediate cumulative cost under a workload.

### Right-leaf locality: policies are exactly identical

`right-leaf-hot` is also byte-identical and metric-identical for all 48 cycles.

As with the left-hot trace, no relevant two-sided borrower choice is exposed by this controlled sequence.

### Boundary locality: strongest advantage in this matrix

For `left-middle-boundary-hot`:

```text
pages written:
    left-first      215
    fuller          194
    reduction        21 = 9.77%

bytes appended:
    left-first      3,546,416
    fuller          3,202,352
    reduction         344,064 = 9.70%
```

Fuller-sibling had the smaller retained file after 46 of 48 complete cycles; two cycles had equal size.

Both histories were byte-distinct after every deletion and complete cycle.

## Canonical logical equivalence

At the end of every trace, both persistent histories are passed through `rewrite_all`.

The executable requires:

```text
retained ObjectIds equal the original 92..=379 set
left fresh bytes == fuller fresh bytes
```

CI reported:

```text
canonical_fresh_bytes_equal_for_all_traces=1
```

So no measured difference is caused by different current logical content.

## Interpretation

Experiment 0119 weakens any simplistic claim that fuller-sibling is uniformly cheaper.

The result is more specific:

- when the workload does not expose a borrower-choice distinction, both policies can be exactly identical;
- when borrower choice changes persistent bytes but not restructuring count, costs can remain equal;
- when repeated updates expose asymmetric two-sided repair opportunities, fuller-sibling can materially reduce deletion page writes and append growth.

That pattern is consistent with Experiments 0116–0118: the important mathematical state is concentrated at the underflow frontier and, for fuller-sibling, in the relative sibling occupancy/order information.

It also explains why a policy comparison should use a Markov **reward** model rather than only a root-identity or occupancy model. The same byte divergence can correspond to either zero or material write-cost divergence depending on the transition path.

## Relation to Experiment 0114

Experiment 0114 used 96 cycles over a whole-set deterministic sequence and found:

```text
7.42% fewer cumulative page writes
7.36% less append growth
```

for fuller-sibling.

The 48-cycle `whole-set-lcg` case in this matrix independently shows a similar directional magnitude:

```text
8.02% fewer page writes
7.96% less append growth
```

The boundary-hot case is stronger, while three leaf-local cases are cost-neutral.

This makes workload locality and underflow-frontier exposure explicit variables in the policy decision rather than hidden properties of one trace.

## Decision impact for issue #16

The evidence now supports keeping `FullerSiblingLeftTie` as a serious EXP-0003 candidate, but **not** adopting the stronger statement that it is universally cheaper.

A normative choice should next combine:

1. the broader Rust workload evidence here;
2. the frontier-conditioned state reductions from Experiments 0116–0118;
3. a multi-step Markov reward / approximate-lumpability model with explicit transition and reward residuals;
4. deeper-tree and batch traces;
5. authoritative candidate byte vectors;
6. explicit maintainer/FCP disposition.

The default remains `LeftFirst` until that review occurs.

## Reproduction

```console
cargo run --locked -p ucof-experiments --example exp0003_delete_policy_trace_matrix
```

The same command runs in the normal Rust CI workflow.
