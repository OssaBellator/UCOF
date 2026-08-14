# Experiment 0154 — bounded source genesis behind private publication

**Status:** non-normative Phase 3 implementation evidence  
**Date:** 2026-08-14  
**Tracking:** issue #11  
**Depends on:** Experiments 0151–0153 and the existing `PersistentStagingBackend` publication contract

## Purpose

Experiment 0153 made the direct-sink boundary explicit: bounded source genesis cleans up its private working state after a post-payload freshness failure, but the caller's sink can already contain a terminal partial artifact.

This experiment moves bounded canonical construction behind the repository's existing private staging/publication lifecycle and extends the operation-wide quota so the private final output artifact is not an uncounted second storage pool.

The candidate remains test-only and reuses the existing `PersistentStagingBackend` stages and outcomes. It does not define a competing publication protocol.

## Prepared construction seam

Bounded source genesis is split into two executable phases.

### Metadata/sort preparation

`prepare_bounded_preflight` performs the existing bounded source preflight:

- validates object-count and streaming configuration limits;
- acquires source metadata and initial strong versions;
- externally sorts fixed 64-byte source descriptors;
- rejects duplicate IDs and metadata/source-version acquisition failures;
- computes canonical tree shape and exact output length;
- checks output/file limits;
- returns a retained sorted descriptor stage plus scalar preflight state.

No canonical output bytes are written in this phase.

### Prepared emission

`write_prepared_bounded_candidate` consumes the prepared descriptor stage and performs:

- canonical file header emission;
- bounded source freshness checks and payload streaming;
- 72-byte locator staging;
- 185-entry bounded leaf construction;
- level-by-level 64-byte page-reference staging with at most 255 resident refs;
- existing snapshot/footer publication bytes.

A dedicated seam regression prepares 401 reverse-ordered sources, verifies the canonical sink is still empty, then emits from the prepared state and requires exact byte/report equality with `write_genesis_sources_to`.

This seam is what allows private output staging to begin only after all pre-output metadata/sort failures are resolved.

## Output-inclusive private-storage quota

Experiment 0152 budgets the private working stages. Private publication adds a final output artifact that can coexist with those stages after preflight.

The publication candidate therefore computes a second conservative plan:

- **before private output begins:** retain the existing `sorter live-spill ceiling + complete descriptor stage` reservation;
- **after private output begins:** reserve the **complete canonical output length** plus the maximum post-preflight working-stage overlap:
  - descriptor + locator;
  - locator + first page-reference level;
  - maximum adjacent page-reference levels.

The required publication quota is the maximum of the pre-output and post-output reservations.

This is intentionally conservative. During early object streaming the output file is not yet complete, but reserving the complete final output avoids phase-dependent admission races and makes the bound enforceable before descriptor staging begins.

## Existing publication state machine

After output-inclusive quota admission and metadata/sort preparation, the candidate follows the existing `PersistentStagingBackend` contract:

1. `begin_private(expected_length)`;
2. stream prepared bounded canonical bytes through a digest/count writer;
3. verify staged length;
4. `validate_private(expected_length, sha256)`;
5. `sync_private()`;
6. `publish_no_replace()`;
7. if linked, `sync_parent()`;
8. on durable success, `retire_private()`.

Failures during prepared construction, staged-length validation, private validation, private sync, or a definite publish error abort private output before returning an error.

A possible link remains indeterminate. A linked artifact whose parent sync fails also remains explicitly indeterminate and retains private state, matching the existing publication semantics rather than guessing whether durability was established.

## Durable exact-artifact regression

A 401-object reverse-ordered source set is written both through the current canonical source writer and the bounded staged-publication candidate.

The candidate must:

- produce the same canonical `ImmutableSourceStreamingWriteReport`;
- stage exactly the same bytes;
- report the SHA-256 of those bytes;
- publish those exact bytes to the destination;
- return `PublishedAndDurable { cleanup_pending: false }`;
- retire the private output artifact;
- retire all descriptor/locator/page-reference working stages.

The regression passes.

## Post-payload freshness failure becomes invisible

The Experiment 0153 changing-version source is reused.

Its third strong-version check changes only after the one-byte payload has been emitted into private output.

Under the staged-publication candidate the failure must:

- return no bounded publication evidence;
- invoke `abort_private()` exactly once;
- leave the destination absent;
- clear the private output artifact;
- retire all descriptor/locator working stages.

The regression passes.

This closes the visibility problem demonstrated by Experiment 0153 for the test backend: partial construction remains private and is discarded on a terminal source freshness failure.

## One-byte-short publication quota

The exact output-inclusive private-storage plan is computed before any private work begins.

A regression supplies `required_bytes - 1` and requires:

- `published private storage limit` rejection;
- no `begin_private` call;
- no private output bytes;
- no destination artifact;
- no descriptor/sorter/locator/page-reference files.

The regression passes.

Therefore the private final artifact is now part of admission rather than a second unbounded pool.

## No-overwrite destination-exists behavior

A backend with an existing destination returns `DestinationExists` from the no-replace publication step.

The candidate must:

- preserve the existing destination bytes unchanged;
- return `NotPublishedDestinationExists`;
- abort/retire the newly staged private artifact;
- clean all bounded working stages.

The regression passes.

## Parent-sync uncertainty remains explicit

A backend that links the private artifact but fails `sync_parent()` returns:

`PublicationIndeterminate { stage: SyncParent }`

The candidate must not abort or retire the private state in this case because the destination name may already refer to the artifact while namespace durability is uncertain.

The regression requires:

- destination bytes present;
- private state retained;
- no false durable-success claim;
- no generic error that discards the indeterminate distinction.

The regression passes.

## What this materially advances

The executable sequence now demonstrates one coherent bounded source-genesis path with:

- fail-before-output source descriptor sorting;
- fixed geometry-bounded tree RAM;
- operation-wide private working-stage quota;
- private final-output quota inclusion;
- canonical byte/report equivalence;
- source freshness before and after payload reads;
- private abort on construction failure;
- no-overwrite publication;
- explicit durable versus indeterminate publication outcomes.

That is substantially closer to the issue #11 production-writer shape than the earlier direct-sink experiments.

## Remaining production gaps

This experiment still does **not** make the writer production-ready.

The largest remaining gaps are:

- descriptor/locator/page-reference/spill records are plaintext and unauthenticated;
- no per-operation key derivation or nonce discipline is implemented;
- no authenticated durable restart journal exists;
- no restart/discard authority model is implemented;
- stale-operation cleanup and quarantine are not qualified;
- the memory backend proves state-machine semantics but not descriptor-relative filesystem hardening;
- physical power-loss and supported-filesystem durability qualification remain open;
- network-filesystem policy remains open;
- the research candidate duplicates some test-only staging code that must be consolidated before a production API is proposed.

The existing Unix and Linux staged-publication evidence remains relevant for the eventual filesystem backend; this experiment does not replace that work.

## Verification

Implementation head `6a6bda4fbedaa613f7d46bff2b5d1422f393a377` is green on the decisive implementation gates:

- workspace formatting;
- Clippy with warnings denied;
- full Rust implementation tests, including:
  - prepared-phase no-output seam;
  - durable exact-artifact publication;
  - post-payload freshness abort with destination absent;
  - one-byte-short output-inclusive quota rejection;
  - destination-exists no-overwrite preservation;
  - parent-sync indeterminate retention;
  - all Experiments 0151–0153 regressions;
- Rust 1.85.0 MSRV;
- i686 portability checks;
- powerpc64 portability checks.

The longer protocol/policy/parser/vector replay continues after those gates and supplies broader regression confidence rather than changing the publication result above.

## Next executable slices

1. Replace duplicated test-only fixed-stage implementations with one reusable private staging lifecycle and typed error model.
2. Add authenticated encrypted framing for source descriptors, locators, page references, and sorter runs while preserving deterministic final canonical bytes.
3. Define per-operation key/nonce provenance and prove nonce uniqueness across run creation, merge outputs, retained stages, restart, and discard.
4. Add an authenticated durable journal describing operation identity, private artifacts, stage state, and restart/discard authority.
5. Extend stale-operation cleanup with explicit byte/entry/time bounds and quarantine rules.
6. Re-run the bounded publication path through the hardened Unix/Linux backend and add physical durability/platform qualification before proposing a production API.

## Governance boundary

This remains implementation evidence only. It does not select EXP-0003 D1–D7, allocate an epoch, change immutable-successor wire bytes, or make a compatibility promise.