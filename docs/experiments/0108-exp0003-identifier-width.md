# Experiment 0108 — EXP-0003 Identifier Width and Compact Header Comparison

**Status:** reproducible arithmetic evidence for FCP-0003 Review  
**Date:** 2026-08-13  
**Tool:** `tools/experiment_exp0003_id_width.py`

## Question

Should EXP-0003 use 128-bit opaque object identifiers, and does the first Draft pay unnecessary fixed-header overhead while doing so?

The decision must separate two questions:

1. **namespace semantics:** are core object IDs file-local coordinated handles, or must they support independent/uncoordinated generation and later combination without remapping?
2. **physical density:** what structural cost follows from the selected width and fixed-field layout?

A wider ID should not be selected merely because it is larger, and a narrower ID should not be selected merely because it is denser.

## Compared variants

All variants retain:

- 16 KiB pages;
- exact object-record offset and length in each leaf locator;
- one inline SHA-256 object digest per leaf locator;
- inclusive min/max key ranges in pages and internal references;
- fixed-width deterministic page entries.

| Variant | ID | Object header | Page header | Leaf entry | Internal entry |
|---|---:|---:|---:|---:|---:|
| `draft-128` | 128-bit | 64 | 80 | 64 | 72 |
| `compact-128` | 128-bit | 56 | 64 | 64 | 72 |
| `compact-64` | 64-bit | 48 | 48 | 56 | 56 |

`draft-128` is the first self-contained EXP-0003 Draft.

`compact-128` removes avoidable fixed reserved space while retaining every currently proposed semantic field:

- a 56-byte object header leaves 8 reserved bytes after the 16-byte ID and two u64 lengths;
- a 64-byte page header leaves 12 reserved bytes after the 16-byte minimum and maximum IDs.

`compact-64` applies the same compactness principle to an 8-byte ID:

- 48-byte object header;
- 48-byte page header;
- 56-byte leaf locator;
- 56-byte internal child reference.

These are comparison layouts, not accepted byte formats.

## Derived 16 KiB geometry

| Variant | Leaf capacity | Internal fanout |
|---|---:|---:|
| `draft-128` | 254 | 226 |
| `compact-128` | 255 | 226 |
| `compact-64` | 291 | 291 |

The first Draft's 80-byte page header costs one leaf entry per page relative to the compact 128-bit header while not changing internal fanout.

## One million objects

| Variant | Level page counts | Directory bytes | Object-header bytes | Combined structural bytes | Bytes/object | Path bytes |
|---|---|---:|---:|---:|---:|---:|
| `draft-128` | `3938 / 18 / 1` | 64,831,488 | 64,000,000 | 128,831,488 | 128.831 | 49,152 |
| `compact-128` | `3922 / 18 / 1` | 64,569,344 | 56,000,000 | 120,569,344 | 120.569 | 49,152 |
| `compact-64` | `3437 / 12 / 1` | 56,524,800 | 48,000,000 | 104,524,800 | 104.525 | 49,152 |

## One hundred million objects

| Variant | Level page counts | Directory bytes | Object-header bytes | Combined structural bytes | Bytes/object | Path bytes |
|---|---|---:|---:|---:|---:|---:|
| `draft-128` | `393701 / 1743 / 8 / 1` | 6,479,101,952 | 6,400,000,000 | 12,879,101,952 | 128.791 | 65,536 |
| `compact-128` | `392157 / 1736 / 8 / 1` | 6,453,690,368 | 5,600,000,000 | 12,053,690,368 | 120.537 | 65,536 |
| `compact-64` | `343643 / 1181 / 5 / 1` | 5,649,694,720 | 4,800,000,000 | 10,449,694,720 | 104.497 | 65,536 |

At 100 million objects:

- compacting the 128-bit fixed headers without changing identifier semantics saves about **825.4 MB** of structural bytes versus the first Draft;
- compact 64-bit saves a further about **1.604 GB** versus compact 128-bit;
- total compact-64 versus first-draft-128 structural saving is about **2.429 GB** before payload bytes or optional indexes.

The object-header cost matters as much as the directory entry width at large object counts. Repeated reserved bytes should therefore require a concrete use rather than be retained by habit.

## One billion objects

| Variant | Level page counts | Directory bytes | Object-header bytes | Combined structural bytes | Bytes/object | Path bytes |
|---|---|---:|---:|---:|---:|---:|
| `draft-128` | `3937008 / 17421 / 78 / 1` | 64,790,659,072 | 64,000,000,000 | 128,790,659,072 | 128.791 | 65,536 |
| `compact-128` | `3921569 / 17353 / 77 / 1` | 64,536,576,000 | 56,000,000,000 | 120,536,576,000 | 120.537 | 65,536 |
| `compact-64` | `3436427 / 11810 / 41 / 1` | 56,496,603,136 | 48,000,000,000 | 104,496,603,136 | 104.497 | 65,536 |

The width decision therefore remains material even though all three trees have the same four-page root-to-leaf path at the 100-million and one-billion scales shown here.

## Random identifier collision model

For uniformly random independent IDs, the birthday approximation is:

```text
p(collision) = 1 - exp(-n(n-1) / (2 * 2^bits))
```

| Objects | Random 64-bit collision probability | Random 128-bit collision probability |
|---:|---:|---:|
| 1 million | ~2.71e-8 | ~1.47e-27 |
| 100 million | ~2.71e-4 (~0.0271%) | ~1.47e-23 |
| 1 billion | ~2.67e-2 (~2.67%) | ~1.47e-21 |

This table is relevant **only if the format expects independent random ID allocation in one namespace**.

It is not relevant to a coordinated allocator that checks uniqueness or assigns deterministic/sequential file-local IDs. A 64-bit namespace can represent far more than the project's current billion-object scale target when allocation is coordinated.

## Namespace semantics are the deciding requirement

### If `ObjectId` is a file-local structural handle

If the core contract is only:

- unique within one active file lineage;
- preserved where profile semantics require stable references;
- generated/validated by a coordinated writer;
- not advertised as a globally unique identity;

then 64 bits has a strong density and simplicity case.

Cross-file/global identity can remain a separate profile/application identifier or content digest rather than being overloaded onto the primary B+tree key.

This keeps the mandatory core smaller and avoids paying a global-namespace tax for profiles that do not need one.

### If `ObjectId` must support uncoordinated generation and merge

If the core requires producers to create object IDs independently and later combine their objects into one file without coordinated remapping, random 64-bit allocation is not a credible universal policy at 100-million-to-billion-object scale.

A 128-bit namespace is much more appropriate for that requirement, though the format must still define collision handling rather than treating probability as proof.

### If `ObjectId` is intended as global persistent object identity

The proposal should say so explicitly and justify why a structural directory key, rather than a profile-level UUID/content identity, should carry that semantic burden.

The current FCP wording intentionally calls `ObjectId` an opaque lookup key and says it is not a content digest or globally guaranteed unique name. That weakens the argument for paying the 128-bit cost unless uncoordinated allocation/merge is an actual core use case.

## Header compactness finding

Independent of identifier width, the first EXP-0003 Draft should not retain the 64-byte object header and 80-byte page header merely for convenient round numbers/reserve.

The compact 128-bit layout preserves every current field while reducing:

- object header from 64 to **56** bytes;
- page header from 80 to **64** bytes;
- leaf capacity changes from 254 to **255**;
- proposed leaf minimum would consequently change from 127 to **128** if the half-full policy remains.

At 100 million objects this saves about 825 MB of structural bytes without changing ID width or cryptographic semantics.

Before authoritative vectors are generated, Review should either adopt the compact headers or document a concrete extension use that justifies the permanent reserved-space tax.

## Security and interoperability considerations

- Identifier width does not provide authenticity; SHA-256 object/page identities remain separate.
- A writer must reject duplicate active IDs regardless of width.
- If IDs may be reused after deletion, profiles/history tooling need explicit semantics; width alone does not solve historical ambiguity.
- If objects can be imported from multiple files, collision/remapping behavior must be explicit at the profile/tool layer unless the core promises uncoordinated merge.
- Sequential or structured allocation can leak ordering/creation information; random allocation has privacy benefits but changes collision requirements.
- A globally meaningful ID may create tracking/linkability concerns across files that a file-local handle avoids.
- The format must not infer identifier-generation strategy from observed bytes.

## Current recommendation

Do **not** move the 128-bit choice from Draft to Review-ready policy until the core `ObjectId` namespace requirement is written explicitly.

In parallel, revise the first Draft toward **compact fixed headers** unless a concrete reserved-field use wins review.

A useful decision rule is:

- choose **64-bit** if `ObjectId` is intentionally a coordinated/file-local structural key and global/uncoordinated identity is profile-level;
- choose **128-bit** if uncoordinated generation and later no-remap combination is a core requirement.

For the universal-container architecture, there is a strong simplicity argument for keeping global semantic identity out of the mandatory structural key unless real Archive/Table/merge workloads prove otherwise.

## Reproduce

```console
python3 tools/experiment_exp0003_id_width.py
python3 tools/experiment_exp0003_id_width.py --json
```

The script uses only Python's standard library and deterministic arithmetic.

## Decision boundary

This experiment does not change FCP-0003, select 64 or 128 bits, revise the Draft bytes, or allocate EXP-0003. It makes the namespace assumption and repeated-header cost explicit before those choices become authoritative vectors.
