# Phase 3 Candidate Disposition Draft

**Status:** Maintainer decision requested  
**Original draft:** 2026-07-31  
**Rebased:** 2026-08-13 against consolidated `main`  
**Related:** FCP-0002, FCP-0003 Draft, issues #13 and #76

## Decision requested

Disposition `UCOF-EXP-0002` Candidate 1 as:

> **Superseded as the reusable-page Phase 3 baseline; retained as disposable negative, security, interoperability, and regression evidence.**

This document records a recommendation and the evidence needed for a maintainer decision. It does not itself accept or reject an FCP and does not allocate `UCOF-EXP-0003`.

## What Candidate 1 proved

Candidate 1 remains valuable because it provides executable evidence for:

- deterministic Rust and independent in-repository Python bytes;
- authenticated object and paged-directory validation;
- exact-end active publication;
- complete append snapshots and checkpoints;
- bounded random-access lookup and absence;
- strict validation separated from recovery;
- fully verified linked history;
- repair-all and caller-selected rewrite;
- strong-version source-view modeling;
- valid, invalid, interrupted, adversarial, and fuzz corpora.

Removing it would discard regression evidence and obscure why the successor architecture changed.

Candidate 1 should remain buildable and testable while its evidence is still used to validate later design claims.

## Blocking defect

Candidate 1 authenticates the active snapshot sequence inside every page and requires page-sequence equality during validation.

An unchanged historical page therefore cannot be referenced by a later snapshot without changing its bytes. Re-encoding changes its digest and every ancestor.

This contradicts the Phase 3 reusable-page scale objective. At the modeled 100-million-object scale, one replacement requires rewriting gigabytes of page bytes rather than one leaf-to-root path.

The defect is part of page identity and cannot be repaired by a more efficient writer while retaining Candidate 1 page bytes.

## Post-consolidation evidence

Since the original disposition draft, the immutable-page successor implementation has advanced substantially and was consolidated into `main` by PR #75.

The consolidated research baseline now includes evidence for:

- canonical half-full occupancy in the current research geometry;
- persistent insertion and split propagation;
- persistent deletion, borrowing, merge, recursive underflow repair, and root collapse;
- canonical mixed batches with exact safe page reuse;
- bounded streaming append-tail output;
- source-backed replacement, insertion, deletion, multi-`Put`, and mixed planning;
- selected/history output and per-state semantic-selection planning;
- conditional transport/retry/wait/authentication policy;
- Unix research publication, directory pinning, fault/restart evidence;
- continuous Rust, portability, vector, evidence, property, and fuzz gates.

This additional evidence strengthens the recommendation to move the reusable-page direction to a new experimental epoch rather than extending Candidate 1.

## Proposed disposition

### FCP-0002

- Preserve FCP-0002 and Candidate 1 findings as historical design evidence.
- Add a prominent disposition notice that Candidate 1 is not the reusable-page promotion baseline.
- Classify each material objection as transferred to FCP-0003, specific to Candidate 1/rejected alternatives, or explicitly owned by a later phase.
- Do not assign compatibility promises, stable media types, permanent registry values, or migration guarantees to Candidate 1.
- Once the objection-transfer record is complete and FCP-0003 is accepted for experimentation, mark the reusable-page direction of FCP-0002 superseded rather than leaving two apparent successor baselines.

### Implementation and corpora

- Retain Candidate 1 codec, vectors, experiments, and relevant fuzz targets while they provide regression/security value.
- Label public APIs and CLI commands as disposable research.
- Permit maintenance fixes that improve safety or reproducibility without changing Candidate 1's architectural disposition.
- Avoid adding new feature scope to Candidate 1 unless needed for a controlled comparison or regression test.
- Keep Candidate 1 vectors clearly separated from future EXP-0003 authoritative vectors.

### Successor

- Treat immutable content-addressed pages as the current proposal direction.
- Require a new experimental epoch if that successor is accepted for interoperability work.
- Do not reuse Candidate 1 epoch identity or imply byte compatibility.
- Require the new epoch to be specified independently enough that a clean-room implementation can reproduce authoritative vectors without reading reference-source internals.

## Objection transfer checklist

Before FCP-0002 is marked superseded for the reusable-page direction, every material objection must be classified as:

- resolved by successor policy;
- still open in FCP-0003;
- specific to Candidate 1 and retained as a rejected-alternative/security finding;
- out of scope for Phase 3 with an explicit future-phase owner.

At minimum, transfer review must cover:

- identifier width and namespace policy;
- locator density versus inventory range reads;
- page geometry and authenticated child-reference layout;
- page occupancy, split, redistribution, merge, and deletion rules;
- canonical final-state identity versus historical page reuse;
- catalog/capability/extension placement;
- checkpoint completeness;
- active-root, fork, history, and recovery assurance;
- remote source stability, retry, cancellation, and authentication semantics;
- unknown required and optional capability behavior;
- repair, rewrite, compaction, provenance, and signature consequences;
- spill confidentiality and durable publication;
- freshness and rollback resistance;
- experimental migration non-promises.

## Recommended decision

The evidence now supports the following maintainer decision:

```text
Decision: Accepted
Date: <maintainer decision date>
Maintainer: <name or handle>
Candidate 1 status: Superseded as reusable-page baseline; retained as disposable evidence
FCP-0002 status: Preserve historical proposal/evidence; reusable-page direction superseded after objection transfer
Successor proposal: FCP-0003 Draft
Proposed successor epoch: UCOF-EXP-0003 only after explicit FCP acceptance for experimentation
Material objections transferred: See committed objection-transfer record
Required follow-up: Complete #13, #16, #76 and Phase 3 exit gates #10-#12
```

The pre-filled wording is a recommendation only. A maintainer may revise, reject, or defer it.

## Decision record

A maintainer should replace the placeholder record above with the actual decision and ensure the following artifacts agree:

- FCP-0002 status/disposition notice;
- FCP-0003 status;
- `docs/PHASE_3_STATUS.md`;
- issue #13;
- issue #76;
- EXP-0002 and future EXP-0003 specification/corpus labels.

Until that record is committed, Candidate 1 remains executable but formally undispositioned.
