# Experiment 0077: independent mixed transition recipes

## Question

Can the persistent canonical mixed writer's stable-shape, root-collapse, and root-growth transitions be reproduced independently from the Rust implementation while preserving strict validation, canonical occupancy, deterministic caller order, and exact current-page-body reuse?

## Independent writer

`verify_exp0002_immutable_mixed_transition_recipes.py` uses the existing clean-room Python object and immutable-page codec. It does not invoke or parse output from the Rust writer. For each recipe it:

- constructs a canonical base file from object inputs;
- strictly validates and inventories the authenticated current tree;
- preflights a unique operation set against the original active identifiers;
- appends only inserted and replacement object records in identifier order;
- derives the complete final locator set;
- partitions leaves and internal levels with the canonical occupancy rule;
- reuses a current page only when the complete encoded page body is byte-identical;
- appends and authenticates changed pages;
- publishes one linked successor commit;
- strictly validates the resulting objects, commit linkage, pages, and canonical occupancy.

## Recipes

The pinned contract covers:

1. a 400-object stable-height batch deleting object 700, inserting 701, and replacing 702, with two exact page bodies reused;
2. a 186-to-185 object transition collapsing a level-one root to one leaf;
3. a 185-to-186 object transition growing one root leaf into two leaves and one internal root.

Every recipe is regenerated in forward and reverse caller order. The output bytes, structural facts, page accounting, payload semantics, SHA-256 identity, and aggregate digest must agree.

## Boundary

These are non-normative successor research recipes, not a complete epoch transition corpus. They cover canonical mixed-operation transitions against the current 64-bit identifier and 88-byte locator experiment. Independent generation does not resolve the proposed epoch's identifier width, locator schema, policy acceptance, or external clean-room review requirements.
