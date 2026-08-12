# Experiment 0010: EXP-0002 Authenticated Locator Widths

- **Status:** Reproducible model evidence
- **Date:** 2026-07-30
- **Related:** FCP-0002, Candidate 1 byte draft, Experiments 0005 and 0006
- **Script:** `tools/experiment_exp0002_locator_widths.py`

## Question

How much of Candidate 1's large-directory cost comes from the 88-byte leaf entry, and what is the effect of wider object identifiers?

## Invariants held constant

Every compared locator retains:

- an object identifier used as the ordered lookup key;
- exact object-record offset and record length;
- an inline 32-byte cryptographic object digest;
- 16 KiB pages with a 64-byte page header;
- 64-byte internal entries and fanout 255;
- the same root-to-leaf authentication model.

The experiment does not compare unauthenticated indexes. The smaller variants remove fields that can be authenticated by reading the object header rather than removing the digest.

## Variants

| Variant | Entry bytes | Identifier | Mirrored kind | Mirrored logical length | Trailing reserved |
|---|---:|---:|---|---|---:|
| Candidate 1 baseline | 88 | 64-bit | yes | yes | 16 bytes |
| Tight same fields | 72 | 64-bit | yes | yes | none |
| Minimal authenticated | 56 | 64-bit | no | no | none |
| Minimal authenticated, 128-bit identifier | 64 | 128-bit | no | no | none |
| Baseline fields, 128-bit identifier | 96 | 128-bit | yes | yes | 16 bytes |

The minimal locator contains identifier, record offset, record length, and object digest. Kind and logical length remain authenticated because they are inside the digested object header, but a metadata-only inventory would need an additional object-header read to obtain them.

## Results

| Variant | Leaf capacity | 1 million objects | 100 million objects | 100M tree depth |
|---|---:|---:|---:|---:|
| Candidate 1 baseline | 185 | 84.83 MiB | 8.280 GiB | 4 |
| Tight same fields | 226 | 69.44 MiB | 6.778 GiB | 4 |
| Minimal authenticated | 291 | 53.94 MiB | 5.264 GiB | 4 |
| Minimal authenticated, 128-bit identifier | 255 | 61.55 MiB | 6.007 GiB | 4 |
| Baseline fields, 128-bit identifier | 170 | 92.31 MiB | 9.011 GiB | 4 |

At 100 million objects, removing only the 16 reserved bytes saves about 1.50 GiB. Removing both mirrored fields and trailing reserve saves about 3.02 GiB relative to Candidate 1. A minimal 128-bit-identifier locator is still smaller than the current 64-bit baseline.

## Findings

### Reserved expansion space is unusually expensive in repeated entries

Sixteen reserved bytes appear modest in one structure but cost roughly 1.6 billion bytes across 100 million leaf entries before internal-page overhead. Future expansion should prefer a versioned entry layout or authenticated side metadata over permanent per-entry reserve unless a concrete extension justifies it.

### Mirrored fields trade range reads for directory density

Keeping kind and logical length in the leaf allows structural inventory without reading each object header and provides an early cross-check. Removing them does not remove integrity because the object digest covers the header, but it changes the access pattern:

- lookup and extraction already read the object header, so the fields are redundant there;
- bulk metadata inventory may incur many additional small range reads;
- remote storage may value fewer requests more than fewer directory bytes.

This trade-off requires a cold-cache range benchmark before a final choice.

### Identifier width and locator width are separable

Moving from 64-bit to 128-bit identifiers adds 8 bytes per locator. That cost can be more than offset by removing redundant mirrored fields and reserve. Identifier collision policy should therefore be decided from namespace and longevity requirements, not by assuming the current 88-byte shape is fixed.

### Tree depth is not the differentiator at this scale

All variants remain depth 4 at 100 million objects with 16 KiB pages. The main effect is total directory bytes and leaf-page count, not authenticated lookup depth.

## Security implications

- Inline object digests remain mandatory in every compared variant.
- Removing mirrored fields must not permit callers to trust kind or logical length before authenticating and parsing the object header.
- A versioned compact layout must fail closed on unknown entry versions and must not infer digest algorithms from length.
- Wider identifiers do not provide authenticity and do not replace digest collision resistance.
- Variable-length entries could improve density further but would add offset-table, canonicalization, and parser-differential risks not measured here.

## Recommendation for Candidate 1 review

Do not accept the 88-byte leaf entry merely because the codec works. Before FCP-0002 moves to Review, compare at least:

1. the current 88-byte inventory-friendly entry;
2. a 72-byte no-reserve entry;
3. a 56-byte authenticated locator with on-demand object-header inventory;
4. a 64-byte compact entry with 128-bit identifiers.

The decision should use measured local and HTTP-range workloads, not directory size alone.

## Reproduction

```console
python3 tools/experiment_exp0002_locator_widths.py
```

The script asserts page capacities, the Candidate 1 100-million-object shape, all size orderings, constant depth, and more than 3 GiB savings for the minimal 64-bit authenticated locator.
