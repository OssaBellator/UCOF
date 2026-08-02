# Phase 3 Candidate Disposition Draft

**Status:** Maintainer decision requested  
**Date:** 2026-07-31  
**Related:** FCP-0002, FCP-0003 Draft, `docs/PHASE_3_STATUS.md`

## Decision requested

Disposition `UCOF-EXP-0002` Candidate 1 as:

> **Superseded as the reusable-page Phase 3 baseline; retained indefinitely as disposable negative and security evidence.**

This document records a recommendation and the evidence needed for a maintainer decision. It does not itself accept or reject an FCP.

## What Candidate 1 proved

Candidate 1 remains valuable because it provides complete executable evidence for:

- deterministic Rust and independent Python bytes;
- authenticated object and paged-directory validation;
- exact-end active publication;
- complete append snapshots and checkpoints;
- bounded random-access lookup and absence;
- strict validation separated from recovery;
- fully verified linked history;
- repair-all and caller-selected rewrite;
- strong-version source-view modeling;
- valid, invalid, interrupted, adversarial, and fuzz corpora.

The candidate should therefore remain buildable and testable while Phase 3 is active. Removing it would discard regression evidence and obscure why the successor architecture changed.

## Blocking defect

Candidate 1 authenticates the active snapshot sequence inside every page and requires page sequence equality during validation. An unchanged historical page cannot be referenced by a later snapshot without changing the page bytes. Re-encoding changes its digest and every ancestor.

This contradicts the Phase 3 scale objective for safe page reuse. At the modeled 100-million-object scale, one replacement requires rewriting gigabytes of page bytes instead of one leaf-to-root path.

The defect is part of page identity and cannot be repaired by a more efficient writer while retaining Candidate 1 bytes.

## Proposed disposition

### FCP-0002

- Keep FCP-0002 in Draft while objections and historical findings are preserved.
- Add a prominent notice that Candidate 1 is not the promotion baseline.
- Close the proposal only after FCP-0003 or another successor proposal explicitly incorporates or rejects each still-relevant objection.
- Do not assign compatibility promises, media types, permanent registry values, or migration guarantees to Candidate 1.

### Implementation and corpora

- Retain Candidate 1 codec, vectors, experiments, and fuzz targets through Phase 3.
- Label public APIs and CLI commands as disposable research.
- Permit maintenance fixes that improve safety or reproducibility without changing the candidate's architectural disposition.
- Avoid adding new features to Candidate 1 unless needed for a controlled comparison with the successor.

### Successor

- Treat immutable content-addressed pages as the current proposal direction.
- Require a new experimental epoch if the successor is accepted.
- Do not reuse Candidate 1 magic, version, or page semantics.

## Objection transfer checklist

Before FCP-0002 is closed or marked superseded, every material objection must be classified as:

- resolved by successor policy;
- still open in FCP-0003;
- specific to Candidate 1 and retained as a rejected-alternative finding;
- out of scope for Phase 3 with an explicit future-phase owner.

At minimum, transfer review must cover:

- identifier width and namespace policy;
- locator density versus inventory range reads;
- page occupancy, split, redistribution, merge, and deletion rules;
- catalog and extension placement;
- checkpoint completeness;
- active-root, fork, history, and recovery assurance;
- remote source stability and retry semantics;
- unknown required and optional capability behavior;
- repair, rewrite, compaction, provenance, and signature consequences;
- spill confidentiality and durable publication;
- freshness and rollback resistance.

## Decision record template

A maintainer accepting this recommendation should append:

```text
Decision: Accepted | Revised | Rejected | Deferred
Date:
Maintainer:
Candidate 1 status:
FCP-0002 status:
Successor proposal:
Material objections transferred:
Required follow-up:
```

Until that record is completed, Candidate 1 remains executable but formally undispositioned.
