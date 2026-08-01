# Phase 3 mixed transition evidence handoff

The independent Python transition recipes in Experiment 0077 are intended to review the byte and structural claims made by the reusable Rust mixed writer without treating the Rust output as an oracle.

Reviewers should confirm:

- operation identifiers are canonicalized and duplicates are rejected;
- delete targets are checked against the original active state;
- inserted and replacement object records are appended in identifier order;
- final leaf and internal grouping follows the documented canonical occupancy partition;
- reuse requires byte-identical locator or child-reference sequences, including offsets and digests;
- changed pages and the successor commit are independently authenticated;
- reverse caller order produces identical bytes;
- stable-height, root-collapse, and root-growth recipes match their pinned facts and aggregate digest.

This packet does not claim coverage for every mixed batch, a production spill path, a streaming persistent writer, the proposed wider identifier/locator schema, or external interoperability acceptance.
