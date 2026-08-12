# Experiment 0114: EXP-0003 deletion-policy constant-set trace

- **Status:** Reproducible Rust sequence evidence; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0110 and 0112
- **Executable:** `crates/ucof-experiments/examples/exp0003_delete_policy_trace.rs`

## Question

Experiment 0110 found a lower restructuring rate for fuller-eligible-sibling borrowing in a leaf-level stochastic model. Experiment 0112 then proved that the alternative maps cleanly onto the real recursive Rust persistent deletion implementation and is byte-significant for one exact operation.

Does the difference accumulate in a real persistent mutation sequence when the **logical active set is held constant**?

This experiment is designed so that every cycle returns to exactly the same current logical state. Therefore differences in page writes, page reuse, file growth, and persistent bytes are transition-history effects rather than changes in logical cardinality or payload content.

## Policies

The two persistent deletion policies are:

### Current/default

```text
LeftFirst
```

At underflow:

```text
borrow left if eligible
otherwise borrow right if eligible
otherwise merge left when possible
otherwise merge right
```

### Experimental

```text
FullerSiblingLeftTie
```

At underflow:

```text
if both siblings can lend:
    borrow from the fuller sibling
    use left on an exact occupancy tie
otherwise borrow from the eligible sibling
otherwise keep the same left-first merge fallback
```

Both use the same half-full minimum, split behavior, recursive repair, and persistent insertion implementation.

## Constant-set trace construction

The trace begins from the exact comparison fixture introduced by Experiment 0112.

The active set is:

```text
ObjectId 92 ..= 379
count = 288
```

The fixture has three successor-microformat leaves:

```text
[94, 93, 101]
```

The trace runs 96 cycles.

Each cycle does exactly:

```text
1. delete one selected active ObjectId using the policy under test
2. reinsert that exact ObjectId with the same kind and payload
```

So after every complete cycle:

```text
current logical ObjectId set = 92..=379
payload mapping              = unchanged
object count                 = 288
```

Cycle 0 deletes ObjectId 186, forcing the known both-siblings-can-lend case from Experiment 0112. Later cycles choose an ObjectId from the same fixed active set with a deterministic LCG.

The two traces therefore receive the same logical operation sequence.

## Why this is a stronger comparison

A conventional insert/delete benchmark lets occupancy and cardinality drift, which makes it hard to separate policy effects from workload evolution.

Here the active logical state is reset after every pair of operations. Persistent history still evolves, but **logical content does not**.

Consequently:

```text
file-size difference
page-write difference
page-reuse difference
snapshot/root difference
```

are direct measurements of alternative persistent transition histories for equal current content.

## Reproduced results

The Rust CI trace produced:

| Policy | Delete pages written | Insert pages written | Total pages written | Delete pages reused | Insert pages reused | Total pages reused | Bytes appended | Final file bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| left-first | 224 | 194 | 418 | 63 | 285 | 348 | 6,896,224 | 10,279,819 |
| fuller-sibling | 195 | 192 | 387 | 92 | 289 | 381 | 6,388,320 | 9,771,915 |

The fuller-sibling trace therefore used:

```text
31 fewer page writes
= 7.42% fewer than left-first

507,904 fewer appended bytes
= 7.36% less append growth

33 more page reuses
= 9.48% more than left-first
```

The largest difference is in deletion itself:

```text
left-first delete pages written      = 224
fuller-sibling delete pages written  = 195
reduction                            = 29 pages
                                    = 12.95%
```

Reinsertion page writes differ only slightly:

```text
left-first insert pages written      = 194
fuller-sibling insert pages written  = 192
```

So most of the cumulative saving in this trace comes from the deletion repair history rather than from a substantially cheaper insertion path.

## Persistent states diverge immediately and remain divergent

The trace reports:

```text
delete_divergent_cycles = 96 / 96
cycle_divergent_cycles  = 96 / 96
```

Every deletion result differs byte-for-byte between policies, and the persistent files remain different after every delete/reinsert cycle.

File length after complete cycles was:

```text
fuller-sibling smaller = 95 cycles
left-first smaller     = 0 cycles
equal length           = 1 cycle
```

The equal-length cycle does not imply equal bytes; the persistent states are still byte-distinct.

This demonstrates that sibling selection is not a transient one-operation detail. It changes the retained physical history and that difference persists across later equal-content operations.

## Canonical current content remains identical

At the end of the 96 cycles, each trace is passed through `rewrite_all`.

The executable requires:

```text
left_fresh.retained_object_ids == fuller_fresh.retained_object_ids
left_fresh.bytes               == fuller_fresh.bytes
```

Both equal the original 288-object active set.

Therefore the measured file-growth and page-write difference does **not** come from different logical content. It is entirely a persistent-transition representation effect.

This is a concrete implementation-level instance of scoped determinism:

> two files can represent the same current UCOF object set, canonicalize to identical fresh bytes, and yet have different valid persistent histories, sizes, roots, and snapshot identities.

## Interpretation

### Fuller-sibling borrowing is no longer only a Monte Carlo hypothesis

Experiment 0110 predicted lower restructuring for fuller-sibling borrowing. Experiment 0114 observes lower cumulative page writes and file growth in the real Rust persistent writer for a deterministic constant-set sequence.

The two experiments use different geometries and workloads, yet point in the same direction.

That convergence is stronger evidence than either result alone.

### The result is still not a universal proof

The trace is intentionally controlled and small enough to be deterministic/reproducible in CI.

It does not establish that fuller-sibling borrowing is better for every workload. In particular, it does not yet cover:

- broad random mixed-operation traces across many seeds;
- clustered or append-heavy keys;
- adversarial update sequences;
- large depth-two/depth-three recurring underflow;
- batch mutations;
- source-backed planning;
- compact EXP-0003 geometry;
- different page sizes.

A policy should not be frozen from one 96-cycle trace.

### The current default remains important for compatibility

Existing successor vectors and behavior are pinned to left-first. Experiment 0112 already proves the default API remains byte-identical to explicit `LeftFirst`.

Experiment 0114 is evidence for the future EXP-0003 transition rule, not authorization to rewrite historical EXP-0002/successor evidence.

## Mathematical implication

The trace gives exactly the kind of empirical quantities needed to validate a more formal fringe/Markov model:

```text
borrow probability
merge probability
split probability
pages written per operation
pages reused per operation
retained-byte growth per operation
```

A first-order independent-neighbor mean-field model is unlikely to be enough because sibling occupancies are correlated by previous split/borrow/merge operations.

The appropriate next state model should preserve at least local tuples such as:

```text
(left occupancy, target occupancy, right occupancy)
```

near the underflow frontier, with transition rewards for immutable pages written and parent-boundary changes.

The model can then compare:

```text
LeftFirst
FullerSiblingLeftTie
```

under the same insertion/deletion distribution and validate predicted transition frequencies against Rust traces like this one.

## Decision impact for issue #16

The evidence chain is now:

1. **Experiment 0110:** fuller-sibling borrowing lowers modeled leaf restructuring in a mixed stochastic workload;
2. **Experiment 0112:** the rule maps cleanly to the recursive Rust implementation and is byte-significant while preserving logical content;
3. **Experiment 0114:** a constant-current-set Rust trace shows 7.42% fewer cumulative page writes and 7.36% less append growth over 96 cycles.

That is sufficient to keep `FullerSiblingLeftTie` as a serious EXP-0003 review candidate.

Before changing the normative transition rule, the remaining evidence should include:

- a broader deterministic trace matrix over depths/workloads;
- transition/fringe analysis or another correlated-state mathematical model;
- authoritative candidate mutation vectors for both policies so reviewers can inspect exact byte consequences;
- explicit maintainer selection in FCP-0003.

Until then, repository default behavior remains `LeftFirst`.

## Reproduction

```console
cargo run --locked -p ucof-experiments --example exp0003_delete_policy_trace
```

The same command is executed by the normal Rust CI workflow.
