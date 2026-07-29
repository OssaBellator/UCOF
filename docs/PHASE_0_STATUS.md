# Phase 0 Status — Foundations and Governance

**Status:** Review gates remain  
**Started:** 2026-07-29  
**Phase owner:** Project maintainers

## Objective

Turn UCOF from a design hypothesis into a reviewable engineering project before committing to a stable byte layout.

## Deliverable status

| Deliverable | Status | Evidence |
|---|---|---|
| Open-source license | Complete | [`LICENSE`](../LICENSE) — MIT |
| Contribution policy | Complete | [`CONTRIBUTING.md`](../CONTRIBUTING.md) |
| Code of Conduct | Complete | [`CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md) |
| Security policy | Complete | [`SECURITY.md`](../SECURITY.md) |
| Communication and governance channels | Complete | [`GOVERNANCE.md`](GOVERNANCE.md) |
| Software/specification versioning | Complete | [`VERSIONING.md`](VERSIONING.md) |
| Experimental epoch and retirement policy | Active | `UCOF-EXP-0001` exercises the policy |
| Shared terminology | Draft published | [`GLOSSARY.md`](GLOSSARY.md) |
| Ten representative use cases | Draft corpus published | [`USE_CASES.md`](USE_CASES.md) |
| Initial threat model | Draft published | [`THREAT_MODEL.md`](THREAT_MODEL.md) |
| ADR process and template | Active | Accepted [`ADR-0001`](decisions/0001-rust-reference-implementation.md) and [`ADR-0002`](decisions/0002-rust-workspace-and-toolchain.md) |
| FCP process and template | Active | [`FCP-0001`](proposals/0001-exp-0001-framing.md) is in review |
| Registry allocation policy | Complete | [`REGISTRY_POLICY.md`](REGISTRY_POLICY.md) |
| Open decisions register | Complete | [`OPEN_DECISIONS.md`](OPEN_DECISIONS.md) |
| Repository issue/PR templates | Complete | [`.github/`](../.github/) |
| External review of use cases | Pending | Track review feedback in issues |
| External review of threat model | Pending | Review against FCP-0001 and hostile-input results |
| Agreement on 1.0 proof bar | Draft complete | Independent implementation and conformance evidence required |

## Exit criteria assessment

Phase 0 foundations are implemented and are now being exercised by Phase 1. Formal Phase 0 closure still requires:

1. substantive review of the representative use cases;
2. review of the threat model against executable framing experiments;
3. completion of the first FCP review cycle;
4. reconsideration of licensing, patents, and governance before multi-organization standardization;
5. explicit maintainer approval of Phase 0 exit.

## Phase 1 overlap

Phase 1 may proceed because its work is explicitly disposable, cites affected use cases and threats, uses the FCP process, and does not claim that implementation choices are stable.

No byte choice in `UCOF-EXP-0001` is accepted for Core 1.0 merely because the prototype emits or parses it.

## Change log

### 2026-07-29

- Preserved the repository’s MIT license.
- Added contribution, conduct, security, governance, versioning, glossary, use cases, and threat model.
- Established ADR, FCP, and registry processes.
- Accepted ADR-0001 selecting Rust for the non-normative reference implementation.
- Accepted ADR-0002 defining the minimal Phase 1 workspace.
- Entered FCP-0001 into review with an executable disposable experiment.
