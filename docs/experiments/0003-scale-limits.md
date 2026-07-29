# Experiment 0003 — UC-02 scale limits

**Status:** Reproducible Phase 1 evidence  
**Script:** `tools/experiment_scale_limits.py`  
**Scope:** Lower-bound metadata model for `UCOF-EXP-0001`

## Question

Can the current fixed records and fully materialized canonical directory plausibly support UC-02's range of one million to one hundred million logical objects?

## Method

The model uses zero-byte opaque objects and therefore excludes payload storage, manifest bytes, the directory record header, indexes, deduplication references, alignment, and snapshot history. It is a lower bound, not a forecast of a complete archive.

The directory calculation exactly models the current five-field canonical-CBOR entry shape with sequential identifiers and record offsets.

## Results

| Logical objects | Record header bytes | Directory payload bytes | Lower-bound file bytes |
|---:|---:|---:|---:|
| 1 | 40 | 55 | 207 |
| 1,000 | 40,000 | 47,728 | 87,840 |
| 1,000,000 | 40,000,000 | 51,865,384 | 91,865,496 |
| 100,000,000 | 4,000,000,000 | 5,199,865,384 | 9,199,865,496 |

## Finding

EXP-0001 does not satisfy UC-02. At the lower end of the use case, framing plus directory metadata already exceeds the reference reader's 64 MiB default file limit and requires materializing approximately one million records and directory entries. At the upper end, metadata alone is measured in gigabytes.

Raising limits would conceal rather than solve the architectural problem.

## Consequences

A future design serving UC-02 needs:

- paged or hierarchical primary directories;
- range-backed lookup without loading the full inventory;
- profile limits that distinguish local archives from massive remote stores;
- compact object or chunk grouping for tiny logical objects;
- streaming verification and bounded index traversal;
- explicit accounting for deduplicated references and reachability.

The Phase 2 in-memory reader may remain useful for small files, but it must not be presented as evidence that the stable core supports massive object counts. UC-02 should be revisited in Phase 3 when paged directories and snapshots are designed.
