# Experiment 0022: External Freshness Models

- **Status:** Reproducible model
- **Date:** 2026-07-30
- **Related:** FCP-0002 identity and rollback boundaries
- **Script:** `tools/experiment_exp0002_freshness_models.py`

## Question

Which rollback and fork cases can be detected by internal Candidate 1 fields, trust-on-first-use, trusted local latest state, or an online transparency service?

## Internal integrity only

A strictly valid older whole file carries self-consistent:

- sequence;
- parent links;
- snapshot digest;
- commit digest;
- object and page digests.

Replacing a newer complete file with that older complete file therefore remains strictly valid. Internal sequence ordering proves relationships inside the presented file, not that the presented file is the newest file ever published.

## Trust on first use

A local client records the first observed commit identity and sequence.

- A rollback already present on first use is accepted.
- After observing a newer sequence, the client can reject lower sequences.
- Recording the commit digest also detects a different commit at the same sequence.
- Protection is local to the stored state unless devices synchronize it securely.

## Preprovisioned or persisted trusted latest state

A trusted latest identity supplied out of band can reject rollback on first access.

The trusted-state update must be atomic with application acceptance. If a client accepts a new commit but crashes before persisting the new trusted state, a replay inside that window can still be accepted.

Multi-device state needs authenticated synchronization and conflict policy.

## Online transparency or witness

An online service can publish the latest accepted sequence and commit identities or provide append-only inclusion and consistency proofs.

This can detect older replay and same-sequence forks, but it introduces:

- online availability requirements;
- privacy leakage about accessed identities;
- witness trust and equivocation handling;
- proof retention and clock or epoch policy;
- behavior when the service is unreachable.

A client without a current proof cannot claim current freshness merely because the file is internally valid.

## Findings

1. Internal hashes and sequence provide integrity and internal ordering, not external freshness.
2. TOFU detects rollback only after one trusted observation.
3. Trusted state must record both ordering and exact identity to detect same-sequence forks.
4. Trusted-state updates need atomic application semantics.
5. Device-local state can diverge without secure synchronization.
6. Transparency can improve rollback and fork detection but adds online dependencies.
7. Phase 3 should expose snapshot and commit identities and make no freshness claim.
8. A future trust-layer proposal should define freshness separately from signatures, provenance, and source-view stability.

## Decision impact

FCP-0002 should not attempt to solve freshness with another self-contained field. It should require tools to report sequence, snapshot identity, commit identity, and verified history so an external trust policy has unambiguous inputs.

External freshness remains a later trust-layer and profile/application decision.

## Reproduction

```console
python3 tools/experiment_exp0002_freshness_models.py
```
