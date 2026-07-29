# ADR-0008: Use Rust 1.85 as the provisional minimum supported version

- **Status:** Accepted
- **Date:** 2026-07-30
- **Owners:** UCOF maintainers
- **Related FCPs:** None
- **Supersedes:** None
- **Superseded by:** None

## Context

The reference implementation previously followed the latest stable Rust compiler without a declared floor. That allows accidental use of new language or library features and makes downstream packaging failures appear late.

Phase 2 requires a continuously checked minimum supported Rust version once one is selected. This is an implementation compatibility decision only; it has no effect on UCOF wire compatibility or independent implementations.

## Decision

The workspace declares `rust-version = "1.85"` and CI checks all workspace targets with Rust 1.85.0.

The fuzz package remains nightly-only because `cargo-fuzz` and libFuzzer instrumentation are separate development tooling. It does not set the library MSRV.

The MSRV may be raised before UCOF 1.0 through an ADR update. A release that raises the MSRV must state the change explicitly.

## Decision drivers

- keep the compiler floor visible in package metadata;
- catch accidental use of newer APIs;
- support contemporary distribution toolchains without committing to an excessively old compiler during early development;
- separate language-toolchain compatibility from wire-format compatibility.

## Consequences

### Positive

- downstream builders receive machine-readable compiler requirements;
- CI prevents silent MSRV drift;
- contributors can reproduce the compatibility floor;
- future MSRV changes have an auditable decision record.

### Negative

- dependency updates must remain compatible with Rust 1.85;
- maintaining another CI job increases build time;
- the provisional floor may still change before stable releases.

## Security implications

An older compiler floor must not prevent security dependency updates. If a critical dependency fix requires a newer compiler, maintainers may raise the MSRV with an expedited ADR and release note rather than retain a vulnerable dependency.

## Follow-up work

- add `--locked` CI once the updated dependency lockfile is committed;
- document MSRV in the release checklist;
- reassess the floor before the first public crate release.
