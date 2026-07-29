# ADR-0009: Isolate Phase 3 algorithms in a non-normative research crate

- **Status:** Accepted
- **Date:** 2026-07-30
- **Owners:** UCOF maintainers
- **Related FCPs:** FCP-0002
- **Supersedes:** None
- **Superseded by:** None

## Context

Phase 3 must evaluate paged directories, snapshot chains, recovery selection, checkpoints, and compaction before choosing EXP-0002 bytes. Implementing those algorithms directly inside `ucof-core` would make Rust public types and internal page structures appear more settled than the proposal allows.

The project also needs executable counterexamples now: directory cycles, forged child ranges, candidate forks, missing parents, interrupted appends, progress checkpoints, graph cycles, orphans, and resource exhaustion. These properties can be tested without a serialized layout.

## Decision

Create a workspace crate named `ucof-experiments`.

The crate:

- is private and not published;
- contains no normative wire parser or writer;
- may use convenient in-memory representations that are unsuitable for production;
- exposes research models only so tests and benchmarks can compare alternatives;
- documents that successful algorithms do not select field widths, page sizes, encodings, identities, or footer layouts;
- remains covered by formatting, lint, tests, MSRV, and portability checks.

The initial modules are:

- `directory` — canonical ordered-page construction, validation, bounded lookup, and closed-form scale estimates;
- `snapshots` — exact-end and recovery selection over already classified candidates, including parent chains, forks, progress checkpoints, and limits;
- `compaction` — bounded iterative reachability planning, orphan reporting, cycle handling, and missing-dependency rejection.

An algorithm can move into `ucof-core` only after its wire-facing invariants are defined by an accepted or reviewed FCP and the implementation no longer leaks unresolved research choices.

## Consequences

### Positive

- algorithmic evidence can accumulate without stabilizing bytes;
- multiple directory strategies can coexist for benchmarks;
- tests can mutate private model state to exercise corruption cases;
- EXP-0001 core APIs remain focused and reviewable;
- research failures can be deleted or replaced without compatibility claims.

### Negative

- some logic may later be reimplemented after the wire layout is chosen;
- the extra crate increases workspace and CI surface;
- public Rust visibility inside the private crate may still be mistaken for a promise unless documentation remains explicit.

## Security implications

- research models must still use checked arithmetic and caller-controlled limits;
- candidate classification is assumed input to the snapshot model; a future parser must establish those classifications independently;
- in-memory page identifiers are not physical offsets and must never be reused as wire locators;
- compaction reachability does not authenticate dependencies and cannot make unverified input valid;
- tests must include malicious cycles, overlaps, forks, missing dependencies, and limit exhaustion.

## Follow-up work

- add alternative sorted-array and hash-page directory models;
- benchmark page sizes and fanout;
- implement EXP-0002 bytes only after FCP-0002 resolves its open questions;
- move stable algorithms into the core behind explicit experimental epoch support;
- remove research-only APIs before any public crate release.
