# Experiment 0005 — Initial directory model comparison

## Status

Initial closed-form baseline. These results guide FCP-0002 experiments but do not select a wire layout.

## Question

Which primary-directory family best balances:

- bounded single-object lookup;
- deterministic layout;
- ordered inventory;
- append/update rewrite amplification;
- page-local hostile-input validation;
- massive-object overhead;
- remote range-request count?

## Compared models

### Copy-on-write B+ tree

Assumptions:

- 16 KiB pages;
- 240 fixed-size leaf entries per page;
- internal fanout 256;
- one rewritten root-to-leaf path for a single-object update.

### Monolithic sorted fixed-entry array

Assumptions:

- 64 bytes per entry;
- binary search probes one fixed entry at a time;
- a canonical insertion rewrites the complete array.

This is intentionally optimistic for lookup bytes and pessimistic but realistic for append rewrite amplification.

### Deterministic hash pages

Assumptions:

- 16 KiB pages;
- effective 180 entries per bucket page at the target load factor;
- 1,024 bucket locators per locator page;
- root, locator, and bucket page reads for lookup;
- one page at each level rewritten for an ordinary insertion.

The model does not yet charge for adversarial overflow chains, collision proofs, deterministic resizing, or ordered inventory.

## Results

| Entries | Model | Directory MiB | Directory page reads per lookup | One-update rewrite MiB |
|---:|---|---:|---:|---:|
| 1,000 | Copy-on-write B+ tree | 0.09 | 2 | 0.03 |
| 1,000 | Monolithic sorted array | 0.06 | 10 | 0.06 |
| 1,000 | Deterministic hash pages | 0.12 | 3 | 0.05 |
| 1,000,000 | Copy-on-write B+ tree | 65.39 | 3 | 0.05 |
| 1,000,000 | Monolithic sorted array | 61.04 | 20 | 61.04 |
| 1,000,000 | Deterministic hash pages | 86.92 | 3 | 0.05 |
| 100,000,000 | Copy-on-write B+ tree | 6,535.98 | 4 | 0.06 |
| 100,000,000 | Monolithic sorted array | 6,103.52 | 27 | 6,103.52 |
| 100,000,000 | Deterministic hash pages | 8,689.06 | 3 | 0.05 |

The figures are reproduced by `tools/experiment_directory_models.py`.

## Findings

### Flat sorted storage is compact but not append-friendly

The sorted array has the smallest nominal directory bytes in this baseline. Its rewrite amplification grows linearly: inserting one canonical entry into a 100-million-entry array rewrites roughly 6.0 GiB of directory data. This conflicts with frequent append snapshots and immutable object storage.

Binary search also creates many small probes unless larger ranges are cached. Page authentication, unknown-field preservation, and remote request coalescing would add complexity not charged here.

### Hash pages shorten expected lookup but add unresolved adversarial semantics

The hash model has a short expected lookup path and low ordinary update amplification. It uses more space at the selected load factor and does not naturally provide canonical identifier-order iteration.

Before it can be a serious candidate, the project would need deterministic hash selection, collision and overflow limits, canonical resizing, malicious-key analysis, stable bucket enumeration, and authenticated locator paging. Expected constant-time lookup alone is insufficient.

### A paged ordered tree is the strongest initial baseline

The B+ tree requires two to four directory page reads across the tested sizes, supports ordered iteration, isolates corruption to bounded pages, and rewrites only a root-to-leaf path for a small update.

This does not select 16 KiB pages, fixed entries, fanout 256, or B+ tree bytes. It selects the ordered paged tree as the baseline that other candidates must beat with complete security and canonicalization costs included.

## Rejected conclusion

The experiment does **not** claim that the B+ tree is universally optimal. The result depends on page size, entry width, authentication, locality, update distribution, and storage latency. The final decision requires measured implementations and remote-access traces.

## Required next measurements

- 4 KiB, 16 KiB, and 64 KiB pages;
- variable-size versus fixed-size entries;
- page-local canonical metadata versus a fixed binary entry layout;
- copy-on-write page reuse between snapshots;
- bulk append and random replacement workloads;
- remote latency and request coalescing;
- authenticated page identities and proof bytes;
- collision-storm hash pages;
- sorted-array chunking or Merkle blocking;
- 32-bit parser limits and malformed-page isolation.

## Current decision

Use the copy-on-write ordered-page model as the EXP-0002 prototype baseline. Keep the sorted array as a compact static baseline and hash pages as a conditional challenger. No numeric page or entry parameters are accepted by this experiment.
