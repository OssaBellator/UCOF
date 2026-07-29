# UCOF Governance

## 1. Purpose

This document defines how UCOF makes decisions, assigns responsibility, and evolves from a founder-led research project into a multi-maintainer interoperability project.

The governance model must protect two things at once:

- the ability to make progress while the contributor base is small; and
- the requirement that durable format decisions remain reviewable, documented, and implementable independently.

## 2. Current model

UCOF currently uses a **maintainer-led, consensus-seeking model**.

The repository owner is the initial lead maintainer. The lead maintainer may merge routine changes, appoint additional maintainers, moderate project spaces, and accept or reject proposals according to the processes below.

Maintainer authority does not make undocumented behavior normative. The specification, accepted Format Change Proposals, registries, and conformance material define the format—not a private conversation or an implementation accident.

## 3. Roles

### 3.1 Contributor

Anyone who submits an issue, review, document, test, proposal, implementation, or other project material.

### 3.2 Reviewer

A contributor with demonstrated subject-matter knowledge who provides substantive review. Reviewer status is informal and does not grant merge or governance authority.

### 3.3 Maintainer

A trusted contributor who may triage issues, review changes, merge pull requests, enforce repository policy, and vote on proposals. Maintainers are expected to:

- prioritize interoperability and security over convenience;
- disclose material conflicts of interest;
- distinguish personal preference from format requirements;
- document reasons for consequential decisions;
- recuse themselves where impartial review is not credible;
- avoid making their own implementation the sole source of truth.

### 3.4 Lead maintainer

The lead maintainer coordinates releases, resolves deadlocks, appoints or removes maintainers, and has final responsibility for repository administration during the founder-led stage.

The lead maintainer may not silently override an accepted normative proposal. Reversal requires a new proposal or a documented emergency correction followed by retrospective review.

## 4. Decision classes

### 4.1 Routine changes

Examples include typo fixes, editorial clarification without semantic effect, repository maintenance, tests for already-specified behavior, and implementation refactors.

Routine changes may be accepted through ordinary pull-request review.

### 4.2 Architecture Decision Records

ADRs record implementation-local choices such as crate boundaries, test tooling, internal APIs, or build infrastructure. An ADR must not define wire bytes or mandatory interoperable behavior.

See [decisions/README.md](decisions/README.md).

### 4.3 Format Change Proposals

FCPs are required for normative changes affecting serialized representation, canonicalization, required behavior, compatibility, profiles, registry semantics, security-critical interpretation, or permanent identifiers.

See [PROPOSAL_PROCESS.md](PROPOSAL_PROCESS.md).

### 4.4 Emergency security corrections

A maintainer may temporarily restrict, revert, or patch behavior without the normal public review period when disclosure would create material risk. The project must publish an advisory and retrospective proposal or decision record once coordinated disclosure permits it.

An emergency correction must not be used to bypass normal review for convenience.

## 5. Consensus and objections

The project seeks **rough consensus backed by evidence**, not unanimity.

A blocking objection must identify at least one of:

- an interoperability failure;
- a security or privacy regression;
- an unaddressed compatibility break;
- an implementation feasibility problem;
- a contradiction with an accepted requirement;
- insufficient evidence for an irreversible choice.

A preference without a concrete technical consequence is not a blocking objection.

Maintainers should attempt to resolve objections through experiments, revised scope, explicit profile constraints, or documented alternatives. Unresolved objections must be preserved in the proposal record.

## 6. Proposal approval

During the single-maintainer stage, an FCP is accepted when:

1. the required review period has elapsed;
2. the proposal is complete;
3. material objections have been resolved or explicitly dispositioned;
4. the lead maintainer approves it; and
5. any required prototype, test vector, benchmark, or threat-model update is present.

With two or more active maintainers, acceptance additionally requires approval from at least two maintainers and no unresolved blocking objection from another maintainer.

A maintainer who is the principal author may approve their own proposal only during the single-maintainer stage. Such proposals should seek external review before freezing stable bytes.

## 7. Review periods

Default minimum public review periods are:

- 3 calendar days for a narrowly scoped experimental proposal;
- 7 calendar days for registry, profile, or compatibility-preserving normative changes;
- 14 calendar days for core framing, canonicalization, cryptographic scope, compatibility breaks, or stabilization decisions.

The review clock restarts when a revision materially changes the proposal’s behavior or risk.

## 8. Maintainer selection and removal

A contributor may be invited as a maintainer after sustained, constructive work demonstrating technical judgment, review quality, reliability, and alignment with project policy.

The lead maintainer records appointments publicly. As the project grows, maintainers should review appointments by consensus.

A maintainer may be removed for prolonged inactivity, repeated policy violations, abuse of authority, undisclosed conflicts, or loss of trust. Removal should be documented to the extent compatible with privacy and safety.

## 9. Conflicts of interest

Contributors and maintainers must disclose material interests such as:

- employment by a vendor whose product depends on the decision;
- ownership of relevant patents or patent applications;
- responsibility for a competing format or implementation;
- financial benefit from a particular codec, service, or dependency.

Disclosure does not automatically disqualify participation. It allows reviewers to evaluate incentives and may require recusal from final approval.

## 10. Normative authority

The order of authority is:

1. a released stable specification and its errata;
2. accepted normative FCPs incorporated into the active draft;
3. normative registries and profile specifications;
4. conformance vectors where explicitly referenced by the specification;
5. reference implementation behavior only as non-normative evidence.

README examples, issue comments, prototype code, and historical files are not normative unless incorporated by one of the authorities above.

## 11. Independent implementation requirement

A single implementation is insufficient evidence for UCOF 1.0 stability.

Before the core specification can reach 1.0, at least one implementation developed independently from the reference implementation must parse the required core, pass agreed conformance vectors, and demonstrate compatible interpretation of at least the mandatory features.

The independent implementation may be partial, but it must be sufficient to expose specification ambiguity rather than merely wrap the reference library.

## 12. Project and specification licensing

The repository’s existing [MIT License](../LICENSE) applies to source code, documentation, draft specifications, fixtures, and other repository material unless a file states otherwise.

Changing the project or specification license requires an FCP, legal compatibility analysis, and explicit maintainer approval. Contributions already received remain under the terms under which they were submitted.

## 13. Communication and records

GitHub issues, discussions, pull requests, FCPs, ADRs, and security advisories are the supported project channels.

Consequential decisions made during calls or private discussion must be summarized in a public issue, ADR, or proposal before they are treated as settled.

## 14. Governance evolution

This model should be reviewed when any of the following occurs:

- three or more regular contributors are active;
- a second interoperable implementation exists;
- an organization depends on UCOF in production;
- the project begins a 1.0 stabilization process;
- control of the repository or trademark changes.

Potential future changes include a maintainer council, formal voting rules, a technical steering committee, a neutral foundation, and a more explicit patent policy. These are not required before the contributor base justifies their overhead.
