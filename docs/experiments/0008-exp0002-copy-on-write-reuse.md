# Experiment 0008: EXP-0002 Copy-on-Write Page Reuse

- **Status:** Reproducible model evidence
- **Date:** 2026-07-30
- **Related:** FCP-0002, ADR-0010, Experiment 0006
- **Implementations:** persistent Rust model and independent Python calculation

## Question

How much directory data could Candidate 1 avoid rewriting if an append reused unchanged pages and copied only the updated root-to-leaf path?

The current concrete writer rebuilds every directory page. Candidate 1 already authenticates child pages by digest and locator, so a future append could instead retain unchanged historical pages and write new versions only for modified pages and any split propagation.

This experiment tests that architectural possibility without changing Candidate 1 bytes or declaring a normative page-reuse algorithm.

## Model

The Rust persistent model:

- builds canonical ordered leaf and internal pages;
- treats a changed entry revision as a changed physical locator;
- leaves the old snapshot and every old page immutable;
- copies one leaf and each ancestor for replacement;
- splits only pages on the affected insertion path;
- appends new page identifiers rather than overwriting old pages;
- validates ranges, ordering, references, cycles, depth, visited pages, and new-page limits;
- reports new pages, reused pages, old/new depth, and ideal copied bytes.

The independent Python experiment uses the concrete Candidate 1 capacities:

- 16 KiB page;
- 64-byte page header;
- 88-byte leaf entry;
- 64-byte internal entry;
- leaf capacity 185;
- internal fanout 255.

It computes full-rebuild bytes, replacement path-copy bytes, and right-edge insertion bytes for deterministic bulk-packed trees.

## Replacement results

| Objects | Depth | Reachable pages | Full rebuild | Path-copy replacement | Rebuild/path-copy amplification |
|---:|---:|---:|---:|---:|---:|
| 1,000 | 2 | 7 | 112 KiB | 32 KiB | 3.5x |
| 1,000,000 | 3 | 5,429 | 84.83 MiB | 48 KiB | over 1,800x |
| 100,000,000 | 4 | 542,671 | 8.28 GiB | 64 KiB | over 135,000x |

A replacement copies exactly one page per level in the model. Every unrelated page remains reachable from the old root and is reused by the new root.

## Insertion behavior

A right-edge insertion writes at least one page per level. If the target leaf is full, it writes two leaf pages and propagates one new child reference upward. Each full ancestor can similarly split into two pages. If the old root splits, one additional root page is written.

For a tree of depth `d`, the model bounds one insertion between:

- `d` new pages when no page splits;
- `2d + 1` new pages when every page on the path splits and a new root is required.

Even the worst path-copy bound is dramatically below a full rebuild at large object counts.

## Security and recovery implications

Page reuse is safe only if the new snapshot authenticates every reused child page by exact digest, locator, level, and key range. A reader must not infer trust from historical location alone.

A page-reuse writer must also preserve:

- append-only publication;
- exact-end footer authority;
- no overwrite of pages reachable from an earlier complete snapshot;
- deterministic split and occupancy rules;
- checked arithmetic and bounded page creation;
- cycle and repeated-offset rejection;
- repair and compaction rules for retained historical pages.

Interrupted writes before the new footer remain unpublished. The earlier root continues to reference the unchanged page set.

## Finding

Copy-on-write reuse is not merely an optimization. At UC-02 scale, it changes append directory work from gigabytes to tens or hundreds of kibibytes for a small update.

Candidate 1 therefore should not move to Review with full-directory rebuild as its only append algorithm. The next byte-level experiment must define:

1. deterministic split and minimum-occupancy rules;
2. how new internal entries refer to reused historical pages;
3. whether page sequence records creation or snapshot membership;
4. how a strict reader distinguishes acceptable historical reuse from stale or cyclic references;
5. how repair and compaction rewrite reused page graphs;
6. append-amplification vectors covering replacement, leaf split, internal split, and root split.

## Boundaries

This experiment does not define:

- exact reused-page bytes;
- a stable page allocation policy;
- deletion, merge, or rebalancing rules;
- concurrent writers;
- free-space reuse;
- production benchmarks;
- remote cache behavior.

The Rust page identifiers are in-memory indexes, not file offsets.

## Reproduction

```console
cargo test --locked -p ucof-experiments --test cow_reuse_model
python3 tools/experiment_exp0002_cow_reuse.py
```

Both implementations contain assertions for page capacities, tree shape, old-snapshot preservation, copied-page counts, split bounds, and full-rebuild amplification.
