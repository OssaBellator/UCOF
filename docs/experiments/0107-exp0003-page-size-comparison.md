# Experiment 0107 — EXP-0003 Page-Size Comparison

**Status:** reproducible arithmetic evidence for FCP-0003 Review  
**Date:** 2026-08-13  
**Tool:** `tools/experiment_exp0003_page_size.py`

## Question

Does the 16 KiB immutable-page size proposed in the first EXP-0003 Draft remain a reasonable review target after changing the research geometry to:

- 80-byte page headers;
- 64-byte primary leaf locators;
- 72-byte internal child references?

Candidate 1 page-size evidence used different entry widths. The successor should not inherit 16 KiB merely because an earlier experiment used it.

## Model

For candidate page size `P`:

```text
leaf_capacity    = floor((P - 80) / 64)
internal_fanout  = floor((P - 80) / 72)
leaf_minimum     = ceil(leaf_capacity / 2)
internal_minimum = ceil(internal_fanout / 2)
```

The model uses the number of pages required by canonical left-to-right grouping. Minimum-occupancy final-two-page redistribution changes partition sizes but not the number of pages, so page-count arithmetic remains `ceil(N / capacity)` at each level.

For a selected-object lookup or no-split persistent mutation, the model counts one complete authenticated page per tree level. It deliberately excludes:

- object record/header bytes;
- bootstrap/snapshot/footer bytes;
- transport headers and TLS framing;
- caching/coalescing;
- latency and provider billing;
- page compression;
- operating-system block size;
- read amplification from source API chunking.

This is therefore a geometry/work model, not a benchmark.

## Derived geometry

| Page size | Leaf capacity | Leaf minimum | Internal fanout | Internal minimum |
|---:|---:|---:|---:|---:|
| 4 KiB | 62 | 31 | 55 | 28 |
| 16 KiB | 254 | 127 | 226 | 113 |
| 64 KiB | 1,022 | 511 | 909 | 455 |

## One million objects

| Page size | Level page counts (leaf → root) | Directory bytes | Bytes/object | Path reads | Path bytes |
|---:|---|---:|---:|---:|---:|
| 4 KiB | `16130 / 294 / 6 / 1` | 67,301,376 | 67.301 | 4 | 16,384 |
| 16 KiB | `3938 / 18 / 1` | 64,831,488 | 64.831 | 3 | 49,152 |
| 64 KiB | `979 / 2 / 1` | 64,356,352 | 64.356 | 3 | 196,608 |

## One hundred million objects

| Page size | Level page counts (leaf → root) | Directory bytes | Bytes/object | Path reads | Path bytes |
|---:|---|---:|---:|---:|---:|
| 4 KiB | `1612904 / 29326 / 534 / 10 / 1` | 6,728,806,400 | 67.288 | 5 | 20,480 |
| 16 KiB | `393701 / 1743 / 8 / 1` | 6,479,101,952 | 64.791 | 4 | 65,536 |
| 64 KiB | `97848 / 108 / 1` | 6,419,709,952 | 64.197 | 3 | 196,608 |

At this scale:

- moving from 4 KiB to 16 KiB reduces directory metadata by about 249.7 MB (3.7%) and removes one authenticated page read per lookup path, but increases complete-path bytes from 20 KiB to 64 KiB;
- moving from 16 KiB to 64 KiB saves only about 59.4 MB (0.9%) of directory metadata and removes one more page read, while tripling path bytes from 64 KiB to 192 KiB.

## One billion objects

| Page size | Level page counts (leaf → root) | Directory bytes | Bytes/object | Path reads | Path bytes |
|---:|---|---:|---:|---:|---:|
| 4 KiB | `16129033 / 293256 / 5332 / 97 / 2 / 1` | 67,287,945,216 | 67.288 | 6 | 24,576 |
| 16 KiB | `3937008 / 17421 / 78 / 1` | 64,790,659,072 | 64.791 | 4 | 65,536 |
| 64 KiB | `978474 / 1077 / 2 / 1` | 64,196,050,944 | 64.196 | 4 | 262,144 |

The fixed 64-byte locator dominates total metadata at all three candidate page sizes. Larger pages therefore provide diminishing metadata savings once entry density is already high.

## Mutation consequence

For a no-split single-object persistent update, page bytes rewritten along one leaf-to-root path equal the same path-byte values in this simplified model:

- 4 KiB candidate: fewer bytes, more page objects/digests and usually more levels;
- 16 KiB candidate: moderate bytes and fewer levels;
- 64 KiB candidate: substantially more rewritten bytes per path with only modest metadata savings.

Split/merge operations can touch siblings or two pages at one level, so actual transition bytes exceed the simple path value. That does not change the direction of the page-size trade-off.

## Interpretation

### 4 KiB

Strengths:

- lowest authenticated path-transfer bytes;
- lowest no-split page rewrite bytes;
- naturally friendly to small-range remote lookup when per-request latency is low or requests are coalesced.

Costs:

- approximately 4–5% more directory metadata than 64 KiB at large scale;
- greater tree depth and more page digest/reference operations;
- more potential remote round trips when each level requires a distinct request;
- more page records for construction, validation, compaction, and metadata management.

### 16 KiB

Strengths:

- directory metadata is already close to the theoretical 64-byte-per-object floor;
- materially fewer levels/page records than 4 KiB;
- much lower path bytes than 64 KiB;
- preserves the substantial implementation/fuzz/portability experience already accumulated around 16 KiB pages without relying on Candidate 1 identities.

Costs:

- complete authenticated lookup/update paths transfer roughly 3× the bytes of 4 KiB in the 100-million-object model;
- not obviously optimal for every remote latency/bandwidth regime.

### 64 KiB

Strengths:

- lowest page count and slightly lowest metadata overhead;
- fewer levels/round trips at some scales.

Costs:

- very large targeted path reads and no-split rewrite bytes;
- only small metadata savings over 16 KiB because 64-byte locators dominate;
- larger fault/tamper/rewrite unit and potentially worse cache behavior.

## Current recommendation

**Retain 16 KiB as the FCP-0003 Draft review target, but do not treat this arithmetic model as sufficient evidence for final selection.**

With the proposed EXP-0003 widths, 16 KiB is a plausible compromise rather than an obvious optimum:

- compared with 4 KiB it pays path-byte amplification for fewer page objects/round trips and modestly lower metadata;
- compared with 64 KiB it gives up less than 1% metadata efficiency at 100 million objects while reducing the modeled path from 192 KiB to 64 KiB.

Before EXP-0003 allocation, real HTTP/cloud adapter measurements from issue #10 should compare at least 4 KiB and 16 KiB page geometries under:

- cold and warm cache;
- high-latency/low-latency transport;
- individual versus safely coalesced range requests;
- targeted lookup;
- full validation;
- replacement/insertion/deletion append tails;
- provider request billing where relevant.

A 64 KiB candidate should remain in the comparison but currently has a weaker case because its metadata gain over 16 KiB is small relative to targeted path amplification.

## Reproduce

```console
python3 tools/experiment_exp0003_page_size.py
python3 tools/experiment_exp0003_page_size.py --json
```

The script uses only Python's standard library and performs deterministic integer arithmetic.

## Decision boundary

This experiment does not accept 16 KiB, change FCP-0003 status, or allocate EXP-0003. It replaces inherited intuition with geometry derived from the proposed 128-bit-ID / 64-byte-locator successor layout.
