# Phase 3 Candidate 1 Disposition Draft

**Status:** D1 maintainer decision requested; no decision selected  
**Original draft:** 2026-07-31  
**Rebased:** 2026-08-13 against the Draft→Review ledger  
**Related:** FCP-0002, FCP-0003 Draft, issues #13 and #76, `docs/review/FCP_0003_DRAFT_TO_REVIEW_LEDGER.md`

## Decision requested

This document now owns only **D1** from the unified FCP-0003 Draft→Review ballot:

> **Recommended:** `UCOF-EXP-0002` Candidate 1 is superseded as the reusable-page Phase 3 baseline and retained as disposable negative, security, interoperability, historical, and regression evidence. No migration or compatibility promise is created.

This recommendation is intentionally independent from the other ballot decisions.

Selecting D1 would **not** by itself:

- accept FCP-0003;
- select ObjectId/geometry, occupancy, deletion, catalog, hash/kind, or determinism policy;
- move FCP-0003 from Draft to Review;
- allocate `UCOF-EXP-0003`;
- make any current research vectors authoritative;
- start Phase 4 wire work.

## Why Candidate 1 should not remain the reusable-page baseline

Candidate 1 authenticates the active snapshot sequence inside every page and requires page-sequence equality during validation.

An unchanged historical page therefore cannot be referenced by a later snapshot without changing its bytes. Re-encoding changes the page digest and every ancestor.

That behavior conflicts with the Phase 3 immutable-page reuse objective. It is a wire-identity limitation, not a writer optimization problem.

The immutable-page successor research removes active snapshot sequence from page identity and has since demonstrated localized page reuse under replacement, insertion, deletion, mixed batches, history/recovery, bounded-source planning, and rewrite experiments.

## What Candidate 1 still proves

Candidate 1 remains valuable disposable evidence for:

- deterministic Rust and independent in-repository Python bytes;
- authenticated object and paged-directory validation;
- exact-end active publication;
- complete append snapshots/checkpoints;
- bounded random-access lookup and absence;
- strict validation separated from recovery;
- linked-history verification;
- repair/rewrite research;
- strong-version source-view modeling;
- valid, invalid, interrupted, adversarial, and fuzz corpora.

Its codec, vectors, experiments, and relevant fuzz targets should remain buildable while they provide regression/security value.

## D1 recommended disposition

If maintainers select the recommended D1 option, record the following consequences explicitly.

### Candidate 1

- Status: **superseded as reusable-page baseline**.
- Retain implementation/corpora/findings as disposable historical evidence.
- Permit safety/reproducibility maintenance without reopening feature expansion.
- Keep all Candidate 1 vectors visibly separate from future EXP-0003 candidate/authoritative vectors.
- Do not promise migration, byte compatibility, stable media types, or permanent registry values.

### FCP-0002

- Preserve the proposal and its findings as historical design evidence.
- Mark the reusable-page direction superseded by the immutable-page successor proposal direction.
- Keep rejected-alternative/security findings intact rather than deleting or silently reinterpreting them.
- Point current successor governance to FCP-0003 Draft and the Draft→Review ledger.

### Successor direction

- FCP-0003 remains **Draft** until the separate D1–D7 ballot and Draft→Review gates are completed.
- A successor experimental epoch remains unallocated until the later explicit allocation gate.
- No Candidate 1 epoch identity or compatibility promise carries into a future successor epoch.

## Objection transfer status

The detailed material-objection classification remains in:

- `docs/review/FCP_0002_TO_0003_OBJECTION_TRANSFER.md`.

The **current blocker/decision source of truth** is now:

- `docs/review/FCP_0003_DRAFT_TO_REVIEW_LEDGER.md`.

The old transfer record's first-Draft numeric blocker descriptions are historical context. Focused work has converted the remaining normative choices into D1–D7 decision surfaces:

```text
D1 Candidate 1 / FCP-0002 disposition
D2 ObjectId and primary-directory geometry
D3 occupancy / grouping / split
D4 deletion borrower policy
D5 catalog / roots / capabilities / extensions
D6 hash domains / magics / kind semantics
D7 scoped determinism
```

The transfer record still owns the provenance of how FCP-0002 objections were classified; it no longer supersedes the ledger as the live ballot.

## Broader Phase 3 gates remain separate

D1 does not resolve:

- #10 maintained real HTTP + one versioned cloud-object source + native async cancellation;
- #11 production-candidate spill confidentiality/durable publication/restart qualification;
- #12 independently maintained implementation or documented external clean-room review for Phase 3 exit;
- semantic/profile compaction convergence.

Those are implementation/exit requirements, not reasons to keep Candidate 1 as the reusable-page baseline.

## Maintainer ballot — D1 only

No option is selected by this document.

- [ ] **Recommended:** Candidate 1 is superseded as reusable-page baseline; retain it as disposable historical/security/regression evidence with no migration/compatibility promise.
- [ ] Retain Candidate 1 as a competing reusable-page baseline. Rationale: ______________________________
- [ ] Revise disposition: _____________________________________________________________________________
- [ ] Defer for one named blocker: ____________________________________________________________________

## Decision record template

After an explicit maintainer decision, replace this template with the actual record:

```text
D1 decision:
Date:
Maintainer:
Candidate 1 status:
FCP-0002 reusable-page direction status:
Evidence retained:
Compatibility/migration promise: none | revised: __________
Successor proposal status: FCP-0003 Draft unless separately changed
EXP-0003 allocation status: unallocated unless separately decided
Required follow-up:
```

Then make the following artifacts agree:

- FCP-0002 status/disposition notice;
- this disposition record;
- `docs/review/FCP_0002_TO_0003_OBJECTION_TRANSFER.md` current-status note;
- `docs/review/FCP_0003_DRAFT_TO_REVIEW_LEDGER.md` D1 record;
- `docs/PHASE_3_STATUS.md`;
- `docs/EXP_0003_INTEROP_PLAN.md`;
- issues #13 and #76;
- Candidate 1 / future EXP-0003 corpus labels.

## Boundary

Until a maintainer explicitly records D1, Candidate 1 remains executable and formally **undispositioned**, even though the repository recommendation is to supersede it as the reusable-page baseline.
