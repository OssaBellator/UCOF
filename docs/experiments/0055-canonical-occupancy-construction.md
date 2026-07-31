# Experiment 0055 — Canonical occupancy construction

**Status:** active byte-changing successor experiment  
**Date:** 2026-07-31

## Question

Can the immutable successor writer implement FCP-0003's half-full non-root occupancy rule deterministically in both Rust and an independent Python generator without changing page count or weakening exact-end validation?

## Policy under test

For capacity `C` and minimum `M = ceil(C / 2)`:

1. emit full pages left-to-right;
2. when the final remainder is at least `M`, emit it unchanged;
3. when the final remainder is below `M`, move exactly `M - remainder` entries from the penultimate page to the final page;
4. apply the same rule independently at every internal level;
5. permit a root leaf with one entry and require an internal root to contain at least two children.

This is deliberately not an even redistribution across all pages. For 400 objects and a 185-entry leaf capacity, the canonical leaf counts are `185, 122, 93`, not `134, 133, 133` and not the earlier sparse-tail shape `185, 185, 30`.

## Implementation

- Rust `build_tree` uses checked canonical group sizes for leaves and internal levels.
- `validate_canonical_occupancy` first performs complete strict validation, then performs a separately bounded page traversal enforcing root and non-root occupancy.
- Genesis, replacement rebuild, and mixed full rebuild self-check canonical occupancy before returning bytes.
- An independent Python module implements the same partition rule without calling the Rust implementation.
- Generated vector recipes require both complete Python validation and canonical occupancy validation.

## Compatibility boundary

This changes page boundaries and therefore page, snapshot, commit, and whole-file identities for affected trees. Earlier successor bytes remain disposable research evidence and are not accepted as canonical epoch bytes merely because loose structural validation succeeds.

Persistent insertion already splits an overflowing 186-entry leaf into `93, 93` and a 256-child internal page into `128, 128`, so split propagation preserves the selected minimum. Persistent deletion remains a separate change and may now target one explicit invariant.

## Required evidence

- boundary partitions at capacity, capacity plus one, minimum-minus-one remainder, exact minimum remainder, and exact multiples;
- the 400-object `185, 122, 93` shape;
- independently reproduced changed vector identity;
- rejection of authenticated non-root pages below minimum by the canonical validator;
- unchanged validity of one-page roots;
- full Rust, MSRV, portability, vector, integration, evidence, and fuzz matrices.

## Result interpretation

Passing this experiment establishes deterministic construction and strict occupancy checking for the current 64-bit/88-byte research microformat. It does not allocate `UCOF-EXP-0003`, accept FCP-0003, or migrate the proposed 128-bit/64-byte locator layout.
