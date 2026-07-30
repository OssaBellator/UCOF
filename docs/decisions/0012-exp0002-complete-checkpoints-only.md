# ADR-0012: Use Complete Snapshot Commits as EXP-0002 Checkpoints

- **Status:** Accepted for Candidate 1
- **Date:** 2026-07-30
- **Decision owners:** UCOF maintainers
- **Related:** FCP-0002, ADR-0010, Phase 3 publication and recovery models
- **Normative impact:** Experimental candidate only

## Context

Phase 3 requires checkpoints for interrupted append workloads, but “checkpoint” can mean two materially different things:

1. a complete independently valid snapshot published by an ordinary commit footer;
2. a partial progress marker that may name incomplete object, directory, transform, or application state.

The abstract Phase 3 model deliberately represented both complete and progress checkpoints to test selection rules. Candidate 1 then implemented exact bytes only for complete snapshots and exact-end commit footers. Progress checkpoint bytes, validity, directory semantics, recovery priority, capability requirements, and repair behavior remained unresolved.

Adding a second partially authoritative root mechanism would expand the trusted parser and recovery surface before there is a demonstrated workload that cannot use ordinary complete snapshots at an appropriate cadence.

## Decision

For EXP-0002 Candidate 1:

1. **A checkpoint is an ordinary complete snapshot commit.** It uses the same snapshot record, directory rules, object authentication, commit digest, and footer publication as every other complete commit.
2. Candidate 1 defines no progress-checkpoint record, flag, magic, or recovery status.
3. Only a complete footer at exact prefix end can make a checkpoint independently valid and recovery eligible.
4. Interrupted bytes written after the latest complete checkpoint remain unpublished tail state.
5. Writers choose checkpoint cadence as application policy under explicit overhead and durability trade-offs.
6. Readers do not need a second parser or reduced-validity mode for partial progress.
7. A future proposal may introduce progress checkpoints only with exact use cases, byte semantics, capability signalling, resource limits, recovery priority, repair rules, vectors, and adversarial evidence.

## Consequences

### Positive

- one validity model applies to normal snapshots and checkpoints;
- strict validation and recovery do not need partial-root exceptions;
- every checkpoint is independently readable and repairable;
- previous-footer traversal already provides checkpoint history;
- interrupted writes cannot make partial semantic state look complete;
- cross-language vectors and fuzzing reuse the ordinary commit path.

### Negative

- a complete checkpoint must finish its directory and snapshot metadata;
- without copy-on-write page reuse, frequent checkpoints rebuild the directory and can be expensive;
- very large in-progress objects cannot expose partially written payload state through Candidate 1;
- applications needing sub-object progress must split data into complete independently published objects or wait for a future proposal.

## Security analysis

Progress checkpoints would create pressure to relax one or more of:

- complete object authentication;
- complete directory reachability;
- root eligibility;
- exact-end publication;
- strict versus recovery separation;
- repair-source requirements.

Candidate 1 avoids those ambiguous states. A checkpoint is valid only if it satisfies the same checks as an ordinary active snapshot.

Checkpoint frequency does not provide trusted freshness. An attacker can still replace the whole file with an older valid checkpoint unless external trusted state records a newer acceptable commit identity.

## Operational guidance

Writers should choose checkpoint cadence from:

- maximum acceptable lost work after interruption;
- object creation rate and payload size;
- current directory rebuild or page-reuse cost;
- storage latency and durability guarantees;
- application-level atomic grouping requirements.

A high-rate capture profile should prefer many bounded complete objects and periodic complete snapshot publication rather than one indefinitely incomplete object.

## Alternatives considered

### Partial directory checkpoint

Rejected for Candidate 1. It would require defining whether missing pages and objects are invalid, absent, pending, or advisory, and whether a later reader can safely extract partial state.

### Progress footer with weaker integrity

Rejected. A weaker footer would invite assurance confusion and expand recovery candidate attacks.

### Progress checkpoint never eligible as a root

Deferred. Such a marker could still be useful for application-specific resumption, but it would not be a UCOF snapshot and belongs in a future advisory object or profile proposal with explicit semantics.

### Fixed mandatory checkpoint cadence

Rejected. Workloads differ too much for one cadence, and cadence is not a wire-format validity property.

## Validation requirements

Candidate 1 evidence must continue to show:

- every incomplete append cut fails strict validation;
- earlier complete prefixes remain recoverable;
- ordinary append vectors serve as complete checkpoint vectors;
- previous-footer chain enumeration reports every verified complete checkpoint;
- repair accepts complete checkpoints and rejects unpublished tails;
- checkpoint cadence overhead is measured with both full rebuild and copy-on-write assumptions.

## Revisit conditions

Revisit only when a concrete use case demonstrates that complete snapshots cannot meet its recovery objective within acceptable overhead, and a proposed progress representation can preserve explicit assurance boundaries.
