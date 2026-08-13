# Experiment 0137 — EXP-0003 fixed-header reserve and alignment

**Status:** non-normative research evidence  
**Date:** 2026-08-13  
**Related:** Experiments 0108, 0135, 0136; FCP-0003; issues #13, #16, #76

## Question

Experiment 0108 showed that the first EXP-0003 Draft pays avoidable fixed-header cost, but its comparison layouts still retain reserve in every object header:

```text
compact-128 object header: 56 bytes, including 8 reserved bytes
compact-64  object header: 48 bytes, including 8 reserved bytes
```

The same comparison also leaves page-header reserve:

```text
compact-128 page header: 64 bytes
compact-64  page header: 48 bytes
```

Those two kinds of reserve have very different economics:

- object-header reserve is emitted once per object and permanently consumes file bytes;
- page-header reserve lives inside a fixed 16 KiB page, so reducing it saves no physical page bytes unless the smaller header crosses an entry-capacity threshold.

This experiment asks how far the fixed headers can be tightened while preserving 8-byte entry alignment and the full-range child-reference recommendation from Experiment 0136.

## Exact semantic field sizes

### 128-bit ObjectId object header

Required fields consume exactly 48 bytes:

```text
magic                    8
header length            2
object kind              2
flags                    4
ObjectId                16
stored payload length    8
logical payload length   8
                        --
                        48
```

A **48-byte** object header therefore carries every currently proposed EXP-0003 object-header semantic field with no reserve.

### 64-bit ObjectId object header

The same fields with an 8-byte ObjectId consume exactly 40 bytes:

```text
magic                    8
header length            2
object kind              2
flags                    4
ObjectId                 8
stored payload length    8
logical payload length   8
                        --
                        40
```

A **40-byte** object header is already 8-byte aligned and needs no filler.

Transforms/compression are outside EXP-0003, so there is no currently accepted transform field that justifies paying an 8-byte reserve in every object record.

## Exact page-header field sizes

With the current proposed page fields:

```text
magic            8
kind             1
level            1
header length    2
entry count      4
entry width      2
flags            2
minimum ID       I
maximum ID       I
```

semantic bytes total:

```text
128-bit ObjectId: 52 bytes
64-bit ObjectId:  36 bytes
```

To keep the first packed entry 8-byte aligned, the smallest aligned headers are therefore:

```text
128-bit: 56 bytes = 52 semantic + 4 zero reserved
64-bit:  40 bytes = 36 semantic + 4 zero reserved
```

The 4 zero bytes are alignment filler, not claimed future extension capacity.

## Compared layouts

All variants retain 16 KiB pages, SHA-256 locator/page authentication, explicit child minimum + maximum ranges, and fixed-width entries.

| Variant | ID | Object header | Page header | Leaf entry | Internal entry |
|---|---:|---:|---:|---:|---:|
| `draft-128` | 128 | 64 | 80 | 64 | 72 |
| `compact-128` | 128 | 56 | 64 | 64 | 72 |
| `tight-128` | 128 | **48** | **56** | 64 | 72 |
| `compact-64` | 64 | 48 | 48 | 56 | 56 |
| `tight-64` | 64 | **40** | **40** | 56 | 56 |

## Page-capacity plateau

The important result is that tightening the page header does **not** change capacity relative to the already-compacted layouts:

| Variant | Leaf capacity | Internal fanout |
|---|---:|---:|
| `compact-128` | 255 | 226 |
| `tight-128` | 255 | 226 |
| `compact-64` | 291 | 291 |
| `tight-64` | 291 | 291 |

Therefore:

```text
compact-128 page 64 -> tight-128 page 56
compact-64  page 48 -> tight-64  page 40
```

changes where zero bytes sit inside the fixed page but does not reduce the number of 16 KiB pages.

For a fully occupied page, the removed header reserve simply becomes additional zero padding at the end of the page. The complete page remains the same size and remains fully authenticated.

This means page-header reserve should be judged mainly on parser/layout clarity, not storage density, until a proposed field actually crosses a capacity threshold.

## Per-object reserve is different

Removing the remaining 8-byte object-header reserve saves exactly:

```text
8 * object_count bytes
```

because every active or historical object record carries its own header.

At 100 million objects:

```text
8 * 100,000,000 = 800,000,000 bytes
```

At one billion objects:

```text
8 * 1,000,000,000 = 8,000,000,000 bytes
```

That is real file growth, not a layout-only relocation of page padding.

## Structural-byte comparison

### One hundred million objects

| Variant | Directory bytes | Object-header bytes | Combined structural bytes | Bytes/object |
|---|---:|---:|---:|---:|
| `draft-128` | 6,479,101,952 | 6,400,000,000 | 12,879,101,952 | 128.791 |
| `compact-128` | 6,453,690,368 | 5,600,000,000 | 12,053,690,368 | 120.537 |
| `tight-128` | 6,453,690,368 | 4,800,000,000 | **11,253,690,368** | **112.537** |
| `compact-64` | 5,649,694,720 | 4,800,000,000 | 10,449,694,720 | 104.497 |
| `tight-64` | 5,649,694,720 | 4,000,000,000 | **9,649,694,720** | **96.497** |

At this scale:

- `tight-128` saves exactly **800,000,000** bytes versus `compact-128`;
- `tight-64` saves exactly **800,000,000** bytes versus `compact-64`;
- `tight-128` saves about **1.625 GB** versus the first Draft;
- `tight-64` saves about **3.229 GB** versus the first Draft;
- `tight-64` remains about **1.604 GB** smaller than `tight-128` because identifier width still affects object headers and directory entries.

### One billion objects

| Variant | Combined structural bytes | Bytes/object |
|---|---:|---:|
| `draft-128` | 128,790,659,072 | 128.791 |
| `compact-128` | 120,536,576,000 | 120.537 |
| `tight-128` | **112,536,576,000** | **112.537** |
| `compact-64` | 104,496,603,136 | 104.497 |
| `tight-64` | **96,496,603,136** | **96.497** |

The remaining per-object reserve is therefore large enough to deserve an explicit semantic justification. No such justification currently exists in EXP-0003 scope.

## Why keep four alignment bytes in the page header

The semantic page header ends at byte 52 for 128-bit IDs or byte 36 for 64-bit IDs.

Starting fixed-width entries at byte 56 or 40 keeps entry starts 8-byte aligned. That is useful for straightforward cross-language parsing and avoids creating an odd unaligned packed-entry base merely to move four zero bytes to tail padding.

The four bytes remain required zero in this experimental epoch.

They should not be advertised as generic forward-compatibility reserve. If a future incompatible epoch assigns new semantics, it must define those bytes explicitly under that epoch's rules.

## Why not retain eight object-reserve bytes for future transforms

Transforms/compression are intentionally outside EXP-0003. Paying eight bytes in every object now for an undefined later transform grammar would couple future service semantics into the structural core before those semantics exist.

A later incompatible epoch/profile can define whatever transform metadata it actually needs. EXP-0003 should test the smallest structural object record that satisfies its own scope.

## Consequence for the geometry decision

Combining Experiments 0135–0137 now yields two clean full-range candidates:

### Tight 128-bit

```text
ObjectId:             16 bytes
object header:        48 bytes
page header:          56 bytes
leaf locator:         64 bytes
internal reference:   72 bytes
leaf capacity:       255
internal fanout:     226
leaf minimum:        128
internal minimum:    113
```

### Tight 64-bit

```text
ObjectId:              8 bytes
object header:        40 bytes
page header:          40 bytes
leaf locator:         56 bytes
internal reference:   56 bytes
leaf capacity:       291
internal fanout:     291
leaf minimum:        146
internal minimum:    146
```

The 64-bit candidate has an additional simplicity property:

> leaf and internal pages have the same entry width, maximum occupancy, minimum occupancy, and overflow split count.

With half-full occupancy:

```text
C = 291
M = ceil(291 / 2) = 146
C + 1 = 292 -> split 146 / 146
```

That reduces the number of distinct byte-significant boundary constants implementations must reproduce.

## Current recommendation

Before authoritative EXP-0003 vectors are generated:

1. **remove the remaining per-object fixed reserve** unless Review identifies a concrete EXP-0003 field that needs it;
2. use the **smallest 8-byte-aligned page header** that carries the accepted page fields;
3. keep explicit full child ranges per Experiment 0136 unless a separate Review decision reverses that trade-off;
4. make the final 64-vs-128 ObjectId choice from the namespace contract and total geometry, not from round-number header sizes.

This experiment does not by itself select identifier width, but it removes one avoidable confounder from that decision.

## CI assertions

`tools/experiment_exp0003_header_reserve.py` runs in the normal experiment block and requires:

- semantic object-header field sums of exactly 48 bytes (128-bit) and 40 bytes (64-bit);
- semantic page-header field sums of 52 and 36 bytes respectively;
- tight aligned page headers of 56 and 40 bytes;
- unchanged compact/tight page capacity plateaus;
- exact 800,000,000-byte savings from removing 8 object-header bytes at 100M objects;
- equal leaf/internal capacity 291 for the tight 64-bit full-range candidate.

## Boundary

This experiment does **not**:

- edit the EXP-0003 Draft;
- select 64-bit or 128-bit ObjectId;
- change catalog/snapshot bytes;
- change current Rust research bytes;
- accept FCP-0003;
- allocate EXP-0003;
- regenerate authoritative vectors.

The next artifact should be a bounded **identifier + geometry decision packet**, not another sequence of header-size experiments.

## Reproduction

```console
python3 tools/experiment_exp0003_header_reserve.py
```
