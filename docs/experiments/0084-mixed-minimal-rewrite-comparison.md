# Experiment 0084: Mixed Minimal-Rewrite Comparison

**Status:** Research evidence  
**Date:** 2026-08-01  
**Epoch allocation:** None

## Question

How many leaf pages does the authenticated canonical mixed writer rewrite because it globally regroups the final locator set, compared with the existing valid path-local borrow/merge/split planner for the same complete operation set?

## Construction

`compare_persistent_mixed_leaf_rewrites` strictly validates the canonical base, inventories its authenticated leaf bodies, and canonicalizes the mixed operation order. It then:

1. runs `plan_mixed_leaf_updates` over the original leaf identifier ranges;
2. constructs the same final authenticated locators used by the byte writer, including new record offsets and digests for replacements and insertions;
3. maps the path-local final identifier pages to those final locators;
4. independently derives the canonical global leaf grouping;
5. counts exact byte-body reuse against the original authenticated leaves for each layout.

The report includes original/final leaf counts, operation counts, path-local touched originals, exact leaf writes/reuse under both layouts, layout equality, the first differing leaf, final occupancy counts, and the non-negative extra canonical leaf-write count.

The comparison does **not** treat the path-local layout as canonical output. It measures the cost of the current canonical grouping rule while retaining the authenticated writer as the byte oracle.

## Boundary cases

For a canonical 400-object base with capacity 185 and minimum occupancy 93, the original leaf counts are `[185, 122, 93]`.

- Deleting object 2 and inserting object 1 keeps both layouts at `[185, 122, 93]`; each writes one leaf and reuses two.
- Deleting object 2 and replacing an object in the second leaf yields path-local `[184, 122, 93]` and canonical `[185, 121, 93]`, but both write the same two affected leaves.
- Deleting object 2 and replacing object 800 yields the same layout divergence, while the path-local layout writes the first and third leaves and reuses the second. Canonical regrouping shifts the first/second boundary and also rewrites the replaced third leaf, producing one additional leaf write.

## Fuzz evidence

`immutable_successor_mixed_rewrite_comparison` varies:

- two to 481 objects;
- one deletion and one distinct replacement;
- optional insertion;
- operation order;
- object payload size and replacement bytes;
- root-leaf and multi-leaf bases.

It verifies caller-order-independent reports, exact final object totals, write/reuse accounting, layout-difference reporting, and successful strict canonical validation of the authenticated writer output.

## Important boundary

`extra_canonical_leaf_writes` is a leaf-level comparison, not a complete-tree minimum. Parent pages can require rewrites when child references change, and a path-local leaf layout can produce different canonical bytes, snapshot identities, and commit identities. Adopting a path-local policy would require an explicit canonical-policy decision and regenerated cross-language vectors; this experiment only quantifies the current trade-off.
