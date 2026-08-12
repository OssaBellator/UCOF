# Experiment 0112: EXP-0003 fuller-sibling persistent deletion prototype

- **Status:** Reusable Rust prototype evidence; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0110 and 0111
- **Implementation:** `crates/ucof-experiments/src/immutable_successor/persistent_delete.rs`
- **Test:** `crates/ucof-experiments/tests/immutable_successor_persistent_delete.rs`

## Question

Experiment 0110 found that, under its finite-horizon mixed insert/delete leaf model, changing only borrower selection from unconditional left-first to

```text
borrow from the fuller eligible sibling
left on an exact tie
```

reduced modeled leaf restructuring by about 15.9% while retaining the same half-full floor and deterministic merge direction.

Does that alternative map cleanly onto the repository's **real recursive persistent deletion implementation**, and is the choice actually byte-significant once page digests, parent references, snapshots, and canonical validation are involved?

## Boundary

This experiment does **not** change the default deletion algorithm.

The existing APIs remain left-first:

```text
append_persistent_delete(...)
append_persistent_batch(... Delete ...)
```

A separate comparison API is added:

```text
append_persistent_delete_experimental(
    data,
    object_id,
    limits,
    ExperimentalDeleteBorrowPolicy::{
        LeftFirst,
        FullerSiblingLeftTie,
    },
)
```

`LeftFirst` is required by test to be exactly equal to the existing default result.

The experimental alternative changes only which eligible sibling lends when both can lend. It does **not** change:

- the half-full minimum;
- split policy;
- left-first merge fallback;
- recursive underflow rules;
- root-collapse rules;
- validation rules; or
- the current default public behavior.

## Why test the existing successor microformat first?

The current reusable Rust successor experiment still uses its established `UCOFIM02` geometry:

```text
page size           = 16,384 bytes
page header         = 64 bytes
leaf entry          = 88 bytes
leaf capacity       = 185
leaf minimum        = 93
internal fanout     = 255
internal minimum    = 128
```

Those bytes are not the proposed EXP-0003 Draft geometry. That is useful here: borrower selection is an algorithmic question separable from the later 128-bit/compact-header decision.

The prototype therefore answers whether the **transition rule** is implementable and byte-significant without prematurely migrating the wire layout.

## Recursive implementation

The repository's persistent deletion already uses the same repair mechanism recursively for leaf and internal nodes. The prototype threads one explicit `ExperimentalDeleteBorrowPolicy` through that path.

At an underflowed node with minimum occupancy `M`, define:

```text
left_can_lend  = left_occupancy  > M
right_can_lend = right_occupancy > M
```

### Current/default

```text
if left_can_lend:
    borrow left
else if right_can_lend:
    borrow right
else if left exists:
    merge left
else:
    merge right
```

### Experimental fuller-sibling borrower

```text
if left_can_lend and right_can_lend:
    borrow from the sibling with greater occupancy
    if equal, borrow left
else if left_can_lend:
    borrow left
else if right_can_lend:
    borrow right
else if left exists:
    merge left
else:
    merge right
```

The left tie-break keeps the transition deterministic.

## Exact byte-level fixture

The integration test deliberately constructs a three-leaf state where both siblings can lend but the right sibling is fuller.

The current successor leaf capacity is `185`, with minimum `93`.

### Construct state

Start with two full leaves:

```text
[185, 185]
```

Insert object 371. The right full leaf overflows and splits under the existing insertion rule:

```text
[185, 93, 93]
```

Insert objects through 379:

```text
[185, 93, 101]
```

Delete 91 identifiers from the left leaf without underflowing it:

```text
[94, 93, 101]
```

Delete the first identifier from the middle leaf:

```text
target after deletion = 92
left sibling          = 94
right sibling         = 101
minimum               = 93
```

Both siblings can lend.

The current policy therefore borrows left; the experimental policy borrows right.

## Test assertions

The integration test requires all of the following.

### Default compatibility

```text
append_persistent_delete(...)
    == append_persistent_delete_experimental(..., LeftFirst)
```

The complete `PersistentBatchResult`, including bytes and report, must be equal.

This is the compatibility guard that prevents the experimental surface from silently changing existing successor evidence.

### The policy is byte-significant

For the same input file and same deletion:

```text
left_first.bytes != fuller_sibling.bytes
left_first.snapshot_digest != fuller_sibling.snapshot_digest
```

So borrower selection is not merely an implementation optimization. It changes authenticated persistent transition identity and therefore must be settled before authoritative mutation vectors are frozen.

### Structural accounting stays equal in the fixture

The two results must have equal:

```text
object_count
page_count
root_level
pages_written
pages_reused
```

This isolates the difference to **which sibling contents are rewritten and therefore which authenticated pages/root are produced**, not a gross tree-shape or accounting difference for this particular transition.

### Both results remain valid

Both outputs must pass the repository's canonical occupancy validator.

That demonstrates that the current structural validity rules admit both deterministic transitions; choosing between them is therefore a canonical-transition policy decision, not a validity distinction.

### Canonical fresh content is identical

Finally, the test runs `rewrite_all` on both outputs.

The resulting fresh canonical files must have:

```text
identical retained ObjectIds
identical canonical bytes
```

This proves that both persistent transitions expose the same active objects and payloads even though their authenticated persistent snapshot identities differ.

The result is a concrete instance of the scoped-determinism distinction studied in Experiments 0110 and 0111:

> logical current state can be equal while retained persistent representation differs because transition history differs.

## Implementation quality gates

The prototype is required to pass the existing repository gates without changing pinned vector files.

The branch has demonstrated:

- pinned immutable-successor vector manifest/boundary verification;
- formatting and clippy with warnings denied;
- full Rust workspace tests;
- Rust documentation tests;
- independent Python parser/adversarial checks;
- independent Phase 3 models;
- EXP-0002 codec and invalid corpus checks;
- immutable-successor recipe regeneration checks;
- MSRV 1.85;
- i686 portability;
- powerpc64 portability;
- Phase 3 evidence workflow; and
- Phase 3 integration workflow.

The default `LeftFirst` path remains pinned by equality and vector checks.

## What this experiment establishes

### 1. The alternative is implementation-small

The fuller-sibling rule does not require a second deletion engine. It is a local deterministic lender-selection policy threaded through the existing recursive repair mechanism.

That lowers implementation-complexity objections to considering it during FCP review.

### 2. The decision is normative if mutation bytes are normative

The same logical deletion can yield different page digests and snapshot identity depending on lender selection.

Therefore any EXP-0003 authoritative mutation corpus must name the lender-selection rule explicitly.

### 3. The half-full floor and lender rule are separable

Both policies use the same minimum occupancy and merge fallback. Experiment 0110's recommendation to review these as separate decisions is therefore supported by the actual implementation architecture.

### 4. One fixture is not a performance proof

This prototype does not show that fuller-sibling borrowing is globally better in the real persistent tree.

The fixture has equal page-write/reuse counts for one operation. Experiment 0110's reduction in long-run restructuring comes from future occupancy evolution, which requires sequence-level measurement.

The next implementation experiment should therefore run identical deterministic mixed-operation traces through both Rust policies and compare cumulative:

```text
page writes
page reuse
file growth
borrow direction
merge/split counts
root changes
fresh-canonical equivalence
```

at multiple depths.

## Mathematical next step

Experiment 0110 exposed slow mixing in finite-horizon Monte Carlo. The appropriate mathematical successor is a fringe/transition-state model for occupancy near the repair frontier.

For a state vector containing local sibling occupancies, one can construct transition probabilities for random-key deletion and random-gap insertion and solve or approximate the stationary distribution. That can estimate:

```text
P(left lends)
P(right lends)
P(merge)
P(split)
expected rewritten pages / operation
expected occupancy
```

without requiring very long simulation burn-in.

The actual Rust trace experiment can then serve as an implementation validation of the analytical model rather than the only source of evidence.

## Review implication for issue #16

Before EXP-0003 mutation bytes are accepted, review should explicitly choose among at least:

```text
A. half-full + unconditional left-first borrower
B. half-full + fuller eligible sibling, left tie-break
```

and should justify the choice using both:

1. sequence-level Rust page/write evidence; and
2. a transition/fringe occupancy model or equivalent mathematical analysis.

Until that review occurs, `LeftFirst` remains the repository default and the fuller-sibling path remains experimental only.
