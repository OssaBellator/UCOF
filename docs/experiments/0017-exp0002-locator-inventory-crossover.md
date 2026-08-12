# Experiment 0017: Locator Metadata Inventory Crossover

- **Status:** Reproducible
- **Date:** 2026-07-30
- **Related:** FCP-0002, Experiments 0010 and 0012
- **Script:** `tools/experiment_exp0002_locator_inventory_crossover.py`

## Question

When does a smaller authenticated locator that omits mirrored `kind` and `logical_len` stop saving I/O because metadata inventory must fetch object headers?

## Compared layouts

The experiment reuses the exact 16 KiB page geometry from Experiment 0010:

| Layout | Entry bytes | Mirrored `kind` and logical length |
|---|---:|---|
| Candidate 1 baseline | 88 | Yes, plus 16 reserved bytes |
| Tight same fields | 72 | Yes |
| Minimal authenticated | 56 | No |
| Minimal authenticated, 128-bit ID | 64 | No |
| Baseline fields, 128-bit ID | 96 | Yes, plus 16 reserved bytes |

An object header is 48 bytes in Candidate 1. A minimal locator must fetch that header when an operation needs `kind` or logical length.

## Model

At 100 million objects, the script computes exact paged-directory bytes and then adds 48 bytes for each object whose inventory metadata must be recovered from its object header.

The worst-case request count assumes one independent header range per object. A storage implementation may coalesce adjacent headers, but coalescing can read intervening payload bytes and cannot be assumed for sparse or externally placed records.

## Exact crossover

For tight 72-byte locators versus minimal 56-byte locators:

```text
directory-byte saving / (48 header bytes × object count)
= 0.338685
```

Therefore:

- below approximately 33.9% metadata-inventory coverage, the 56-byte locator transfers fewer total bytes;
- above approximately 33.9%, the 72-byte mirrored locator transfers fewer total bytes;
- the mirrored locator also avoids up to one range request per inspected object.

Candidate 1's 88-byte baseline remains larger than the 56-byte locator until approximately 67.5% inventory coverage because its 16 reserved bytes create additional avoidable overhead.

For the compared 128-bit layouts, the equivalent crossover is approximately 67.2% because the mirrored baseline also retains 16 reserved bytes.

## Findings

1. Removing Candidate 1's 16 trailing reserved bytes is unconditionally beneficial for the currently compared fields.
2. Object-identifier width and metadata mirroring are separate choices and should not be decided as one package.
3. A minimal 56-byte locator is favorable for lookup-heavy workloads that rarely enumerate object metadata.
4. A tight 72-byte locator is favorable for broad metadata inventory and avoids potentially enormous header-request counts.
5. One universal primary-directory layout may not optimize both sparse lookup and broad inventory; a future profile or optional secondary inventory index may be justified.
6. Request latency and payload over-read can make mirrored metadata preferable before the byte-only crossover.

## Security considerations

Mirrored values are not authoritative merely because they avoid a header request. Strict validation and selected-object access must still cross-check mirrored `kind`, lengths, identifier, physical range, and digest against the object record.

A minimal locator reduces duplicated claims but increases attacker-controlled range work for metadata inventory. Header reads must remain bounded by object count, request count, bytes read, and maximum request size.

## Decision impact

Experiment 0010 established the storage costs. This experiment adds a workload criterion:

- retire the 88-byte baseline's reserved tail;
- keep 56-byte and 72-byte 64-bit alternatives alive until primary use-case weighting is explicit;
- evaluate identifier width independently;
- do not select the smallest locator solely from directory-size tables.

## Reproduction

```console
python3 tools/experiment_exp0002_locator_inventory_crossover.py
```
