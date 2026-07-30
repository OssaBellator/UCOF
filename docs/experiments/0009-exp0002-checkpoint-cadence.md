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
2. a conservative copy-on-write model in which every inserted object copies one complete path at the checkpoint's resulting tree depth.

The path-copy estimate does not share paths within a batch and excludes split pages. It is therefore not a production benchmark or a lower bound. It exists to show the architectural order of magnitude.

## Results

| Objects per checkpoint | Checkpoints | Maximum unpublished payload | Cumulative full-rebuild metadata | Conservative path-copy metadata |
|---:|---:|---:|---:|---:|
| 100 | 10,000 | 400 KiB | very high; thousands of growing directory rebuilds | bounded by one path per inserted object |
| 1,000 | 1,000 | 3.91 MiB | one tenth as many complete rebuilds | similar path-copy order with less fixed checkpoint overhead |
| 10,000 | 100 | 39.06 MiB | one hundred growing rebuilds | dominated by inserted-object path work |
| 100,000 | 10 | 390.63 MiB | ten growing rebuilds | lowest fixed checkpoint overhead but largest interruption window |

Exact byte totals are printed and asserted by the reproduction script.

## Interpretation

### Full rebuild

With the current deterministic writer, checkpoint cadence is coupled to total directory size. Frequent complete checkpoints repeatedly rewrite pages that did not change. At large object counts this makes durability policy an architectural performance problem rather than a small footer cost.

### Copy-on-write reuse

With authenticated historical page reuse, checkpoint cost can depend primarily on changed paths rather than total object count. Batch construction should improve further by sharing copied ancestors among many inserted objects.

### Lost work

The maximum unpublished payload grows linearly with objects per checkpoint. The format cannot choose one correct cadence for all applications:

- sensor capture may prefer small bounded objects and frequent complete snapshots;
- archival bulk creation may accept a much larger unpublished tail;
- local storage and remote object storage have different durability and latency costs.

## Finding

Complete-only checkpoints are viable only when the writer can avoid full-directory rebuild amplification. ADR-0012 and Experiment 0008 therefore reinforce the same requirement: copy-on-write page reuse or another bounded deterministic update algorithm is a Phase 3 Review blocker.

The format should define validity and publication, while profiles or applications choose cadence. Candidate 1 should not mandate one number of objects, bytes, or seconds between checkpoints.

## Security implications

A complete checkpoint remains subject to every normal snapshot check. Frequency does not weaken integrity requirements and does not provide trusted freshness.

Writers must not publish a footer before all referenced objects, pages, and snapshot bytes are durable according to the storage environment. The file format cannot by itself guarantee filesystem flush, device persistence, remote-object atomicity, or multi-region durability.

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

The script asserts capacities, million-object tree shape, checkpoint counts, unpublished payload windows, and monotonic full-rebuild overhead.
