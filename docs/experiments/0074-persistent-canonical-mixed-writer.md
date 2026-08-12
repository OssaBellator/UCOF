# Experiment 0074: persistent canonical mixed byte writer

## Question

Can a complete batch containing deletion plus insertion or replacement leave the full object-and-page rebuild path while preserving deterministic bytes and making only exact page-reuse claims?

## Writer

The persistent mixed writer:

- requires a canonical, strictly valid active tree;
- canonicalizes the complete operation set by identifier and rejects duplicate operations;
- validates all deletions against the original active set and preflights the final object count;
- appends only new or replacement object records;
- derives the final ordered locator set after all operations;
- applies the canonical full-construction occupancy grouping to final leaves and every internal level;
- inventories the current authenticated tree;
- reuses a current leaf only when its complete locator sequence exactly equals the final leaf body;
- reuses a current internal page only when its complete ordered child-reference sequence exactly equals the final internal body;
- emits and authenticates every changed page, then publishes one linked snapshot and footer;
- strictly validates canonical occupancy and page accounting before returning success.

## Evidence

Pinned byte-level cases cover:

- a 400-object stable-shape batch deleting one object, inserting one object, and replacing one object in the final leaf while reusing two untouched leaves and rewriting only the changed leaf and root;
- caller-order byte equivalence and general-API routing equivalence;
- selected-object rewrite proving inserted and replacement payload semantics while the deleted identifier is absent;
- collapse from a two-leaf 186-object tree to one 185-entry root leaf;
- growth from one 185-entry root leaf to two leaves and a root at 186 objects;
- missing and duplicate-operation rejection before publication.

## Boundary

This algorithm canonically regroups the complete final locator set. It is not the path-local leaf/internal borrow-and-merge planner modeled in Experiments 0064 and 0067, and it does not claim the minimum possible number of rewritten pages after every structural change. Its reuse claim is exact: only byte-equivalent current page bodies are reused. The implementation still materializes the active locator vector and output file, so streaming and spill integration remain separate work.
