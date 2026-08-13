# Experiment 0121: EXP-0003 deletion-policy candidate review vectors

- **Status:** Pinned non-normative candidate review evidence
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0112, 0114, 0119, and 0120
- **Generator:** `crates/ucof-experiments/examples/exp0003_delete_policy_candidate_vectors.rs`
- **Pinned manifest:** `tests/vectors/exp-0003-candidate-delete-policy/manifest.txt`

## Question

The EXP-0003 deletion-policy review now has stochastic evidence, workload traces, and a reduced mathematical state model. What exact persistent-byte consequences should a reviewer compare for the smallest known fixture where `LeftFirst` and `FullerSiblingLeftTie` choose different lenders?

Experiment 0121 pins that comparison without promoting either result to an authoritative epoch vector.

## Fixture recipe

The fixture is deterministic:

```text
1. build genesis with ObjectIds 1..=370
2. insert ObjectIds 371..=379
3. delete ObjectIds 1..=91 using LeftFirst
4. delete ObjectId 186 under each candidate policy
```

Before the candidate deletion, the active set is `92..=379` with 288 objects. This is the exact three-leaf comparison fixture used by Experiments 0112 and 0114, where the target leaf is at the half-full minimum and both neighbors can lend, with the right sibling fuller.

The pinned fixture is:

```text
bytes           = 3,383,595
SHA-256         = 575718b8d12a3656aed3c554f65984b3bd9e8c6edc5857832014afbeed338c36
sequence        = 100
snapshot digest = 28116935a8b906b92f06e7affb2bbdfd7d9b03163e48238187ae1f20975ac7d2
objects         = 288
```

## Candidate outputs

### Current/default `LeftFirst`

```text
bytes           = 3,432,971
SHA-256         = 6682f5235386c4db6735c27552d0400500a7297ccbe2590b3ebe99f30048f7cc
sequence        = 101
snapshot digest = db0d6dbfcad5bb831705f445851a915ef5eb2c2d4fab79d61f3b58dfe7dbdf9a
objects         = 287
page count      = 4
root level      = 1
pages written   = 3
pages reused    = 1
```

### Experimental `FullerSiblingLeftTie`

```text
bytes           = 3,432,971
SHA-256         = 91e1fb6875283fc4201362bc88b5959f917bf29a63bdb9bc4b0fdfe5cf367647
sequence        = 101
snapshot digest = 90ba980006739205b954fd9d7342ad42b81840c00080e79d9b58263395773f06
objects         = 287
page count      = 4
root level      = 1
pages written   = 3
pages reused    = 1
```

## What differs and what does not

For this single operation, both policies have identical immediate structural/write metrics:

```text
output length   equal
sequence        equal
object count    equal
page count      equal
root level      equal
pages written   equal
pages reused    equal
```

But they are not the same persistent state:

```text
full-file SHA-256       different
snapshot/root digest    different
persistent bytes        different
```

This is a useful review fixture precisely because it separates **authenticated representation choice** from immediate page-count cost. A byte-significant transition rule does not automatically imply a cheaper or more expensive single operation.

That distinction also matches Experiment 0119, whose middle-leaf-hot workload produced persistent byte divergence with equal cumulative write/reuse/size metrics.

## Canonical logical convergence

Each candidate output is passed through `rewrite_all`.

Both produce the same fresh canonical result:

```text
fresh bytes       = 63,503
fresh SHA-256      = 8329209fdfc55c118ad1d039116cfff2fe427958ccc12a0818abda336011b164
retained objects   = 287
```

The generator asserts:

```text
persistent_outputs_equal      = 0
snapshot_digests_equal        = 0
canonical_fresh_bytes_equal   = 1
```

Therefore the policy difference changes persistent transition history and authenticated snapshot identity without changing the current logical object set.

## Why pin a manifest instead of full multi-megabyte files

The candidate outputs are each more than 3.4 MiB and are mechanically reproducible from a short recipe. Checking two large binary files into the repository would add little independent evidence.

Instead, the repository pins:

- the exact fixture recipe;
- fixture length, SHA-256, sequence, and snapshot digest;
- each candidate output length, SHA-256, sequence, snapshot digest, and structural/write metrics;
- the shared canonical fresh length and SHA-256.

CI regenerates the complete byte streams with the Rust implementation and requires the rendered manifest to match the checked-in text exactly.

This makes any future candidate-byte drift explicit while keeping the evidence compact and reviewable.

## Relation to authoritative vectors

These are **not** authoritative successor or EXP-0002 vectors.

The manifest is deliberately labeled:

```text
status=non-normative-candidate-review-evidence
```

Its purpose is to make the exact consequences of the unresolved EXP-0003 borrower decision inspectable before any normative selection.

If maintainers later select a deletion policy and allocate an epoch, authoritative candidate bytes should be regenerated under that epoch's complete frozen geometry and rules rather than renaming this evidence in place.

## Decision impact

Experiment 0121 closes one evidence gap identified in issue #16: reviewers now have exact pinned candidate identities for a minimal byte-significant deletion-policy divergence.

It does not close the policy decision itself. Remaining decision evidence still includes the parent/rewrite reward model, broader/deeper-tree and batch behavior, and explicit maintainer/FCP disposition.

The repository default remains `LeftFirst`.

## Reproduction

Print the generated manifest:

```console
cargo run --locked -p ucof-experiments --example exp0003_delete_policy_candidate_vectors
```

Verify the checked-in pin:

```console
cargo run --locked -p ucof-experiments --example exp0003_delete_policy_candidate_vectors -- --verify tests/vectors/exp-0003-candidate-delete-policy/manifest.txt
```
