# Experiment 0122: EXP-0003 deletion parent/rewrite reward catalog

- **Status:** Pinned implementation-derived reward evidence; non-normative
- **Date:** 2026-08-13
- **Related:** FCP-0003, issue #16, Experiments 0114, 0119, 0120, and 0121
- **Executable:** `crates/ucof-experiments/examples/exp0003_delete_parent_reward_catalog.rs`
- **Pinned manifest:** `tests/vectors/exp-0003-delete-reward-catalog/manifest.csv`

## Question

Experiments 0116–0120 reduced the occupancy state needed near a deletion underflow frontier. That still leaves a reward-model gap: the same leaf-level action can have different immutable cost depending on the parent/root context.

What page-write, page-reuse, root-height, and append-byte rewards does the real persistent Rust deletion implementation produce for representative structural cases?

Experiment 0122 records those rewards directly from the implementation rather than assigning theoretical constants.

## Reward accounting

For each source file and deletion, the executable records:

```text
source/result root level
source/result current page count
current page-count delta
touched original pages
reused original pages
new pages written
bytes appended
source/result object count
```

`touched_original` is computed as:

```text
source current page count - pages reused
```

so it measures how many current source pages cease to be exact reusable pages on the new current path/tree.

`pages_written` is the implementation's actual materialized-page counter.

These are deliberately separate rewards: an old page can be touched/removed by a structural transition without a one-for-one replacement page being emitted.

## Catalog

The pinned CI-produced rows are:

| Case | Repair class | Root level | Current pages | Touched originals | Reused | Written | Appended bytes |
|---|---|---:|---:|---:|---:|---:|---:|
| root leaf | root-leaf rewrite | `0 -> 0` | `1 -> 1` | 1 | 0 | 1 | 16,608 |
| depth-1 no underflow | path copy, no underflow | `1 -> 1` | `4 -> 4` | 2 | 2 | 2 | 32,992 |
| depth-1 borrow left | leaf borrow + parent rewrite | `1 -> 1` | `3 -> 3` | 3 | 0 | 3 | 49,376 |
| depth-1 borrow right | leaf borrow + parent rewrite | `1 -> 1` | `3 -> 3` | 3 | 0 | 3 | 49,376 |
| depth-1 merge + root collapse | leaf merge + root collapse | `1 -> 0` | `3 -> 1` | 3 | 0 | 1 | 16,608 |
| depth-1 merge, root retained | leaf merge + parent rewrite | `1 -> 1` | `4 -> 3` | 3 | 1 | 2 | 32,992 |
| depth-2 recursive internal borrow | leaf merge + internal borrow + root rewrite | `2 -> 2` | `260 -> 259` | 5 | 255 | 4 | 65,760 |

Exact CSV:

```text
case,repair_class,source_root_level,result_root_level,root_level_delta,source_page_count,result_page_count,page_count_delta,touched_original,pages_reused,pages_written,bytes_appended,expected_affine_bytes,source_objects,result_objects
root-leaf,root-leaf-rewrite,0,0,0,1,1,0,1,0,1,16608,16608,10,9
depth1-no-underflow,path-copy-no-underflow,1,1,0,4,4,0,2,2,2,32992,32992,400,399
depth1-borrow-left,leaf-borrow-parent-rewrite,1,1,0,3,3,0,3,0,3,49376,49376,187,186
depth1-borrow-right,leaf-borrow-parent-rewrite,1,1,0,3,3,0,3,0,3,49376,49376,187,186
depth1-merge-root-collapse,leaf-merge-root-collapse,1,0,-1,3,1,-2,3,0,1,16608,16608,186,185
depth1-merge-keep-root,leaf-merge-parent-rewrite,1,1,0,4,3,-1,3,1,2,32992,32992,279,278
depth2-recursive-internal-borrow,leaf-merge-internal-borrow-root-rewrite,2,2,0,260,259,-1,5,255,4,65760,65760,47361,47360
```

## Exact affine byte reward

For every catalog case, the executable requires:

```text
bytes_appended
    == pages_written * PAGE_SIZE
       + SNAPSHOT_LEN
       + FOOTER_LEN
```

At the current immutable-successor microformat:

```text
PAGE_SIZE    = 16,384
SNAPSHOT_LEN = 96
FOOTER_LEN   = 128
```

therefore:

```text
bytes_appended = 16,384 * pages_written + 224
```

The seven implementation paths all satisfy that identity exactly.

This is important for the mathematical model. For deletion-only persistent transitions under this microformat, append-byte reward does not need an independent stochastic model once page-write reward is known:

```text
E[append bytes per deletion]
    = 16,384 * E[pages written per deletion] + 224
```

The `224` publication tail is paid once per successful deletion transition, independent of the repair class.

This relationship is implementation/microformat-specific evidence, not a claim that a future EXP-0003 geometry must preserve the same constants.

## Structural context matters more than the leaf action label

### Borrow at depth 1

Either borrow direction has the same immediate reward in this catalog:

```text
touched originals = 3
pages written      = 3
bytes appended     = 49,376
```

The target leaf, donor leaf, and parent are all represented by new current pages.

This means the local `borrow-left` versus `borrow-right` label alone does not change immediate page-count cost in this structural class, matching Experiment 0121's minimal candidate vector.

### Merge with root retained

A leaf merge in a larger depth-1 tree gives:

```text
current pages      4 -> 3
touched originals  3
pages reused        1
pages written       2
bytes appended      32,992
```

The merged leaf and retained parent/root are materialized; an unrelated leaf remains exactly reusable.

### Merge with root collapse

The same broad leaf-level event can be dramatically cheaper in emitted-page reward when the old root collapses:

```text
root level          1 -> 0
current pages       3 -> 1
touched originals   3
pages written        1
bytes appended      16,608
```

Three old current pages are structurally affected, but only the merged leaf needs to be emitted as the new root/current tree.

This is direct evidence that `touched_original` and `pages_written` must remain distinct model rewards.

### Recursive depth-2 repair

The recursive case gives:

```text
root level           2 -> 2
current pages      260 -> 259
touched originals    5
pages reused        255
pages written         4
bytes appended       65,760
```

A leaf merge propagates into an internal-node underflow, an internal sibling lends, and the root path is rewritten. The recursive structural context adds parent-level write cost even though the top-level root height stays constant.

## Model consequence

Experiments 0118–0120 showed that a compact frontier occupancy state can predict local repair behavior much better than a raw global iid model.

Experiment 0122 shows what must be added to turn that into a useful **Markov reward** model.

A state/reward representation needs at least enough structural context to distinguish:

```text
root leaf
ordinary path copy
borrow with parent rewrite
merge with parent retained
merge with root collapse
merge with recursive parent repair
```

The occupancy state still determines whether borrow/merge occurs and, for fuller-sibling, which lender is selected. Parent context determines how that event maps to immutable page-write reward.

A practical factorization is therefore:

```text
frontier transition state
    -> local repair event

structural context state
    -> page-write/touched/reuse reward

page-write reward
    -> append-byte reward via 16,384*writes + 224
```

That is considerably smaller than modeling raw persistent-file byte growth directly.

## Validation target

The next model should combine:

1. the policy-aware frontier states from Experiment 0120;
2. structural reward classes from this catalog;
3. observed event frequencies from Experiments 0110/0119;
4. a transition/reward residual for any state aggregation;
5. validation against the real Rust cumulative page-write deltas in Experiments 0114 and 0119.

The model should predict page writes first. Append-byte predictions then follow from the affine identity for this microformat.

## Boundary

This catalog is evidence about the current immutable-successor research implementation.

It does **not**:

- choose `FullerSiblingLeftTie`;
- change the default `LeftFirst` behavior;
- accept FCP-0003;
- allocate an EXP-0003 epoch;
- stabilize page geometry or wire bytes;
- turn the candidate vectors from Experiment 0121 into authoritative vectors.

## Reproduction

Print the catalog:

```console
cargo run --locked -p ucof-experiments --example exp0003_delete_parent_reward_catalog
```

The pinned manifest is checked from CI after regeneration.
