# Experiment 0103: versioned-source canonical mixed planning

## Question

Can a canonical deletion-plus-other-operation append tail be planned directly from a bounded strongly versioned random-access source without retaining the complete base file, while preserving the owned mixed writer's exact bytes and page-reuse decisions?

## Construction

The planner:

1. pins one opaque strong non-ABA source version;
2. strictly validates the exact-end active snapshot and all active objects;
3. independently validates canonical occupancy and authenticates the root range;
4. hashes the complete source to obtain a length/SHA-256 base identity;
5. authenticates and decodes every current page into bounded locator/page metadata;
6. sorts operations by object identifier and rejects duplicates, missing deletions, invalid identities, empty results, and non-mixed batches;
7. appends replacement/insertion records in canonical order and removes deletions from the locator set;
8. constructs the complete canonical leaf/internal grouping;
9. reuses an authenticated original page only when its locator or child-reference body is byte-identical at the same level;
10. emits the same snapshot/footer append tail as the owned canonical mixed writer.

All strict, canonical, identity, and inventory reads share one cumulative request/byte budget and one version token. The planner retains active locator and decoded page metadata but never retains base-file bytes.

## Evidence

Deterministic Rust tests cover:

- a stable-height deletion/replacement/insertion batch;
- caller-order independence;
- root collapse;
- root growth;
- missing deletions;
- duplicate operation identifiers;
- source-version mutation;
- cumulative source-budget exhaustion.

Successful plans must match `append_persistent_mixed_batch` tail bytes, report, page-write count, and page-reuse count exactly. Retained metadata capacity is reported separately from tail allocation.

## Result

The experiment closes source-backed planning for the final current persistent writer mode. It does not claim constant memory or minimal source traffic: the current active locator/page inventory is retained, strict validation and the whole-file identity pass remain complete reads, and production provider/publication qualification remains separate.
