# Experiment 0009: EXP-0002 Complete-Checkpoint Cadence

- **Status:** Reproducible model evidence
- **Date:** 2026-07-30
- **Related:** ADR-0012, FCP-0002, Experiment 0008
- **Script:** `tools/experiment_exp0002_checkpoint_cadence.py`

## Question

What durability and metadata-write trade-off results when Candidate 1 uses ordinary complete snapshot commits as checkpoints?

ADR-0012 rejects separate progress-checkpoint bytes for Candidate 1. An application instead publishes complete snapshots at a chosen cadence. More frequent checkpoints reduce unpublished work after interruption but increase directory, snapshot, and footer writes.

## Scenario

The model uses:

- 1,000,000 objects total;
- 4 KiB payload per object;
- one root identifier per checkpoint;
- Candidate 1 16 KiB pages;
- 185 leaf entries per page;
- 255 internal children per page;
- a 168-byte one-root snapshot record;
- a 160-byte footer.

It compares:

1. the current writer's full directory rebuild at every checkpoint;
2. a deliberately conservative copy-on-write model in which every inserted object copies one complete path at the checkpoint's resulting tree depth.

The path-copy estimate does not share ancestors within a batch and excludes split pages. It therefore models a naive persistent update loop, not an efficient batched writer and not a lower bound.

## Results

| Objects per checkpoint | Checkpoints | Maximum unpublished payload | Cumulative full-rebuild metadata | Naive per-object path-copy metadata |
|---:|---:|---:|---:|---:|
| 100 | 10,000 | 400.00 KiB | 414.36 GiB | 45.06 GiB |
| 1,000 | 1,000 | 3.91 MiB | 41.47 GiB | 45.06 GiB |
| 10,000 | 100 | 39.06 MiB | 4.18 GiB | 45.17 GiB |
| 100,000 | 10 | 390.62 MiB | 466.68 MiB | 45.78 GiB |

The script reproduces and asserts these relationships. It demonstrates a strategy crossover rather than one universally superior method.

## Interpretation

### Frequent checkpoints

At 100 objects per checkpoint, repeated full-directory rebuilds dominate. Copying one path per inserted object is still expensive, but it avoids hundreds of gigabytes of repeated unchanged-page writes in this model.

### Sparse checkpoints

At 1,000 or more objects per checkpoint, the naive per-object path-copy loop becomes more expensive than rebuilding the final directory once per checkpoint. It writes persistent intermediate paths that the checkpoint never publishes.

This does not invalidate copy-on-write page reuse. It shows that an implementation must batch changes, share copied ancestors, and emit only the final reachable pages for a checkpoint. Persistent page reuse without batch construction is insufficient.

### Lost work

The maximum unpublished payload grows linearly with objects per checkpoint. The format cannot choose one correct cadence for all applications:

- sensor capture may prefer small bounded objects and frequent complete snapshots;
- archival bulk creation may accept a much larger unpublished tail;
- local storage and remote object storage have different durability and latency costs.

## Finding

Complete-only checkpoints remain viable, but the writer strategy must match cadence:

- frequent checkpoints require avoiding repeated full-directory rebuilds;
- sparse checkpoints require batching path-copy updates so unpublished intermediate page versions are not serialized;
- a production writer should choose between full construction and batched reuse from measured changed-set size, not from a universal rule.

Copy-on-write reuse or another bounded deterministic update algorithm remains a Phase 3 Review blocker, now with the stronger requirement that it support batch path sharing and final-state-only publication.

The format should define validity and publication, while profiles or applications choose cadence. Candidate 1 should not mandate one number of objects, bytes, or seconds between checkpoints.

## Security implications

A complete checkpoint remains subject to every normal snapshot check. Frequency does not weaken integrity requirements and does not provide trusted freshness.

Writers must not publish a footer before all referenced objects, pages, and snapshot bytes are durable according to the storage environment. The file format cannot by itself guarantee filesystem flush, device persistence, remote-object atomicity, or multi-region durability.

A batch builder must also enforce limits on changed entries, copied pages, split pages, retained historical pages, output bytes, and elapsed work. Sharing ancestors must not create aliasing that allows later mutation of an already published page.

## Boundaries

This experiment does not model:

- device flush latency;
- HTTP or cloud-object publication;
- compression or encryption;
- page split frequency;
- batch path sharing;
- object dependency graphs;
- concurrent writers;
- automatic cadence selection.

## Reproduction

```console
python3 tools/experiment_exp0002_checkpoint_cadence.py
```

The script asserts capacities, million-object tree shape, checkpoint counts, unpublished payload windows, monotonic full-rebuild overhead, and the crossover between repeated rebuilds and naive per-object path copying.
