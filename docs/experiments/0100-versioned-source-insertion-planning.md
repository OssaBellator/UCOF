# Experiment 0100: versioned-source insertion planning

## Question

Can one canonical persistent insertion append tail be planned directly from a bounded strongly versioned random-access source without retaining the complete base file, while preserving the owned writer's exact bytes and accounting?

## Construction

The planner:

1. pins one opaque strong non-ABA source version;
2. strictly validates the exact-end active snapshot and all active objects;
3. independently validates canonical leaf and internal occupancy;
4. hashes the complete source to obtain a length/SHA-256 base identity;
5. appends the new object record;
6. rereads only the selected root-to-leaf path;
7. rejects a duplicate identifier;
8. rewrites that leaf and its ancestors, propagating splits and growing the root when required;
9. publishes the same absolute-offset snapshot/footer tail as the owned insertion writer.

All strict, canonical, identity, and path reads share one cumulative request/byte budget and one version token. Only the append tail is retained.

## Evidence

Deterministic Rust tests cover:

- ordinary insertion into a multi-page tree;
- a full root leaf that splits and grows a new root;
- duplicate identifier rejection;
- source-version change rejection;
- cumulative source-budget exhaustion.

The hostile fuzz target varies base size, insertion position, payloads, request sizes, the full-leaf split boundary, duplicates, and version mutation. Successful plans must match the owned writer's tail bytes, report, page-write count, and reuse count exactly.

## Result

The experiment provides a bounded-memory source-backed insertion planner for the current research layout. It does not claim minimal source traffic: strict active validation and the independent whole-file identity pass still read the active data and complete source. Deletion, shared multi-`Put`, and canonical mixed source-backed planners remain separate frontiers.
