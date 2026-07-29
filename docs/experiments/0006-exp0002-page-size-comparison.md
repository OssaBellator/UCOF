# Experiment 0006: EXP-0002 Page-Size Comparison

- **Status:** Reproducible
- **Date:** 2026-07-30
- **Related:** ADR-0010, FCP-0002, `tools/experiment_exp0002_page_sizes.py`
- **Scope:** Fixed-size ordered pages using the provisional 88-byte leaf entry, 64-byte internal entry, and 64-byte page header

## Question

Does the provisional 16 KiB directory page provide a defensible middle point between 4 KiB and 64 KiB pages for the first EXP-0002 byte candidate?

This experiment is closed-form. It measures directory size, tree depth, authenticated path bytes, and ideal copy-on-write single-update bytes. It does not measure operating-system caching, remote-range latency, compression, storage-sector behaviour, implementation overhead, or real workload locality.

## Method

For each page size:

- leaf capacity is `floor((page_size - 64) / 88)`;
- internal fanout is `floor((page_size - 64) / 64)`;
- leaf pages are packed to capacity except the final page;
- each internal level is packed to capacity except its final page;
- lookup work is one complete page per root-to-leaf level;
- ideal copy-on-write update work is one rewritten page per level;
- the current deterministic writer still performs a full directory rebuild, so its actual rewrite work equals the full directory size.

The script evaluates 1,000, 1,000,000, and 100,000,000 objects.

## Capacities

| Page size | Leaf entries | Internal children |
|---:|---:|---:|
| 4 KiB | 45 | 63 |
| 16 KiB | 185 | 255 |
| 64 KiB | 744 | 1,023 |

## Results at 100 million objects

| Page size | Total pages | Depth | Directory bytes | Bytes per object | Authenticated lookup bytes | Ideal COW update bytes |
|---:|---:|---:|---:|---:|---:|---:|
| 4 KiB | 2,258,067 | 5 | 9,249,042,432 | 92.49 | 20 KiB | 20 KiB |
| 16 KiB | 542,671 | 4 | 8,891,121,664 | 88.91 | 64 KiB | 64 KiB |
| 64 KiB | 134,542 | 3 | 8,817,344,512 | 88.17 | 192 KiB | 192 KiB |

The dominant space cost is the 88-byte leaf entry, so increasing page size reduces page-header and partially filled internal-page overhead but cannot materially reduce the lower bound below approximately 8.8 GB for 100 million objects.

## Interpretation

### 4 KiB

Advantages:

- lowest bytes read for one authenticated lookup at every measured scale;
- smallest ideal copy-on-write update unit;
- natural fit for common storage and virtual-memory page sizes.

Costs:

- 2.26 million authenticated pages at 100 million objects;
- five dependent reads at the largest measured scale;
- approximately 432 MB more directory data than 16 KiB;
- larger page-digest and page-locator population.

### 16 KiB

Advantages:

- four dependent page reads at 100 million objects;
- substantially fewer pages than 4 KiB;
- only about 74 MB more directory data than 64 KiB at 100 million objects;
- 64 KiB authenticated path, which remains suitable for bounded range access;
- moderate copy-on-write rewrite unit.

Costs:

- more path bytes than 4 KiB;
- one additional dependent read compared with 64 KiB at 100 million objects;
- still inherits the large 88-byte-per-object leaf-entry floor.

### 64 KiB

Advantages:

- three dependent page reads at 100 million objects;
- fewest pages and lowest directory overhead of the three candidates.

Costs:

- 192 KiB must be authenticated for one lookup at the largest measured scale;
- a single copy-on-write update rewrites at least 192 KiB of directory pages;
- small directories pay a large minimum allocation and transfer cost;
- one corrupted page affects a larger key range.

## Finding

The 16 KiB choice remains a reasonable first experimental midpoint. It avoids the page-count and depth of 4 KiB while avoiding the 192 KiB authenticated lookup and update path of 64 KiB at 100 million objects.

This evidence does **not** accept 16 KiB normatively. It instead narrows the next required evidence:

1. benchmark local and HTTP-range lookups with cold and warm caches;
2. measure page verification throughput and per-request latency;
3. measure copy-on-write append amplification once page reuse exists;
4. test directory page corruption blast radius;
5. evaluate reducing the 88-byte leaf entry, because entry width dominates total size more than page size.

## Reproduction

Run:

```sh
python3 tools/experiment_exp0002_page_sizes.py
```

The script contains assertions for capacities, page counts, depths, path bytes, and directory-size ordering.
