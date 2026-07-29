# UCOF Format Change Proposal Process

## 1. Purpose

A Format Change Proposal (FCP) is the review record for a normative UCOF decision. FCPs prevent serialized behavior from being established accidentally by one implementation, an issue comment, or an undocumented prototype.

An accepted FCP is not automatically the full specification text. Its accepted requirements must be incorporated into the relevant draft specification, registry, profile, vectors, and implementation work.

## 2. When an FCP is required

An FCP is required for changes affecting:

- serialized bytes, framing, alignment, canonicalization, or footer discovery;
- required reader or writer behavior;
- active snapshot selection, recovery, or compaction semantics;
- compatibility or version negotiation;
- required, optional, or advisory capability behavior;
- object, chunk, directory, index, schema, transform, digest, signature, encryption, provenance, or external-reference semantics;
- a core or profile conformance requirement;
- permanent numeric or textual registry identifiers;
- a project or specification license change;
- the stabilization or retirement of a specification version or experimental epoch;
- a security-critical interpretation.

An FCP is usually unnecessary for editorial corrections, internal implementation refactors, build tooling, or tests that do not redefine expected behavior. Those may use an ADR or ordinary pull request.

When uncertain, begin with an issue. Maintainers may require conversion to an FCP before merge.

## 3. Proposal identity and files

Proposals live in `docs/proposals/` and use four-digit numbers:

```text
docs/proposals/0001-short-title.md
```

The proposal number identifies the discussion record and is never reused. Number allocation does not imply acceptance.

The title line should use:

```text
# FCP-0001: Short descriptive title
```

## 4. Status lifecycle

An FCP has one of these statuses:

- **Draft** — incomplete or under author development.
- **Review** — complete enough for the public review period.
- **Accepted** — approved as a project decision.
- **Rejected** — considered and not accepted.
- **Withdrawn** — removed by its author before decision.
- **Deferred** — valid topic postponed pending evidence or dependencies.
- **Superseded** — replaced by a later accepted FCP.

Status changes occur through pull requests or maintainer updates with a recorded reason.

## 5. Required proposal content

Every FCP must include:

1. metadata and status;
2. summary;
3. motivation and problem statement;
4. scope and non-goals;
5. terminology;
6. detailed specification or decision;
7. compatibility impact;
8. security and privacy impact;
9. resource-limit impact;
10. streaming and random-access impact;
11. recovery, truncation, and compaction impact where applicable;
12. alternatives considered;
13. unresolved questions;
14. implementation plan;
15. test vectors, experiments, benchmarks, or other evidence required;
16. registry allocations requested;
17. migration and rollout plan;
18. rejection or rollback strategy;
19. references to primary sources where relevant.

A section may state “not applicable” with a reason. Omitting the analysis is not equivalent.

## 6. Compatibility analysis

The compatibility section must address separately:

- existing valid files read by new readers;
- new files read by old readers;
- unknown required capabilities;
- unknown optional and advisory data preservation;
- profile compatibility;
- canonical identity and signature effects;
- experimental epoch changes;
- stable version changes, if any;
- migration and coexistence.

“Backward compatible” without identifying reader, writer, version, profile, and scope is insufficient.

## 7. Security and privacy analysis

The security section must identify attacker-controlled inputs, trust boundaries, failure behavior, and affected resource limits.

Proposals involving cryptography must define exact algorithm identifiers, parameters, domain separation, covered bytes or canonical values, failure behavior, and migration from deprecated algorithms.

Proposals involving metadata, identifiers, provenance, indexes, external references, deduplication, or encryption must state what information becomes observable.

## 8. Evidence requirements

The author and reviewers decide what evidence is proportionate, but irreversible core decisions normally require more than prose.

Evidence may include:

- executable prototypes;
- annotated byte layouts;
- valid and invalid conformance vectors;
- cross-language implementations;
- truncation simulations;
- fuzzing results;
- benchmarks under representative use cases;
- corpus studies;
- formal or property-based checks;
- comparison with established formats and primary specifications;
- threat-model updates.

A prototype demonstrates feasibility but does not make its behavior normative unless the accepted FCP states that behavior precisely.

## 9. Process

### Step 1 — Discuss the problem

Open an issue or discussion describing the requirement, affected use cases, and major alternatives. Early discussion should focus on whether the problem belongs in the core, a profile, an extension, or an implementation.

### Step 2 — Reserve a proposal number

Create a draft from the FCP template using the next unused number. Number reservation is administrative only.

### Step 3 — Submit the draft

Open a pull request containing the proposal. Include supporting prototypes or vectors in the same pull request or link tracked dependencies.

### Step 4 — Completeness review

A maintainer verifies that required sections are present and that unresolved assumptions are visible. Incomplete proposals remain Draft.

### Step 5 — Enter Review

The proposal status changes to Review and the applicable public review period begins. Default periods are defined in [GOVERNANCE.md](GOVERNANCE.md).

Material revisions restart the review clock.

### Step 6 — Resolve objections

Authors should answer objections through evidence, revision, narrowed scope, or explicit rejection with rationale. Blocking objections must identify a concrete interoperability, security, compatibility, implementation, or requirements problem.

### Step 7 — Decision

Maintainers accept, reject, defer, or request further revision according to governance rules. The decision must summarize the decisive evidence and unresolved dissent.

### Step 8 — Integrate

After acceptance:

- merge the proposal record;
- update the draft specification;
- allocate approved permanent identifiers;
- add required vectors and tests;
- update glossary, threat model, use cases, and versioning where affected;
- track implementation work separately.

An accepted FCP whose normative text has not been integrated must be marked clearly in project status.

## 10. Experimental work

Experiments may use designated experimental identifiers and wire epochs without permanent allocation. They must be labeled non-stable and collision-prone outside their declared scope.

An experiment must not claim permanent interoperability merely because multiple local components share the same provisional identifier.

## 11. Registry allocation

Permanent identifiers are allocated only after the defining FCP is accepted. The registry pull request should reference the accepted FCP and must not broaden the approved semantics.

See [REGISTRY_POLICY.md](REGISTRY_POLICY.md).

## 12. Amendments, errata, and supersession

A substantive change to an accepted FCP requires a new FCP that amends or supersedes it. Editorial corrections may update the original file while preserving a visible change history.

A superseding proposal must identify:

- which requirements are replaced;
- treatment of existing files and identifiers;
- migration behavior;
- whether the old behavior remains readable or becomes invalid;
- security implications of coexistence.

## 13. Rejection and withdrawal

Rejected and withdrawn proposals remain in the repository because their rationale prevents repeated debate and documents alternatives.

A rejected proposal may be reconsidered when new evidence, requirements, or implementation experience materially changes the analysis. Reconsideration uses a new proposal number and links the earlier record.

## 14. Emergency security process

Security-sensitive proposals may be developed privately during coordinated disclosure. A maintainer may merge a temporary implementation restriction or specification correction before the normal review period when delay creates material risk.

After disclosure, the project must publish enough rationale, compatibility impact, and permanent resolution to restore the public record.

## 15. Acceptance bar before UCOF 1.0

No core framing, canonicalization, digest scope, active-root selection, or compatibility rule should be frozen for 1.0 unless:

- the bytes and errors are unambiguous;
- valid and invalid vectors exist;
- hostile-input limits are defined;
- relevant Phase 0 use cases are evaluated;
- the threat model is updated;
- at least one alternative is documented and rejected with evidence;
- an independent implementation can reproduce the interpretation before final stabilization.
