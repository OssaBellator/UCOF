# Experiment 0152 — operation-wide private-storage quota

**Status:** non-normative Phase 3 implementation evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiment 0151 end-to-end bounded source genesis

## Purpose

Experiment 0151 bounds source-genesis memory by replacing workload-wide metadata, locator, and page-frontier vectors with bounded sorting plus fixed private stages. It also exposes a separate production problem: individually bounded staging subsystems can still exceed an operation-wide disk budget when their live windows overlap.

This experiment adds a conservative scalar preflight for the private working set and rejects an undersized private-storage budget before creating the first descriptor stage.

The quota covers sorter-owned spill state plus the descriptor, locator, and page-reference working stages used by the Experiment 0151 candidate. It does not yet include a privately staged final output artifact.

## Overlap model

The candidate precomputes fixed stage sizes from object count, canonical tree geometry, and private record widths.

It then evaluates the live storage windows that can overlap during one operation.

### Sort plus final descriptor output

During the sorter's final output pass, sorter-owned runs can still be live while the retained 64-byte descriptor stage grows.

The conservative reservation is:

`configured max_live_spill_bytes + complete descriptor stage bytes`

Using the configured sorter ceiling rather than an observed run value deliberately avoids optimistic admission followed by a later combined-budget violation.

### Descriptor plus locator stage

During canonical object streaming the complete descriptor stage remains readable while the 72-byte-per-object locator stage grows.

The reservation is:

`complete descriptor stage bytes + complete locator stage bytes`

### Locator plus leaf-reference stage

Leaf construction consumes the locator stage while the first 64-byte page-reference stage grows.

The reservation is:

`complete locator stage bytes + complete leaf-reference stage bytes`

### Adjacent page-reference levels

Internal construction consumes one page-reference level while writing the next.

For each canonical level transition the candidate computes:

`current level bytes + next level bytes`

The maximum adjacent-level sum is retained in the plan.

## Conservative required quota

The operation-wide private working-set requirement is the maximum of all overlap windows above.

All arithmetic is checked. Canonical group counts are obtained through the allocation-free group-size iterator rather than by constructing page-size vectors.

The plan records:

- descriptor bytes;
- locator bytes;
- leaf-reference bytes;
- sorter-plus-descriptor reservation;
- descriptor-plus-locator reservation;
- locator-plus-leaf-reference reservation;
- maximum adjacent page-reference reservation;
- final required private-storage bytes.

## Admission point

The quota wrapper calls the planner before invoking descriptor preparation or any canonical output work.

If `required_bytes > max_private_storage_bytes`, it returns `private storage limit` immediately.

Therefore an undersized quota cannot create sorter runs, descriptor files, locator files, page-reference files, or canonical output bytes.

## Arithmetic regression

A 2,003-object planning case uses a deliberately reduced 100,000-byte sorter live-spill ceiling.

The checked plan is required to report:

- descriptor stage: `2,003 × 64` bytes;
- locator stage: `2,003 × 72` bytes;
- 11 leaf references: `11 × 64` bytes;
- sorter-plus-descriptor: `100,000 + 2,003 × 64` bytes;
- descriptor-plus-locator: `2,003 × (64 + 72)` bytes;
- locator-plus-leaf-reference: `2,003 × 72 + 11 × 64` bytes;
- adjacent leaf/root reference stages: `12 × 64` bytes.

For this geometry the descriptor-plus-locator window is the largest and therefore becomes the required quota.

The regression passes.

## Exact-versus-one-byte-short regression

A second test uses 401 source objects and the normal bounded sorter configuration for the candidate.

The planner computes one exact operation quota.

Two executions are then required:

1. **one byte short** — the writer must reject with `private storage limit`, write zero canonical output bytes, and leave the private directory empty;
2. **exact quota** — the bounded writer must succeed, match the existing canonical source writer byte-for-byte and report-for-report, keep the observed retained-stage peak within the planned quota, keep sorter live spill within its configured sub-limit, and leave the private directory empty after success.

Both assertions pass.

## Why the quota is intentionally conservative

The sorter reports actual peak live spill after execution, but an admission decision must be made before private work starts.

The first implementation therefore reserves the full configured `max_live_spill_bytes` simultaneously with the complete final descriptor stage. Actual operations can use less storage than this reservation.

A later production implementation may tighten admission if the sorter exposes a sound pre-execution live-spill bound specific to record count/run geometry. It must not substitute observed post-hoc usage for an enforceable preflight bound.

## Boundary: private output artifact not yet included

Experiment 0151 still writes canonical bytes directly to the caller's sink after metadata preflight.

When the bounded writer is integrated with private staged publication, the temporary final output artifact can coexist with working stages. That artifact must be added to the same operation-wide storage budget rather than governed by an unrelated limit.

Therefore this experiment closes the combined **working-stage** quota gap, not the complete production-operation quota.

## Other production boundaries

This experiment does not change the remaining issue #11 requirements:

- private records are still plaintext and unauthenticated;
- no operation key or nonce discipline is defined;
- no authenticated durable journal or restart authority is defined;
- stale-operation cleanup and quarantine remain unqualified;
- descriptor-relative hardened filesystem operations remain a separate backend concern;
- physical power-loss, network-filesystem, and supported-platform durability qualification remain open.

## Verification

Implementation head `4d43fb6dd5b306466496fe69c53d5f534676ca38` is green on the decisive implementation gates:

- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including all Experiment 0151 regressions and the new quota arithmetic/exact-versus-short regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The repository's longer protocol/policy/parser/vector replay continues after those gates and supplies broader regression confidence rather than changing the quota proof.

## Next executable slices

1. Add a source-version change after payload bytes have begun and prove that descriptor/locator/page-reference private working stages are retired even though the direct sink contains a terminal partial artifact and no report is returned.
2. Route bounded source genesis through private staged publication, then extend this quota to include the complete private output artifact in every relevant overlap window.
3. Consolidate duplicated test-only fixed-stage implementations into a typed private lifecycle module.
4. Add authenticated encryption to spill/stage records without changing canonical final bytes.
5. Add restart/discard authority and bounded stale-operation cleanup before proposing a production API.

## Governance boundary

This remains implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, modify proposed immutable-successor wire bytes, or make a compatibility promise.