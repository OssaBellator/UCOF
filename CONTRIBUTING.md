# Contributing to UCOF

Thank you for helping develop the Universal Chunked Object Format.

UCOF is still in its design and research stage. Contributions that clarify requirements, expose trade-offs, improve the threat model, add representative workloads, or challenge assumptions are as valuable as code.

## Before contributing

Please read:

- [README.md](README.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Governance](docs/GOVERNANCE.md)
- [Glossary](docs/GLOSSARY.md)
- [Threat model](docs/THREAT_MODEL.md)
- [Proposal process](docs/PROPOSAL_PROCESS.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)

## Communication channels

Use GitHub according to the kind of work:

- **Issues** — defects, requirements, use cases, questions, and scoped implementation work.
- **Discussions** — exploratory design conversation when enabled.
- **Pull requests** — concrete changes with reviewable rationale and tests where applicable.
- **Format Change Proposals (FCPs)** — normative wire-format, compatibility, profile, or registry decisions.
- **Architecture Decision Records (ADRs)** — implementation-local decisions that do not define the format.
- **Private vulnerability reports** — security issues that should not be disclosed publicly.

Normative decisions must not be made only in chat, private messages, or commit history. The accepted rationale must be recorded in the repository.

## Types of contribution

### Documentation and research

Useful contributions include:

- correcting terminology;
- refining a use case with measurable constraints;
- identifying a security or privacy risk;
- comparing alternative framing, indexing, canonicalization, or recovery designs;
- adding citations to primary technical sources;
- identifying interoperability traps in existing formats.

### Implementation work

Implementation work should correspond to the current project phase. Avoid adding speculative crates, codecs, profiles, or abstractions before their requirements and format implications are accepted.

During Phase 0, implementation work is limited primarily to repository policy, research documents, templates, and small tooling that validates those documents.

### Normative format changes

A change requires an FCP when it affects any of the following:

- serialized bytes or canonicalization;
- required reader or writer behavior;
- compatibility or version negotiation;
- object, capability, transform, digest, schema, profile, or registry semantics;
- security-critical interpretation;
- permanent numeric or textual identifiers;
- conformance requirements.

See [docs/PROPOSAL_PROCESS.md](docs/PROPOSAL_PROCESS.md).

## Contribution workflow

1. Search existing issues, proposals, and decisions.
2. Open or reference an issue for non-trivial work.
3. Keep each pull request focused on one coherent change.
4. Explain the problem, alternatives considered, compatibility impact, and security impact.
5. Add or update tests and fixtures when behavior changes.
6. Update the glossary, threat model, use cases, or specification when terminology or guarantees change.
7. Resolve review findings or record unresolved objections explicitly.

Small typo and link fixes do not require a prior issue.

## Pull request expectations

A pull request should include:

- a clear title and summary;
- the motivating problem;
- the chosen approach;
- alternatives or counterarguments for significant design work;
- compatibility and migration impact;
- security and privacy impact;
- tests or a reason tests are not applicable;
- documentation updates.

A working prototype is evidence, not by itself a specification decision.

## Review principles

Reviews should prioritize:

1. safety on hostile input;
2. unambiguous interoperability;
3. recoverability and deterministic behavior;
4. a small mandatory core;
5. measurable performance rather than assumed performance;
6. preservation of unknown optional data;
7. implementation feasibility in more than one language.

Maintainers may reject technically functional changes that prematurely freeze the wire format, expand the mandatory core without evidence, or hide unresolved trade-offs.

## Commit style

Use concise imperative commit subjects. Conventional prefixes are encouraged:

- `docs:` documentation and research;
- `feat:` new behavior;
- `fix:` defect correction;
- `test:` tests and fixtures;
- `refactor:` internal restructuring;
- `chore:` repository maintenance;
- `fcp:` format proposal work;
- `adr:` implementation decision records.

## Licensing and contributor representation

The repository is licensed under the terms in [LICENSE](LICENSE). Unless explicitly stated otherwise, intentionally submitted contributions are provided under the same terms.

By submitting a contribution, you represent that you have the right to provide it and that it does not knowingly include material under incompatible terms. Clearly identify third-party material and its license.

No Contributor License Agreement is required at this stage. This may be revisited through the documented governance process if the project’s legal or standards needs change.

## Generated and AI-assisted contributions

Contributors remain responsible for all submitted material, including generated or AI-assisted text and code. Review it for correctness, licensing risk, fabricated citations, security defects, and accidental disclosure before submission.

## Reporting security issues

Do not open a public issue for an undisclosed vulnerability. Follow [SECURITY.md](SECURITY.md).
