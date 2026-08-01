# Experiment 0067: recursive mixed tree shape planning

## Question

Can the simultaneous mixed leaf repair result be propagated into deterministic internal occupancy and root-height decisions before integrating byte emission?

## Model

The recursive planner:

- invokes the order-independent mixed leaf planner;
- constructs original and final canonical internal levels with explicit fanout and minimum occupancy;
- applies the root exception only when all children fit in one page;
- independently limits depth and total internal pages;
- reports leaf-through-root page counts and every internal grouping;
- reports stable, grown, or collapsed root height;
- maps unchanged shapes to the touched original ancestor path;
- conservatively marks a complete original level and every ancestor when grouping changes, avoiding false page-reuse claims.

## Evidence

Pinned cases cover:

- one replacement rewriting only one original path through a two-level tree;
- a simultaneous leaf split growing a full level-two tree to level three;
- a simultaneous leaf merge collapsing a level-two tree to level one;
- caller-order invariance;
- depth-limit failure before unbounded grouping.

## Boundary

This remains an identifier and shape planner. It does not retain locator payloads, assign new page offsets or digests, coordinate exact reusable references across structural shifts, emit internal pages, or publish a commit. Structural-change accounting is deliberately conservative.
