# Experiment 0084: persistent mixed rewrite comparison

## Question

Does canonical locator regrouping write more authenticated pages than the path-local mixed repair planner on the pinned stable-height, root-collapse, and root-growth transitions?

## Construction

`compare_persistent_mixed_rewrites` inventories the authenticated current leaf bodies, converts the byte-writer operations to the identifier-level mixed planner, runs the planner with the executable page geometry, executes the canonical authenticated writer, and inventories its final leaf bodies.

The comparison is exact only when both paths choose the same final leaf partition. For those cases, the planner estimate counts every final leaf or internal page not covered by an untouched original page or ancestor. This estimate is conservative: the canonical writer may prove additional reuse by complete byte equality. If the canonical writer writes more pages under an equal final partition, the excess is evidence of an avoidable canonical-regrouping rewrite. When partitions differ, the report records both partitions but does not claim a rewrite delta.

## Evidence

The three independently pinned transition recipes all select equal final leaf bodies and equal rewrite counts:

| Case | Root transition | Final leaves | Planner estimate | Canonical writes | Reused pages | Relation |
|---|---|---:|---:|---:|---:|---|
| Stable height, 400 objects | stable | `185,122,93` | 2 | 2 | 2 | equal |
| 186 to 185 objects | collapsed | `185` | 1 | 1 | 0 | equal |
| 185 to 186 objects | grew | `93,93` | 3 | 3 | 0 | equal |

The `immutable_successor_persistent_mixed_comparison` fuzz target varies bounded root-leaf and multi-leaf bases, delete/replace batches, optional insertion, payload sizes, and caller operation order. It requires order-independent comparison reports. Whenever the final leaf partitions agree, it rejects any case where the canonical writer writes more pages than the conservative path-local estimate.

## Result

No avoidable rewrite is present in the three pinned structural boundary recipes. The comparison framework is intentionally capable of reporting divergent final partitions; broader fuzzing may identify cases where canonical global grouping and path-local repair choose different valid occupancy layouts. Such a divergence is a policy question, not automatically an implementation defect.

## Boundary

This experiment compares page counts, partitions, and reuse evidence for the current research geometry. It does not prove global minimality, assign normative preference to path-local repair, or resolve the proposed epoch layout. The planner is identifier-only and conservative for internal-page reuse. Provider I/O, spill, publication, and multi-snapshot history are outside this experiment.
