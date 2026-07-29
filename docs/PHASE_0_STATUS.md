# Phase 0 Status — Foundations and Governance

**Status:** In progress  
**Started:** 2026-07-29  
**Phase owner:** Project maintainers

## Objective

Turn UCOF from a design hypothesis into a reviewable engineering project before committing to a byte layout.

## Deliverable status

| Deliverable | Status | Evidence |
|---|---|---|
| Open-source license | Complete | [`LICENSE`](../LICENSE) — MIT |
| Contribution policy | Complete | [`CONTRIBUTING.md`](../CONTRIBUTING.md) |
| Code of Conduct | Complete | [`CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md) |
| Security policy | Complete | [`SECURITY.md`](../SECURITY.md) |
| Communication and governance channels | Complete | [`GOVERNANCE.md`](GOVERNANCE.md) |
| Software/specification versioning | Complete | [`VERSIONING.md`](VERSIONING.md) |
| Experimental epoch and retirement policy | Complete | [`VERSIONING.md`](VERSIONING.md) |
| Shared terminology | Draft published | [`GLOSSARY.md`](GLOSSARY.md) |
| Ten representative use cases | Draft corpus published | [`USE_CASES.md`](USE_CASES.md) |
| Initial threat model | Draft published | [`THREAT_MODEL.md`](THREAT_MODEL.md) |
| ADR process and template | Active | [`decisions/`](decisions/), including accepted [`ADR-0001`](decisions/0001-rust-reference-implementation.md) |
| FCP process and template | Complete; not yet exercised | [`PROPOSAL_PROCESS.md`](PROPOSAL_PROCESS.md), [`proposals/`](proposals/) |
| Registry allocation policy | Complete | [`REGISTRY_POLICY.md`](REGISTRY_POLICY.md) |
| Open decisions register | Complete | [`OPEN_DECISIONS.md`](OPEN_DECISIONS.md) |
| Repository issue/PR templates | Complete | [`.github/`](../.github/) |
| External review of use cases | Pending | Track review feedback in issues |
| External review of threat model | Pending | Track review feedback in issues |
| Maintainer acceptance of templates | Provisional | ADR template has been exercised; revise all templates from real use |
| Agreement on 1.0 proof bar | Draft complete | Governance, versioning, and implementation plan require independent implementation and conformance evidence |

## Exit criteria assessment

Phase 0 is **not yet complete**. Repository foundations are implemented, but the following review gates remain:

1. At least ten representative use cases must receive substantive review rather than merely exist.
2. The threat model must be reviewed against the first framing proposal and hostile-input experiments.
3. The FCP template and review process should be exercised by the first real normative framing decision.
4. Open licensing, patent, and governance questions relevant to multi-organization standardization must be revisited before 1.0.
5. Maintainers must explicitly approve Phase 0 exit in a pull request or accepted project decision.

## Recommended review issues

Open focused issues for:

- `Review UC-01 through UC-05`
- `Review UC-06 through UC-10`
- `Review initial threat model`
- `Review ADR-0001 Rust reference implementation`
- `Select Phase 1 canonical metadata candidates`
- `Define Phase 1 framing experiment matrix`
- `Draft FCP for UCOF-EXP-0001 framing`

## Phase 1 entry conditions

Experimental Phase 1 research may begin before formal Phase 0 closure, but no byte choice should be treated as accepted until:

- the proposal process is used;
- the affected use cases and threats are cited;
- alternatives and rejection criteria are documented;
- experimental epoch handling is included;
- the prototype is explicitly disposable.

## Change log

### 2026-07-29

- Preserved the repository’s existing MIT license.
- Added contribution, conduct, and security policies.
- Published governance, versioning, glossary, use cases, and threat model.
- Established ADR, FCP, and registry processes.
- Recorded unresolved design decisions explicitly.
- Accepted ADR-0001 selecting Rust for the non-normative reference implementation.
