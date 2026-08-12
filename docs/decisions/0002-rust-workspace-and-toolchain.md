# ADR-0002: Use a minimal Rust workspace for Phase 1

- **Status:** Accepted
- **Date:** 2026-07-29
- **Owners:** UCOF maintainers
- **Related ADRs:** ADR-0001
- **Related FCPs:** FCP-0001
- **Supersedes:** None
- **Superseded by:** None

## Context

Phase 1 needs an executable experiment for framing, bounded parsing, canonical metadata, footer discovery, deterministic writing, inspection, and malformed-input handling.

The repository should not create empty crates for future features. It should introduce only the components needed to execute the current experiment while preserving a path to the larger workspace described in the implementation plan.

## Decision

Create a Cargo workspace with two members:

```text
crates/
  ucof-core/   Experimental format types, canonical CBOR subset, reader, writer
  ucof-cli/    Thin inspect, verify, and demo-generation commands
```

The workspace will initially target stable Rust and use the 2021 edition. The minimum supported Rust version is not yet promised; CI uses the current stable toolchain until the public API and dependency policy mature.

The Phase 1 library may use a small external dependency for SHA-256. All framing, checked range handling, canonical metadata validation, limits, and error categorization remain implemented inside the repository so their behavior is visible and testable.

The CLI is intentionally thin. It must call the library rather than duplicate parser behavior.

## Constraints

- Rust types and crate APIs are non-normative.
- The experimental byte layout is defined by FCP-0001 and the experimental specification, not by implementation accidents.
- `unsafe` Rust is prohibited during Phase 1 unless a later ADR documents a demonstrated need and review.
- Reader limits are explicit inputs, not global constants hidden from callers.
- The library must distinguish strict validation from any future salvage behavior.
- No compression, encryption, signatures, external retrieval, plugins, or embedded execution are introduced in Phase 1.
- The workspace must compile without generated source files.

## Alternatives considered

### One crate containing library and CLI

This is simpler initially, but makes it easier for command-line assumptions and process behavior to leak into the reusable parser API.

### Full planned multi-crate workspace

Creating `ucof-index`, `ucof-schema`, `ucof-transform`, `ucof-crypto`, and profile crates now would produce empty architecture rather than validated boundaries. Those crates will be added only when a phase introduces real responsibilities.

### Python-only prototype

Python is useful for independent vector generation, but using it as the sole reference parser would provide weaker evidence about bounded allocation and checked byte-range APIs for a hostile binary format. A small Python vector generator remains useful as an independent construction path.

## Consequences

### Positive

- Small initial implementation surface.
- Clear separation between reusable codec and user interface.
- Easy expansion without pre-committing to future crate boundaries.
- Library behavior can be tested directly.

### Negative

- The crate boundary may change before 1.0.
- Stable Rust CI does not establish a long-term MSRV.
- A SHA-256 dependency adds supply-chain surface that must be reviewed and pinned through `Cargo.lock` for CLI builds.

## Validation

This decision is validated when:

- the workspace builds on stable Rust;
- formatting, linting, and tests run in CI;
- the CLI exercises the library rather than maintaining its own decoder;
- malformed vectors return structured errors without panics;
- a Python generator reproduces the checked-in hexadecimal vectors.

## Revisit triggers

Revisit this ADR when:

- a third component has a stable, independently useful responsibility;
- a second implementation exposes Rust-specific assumptions;
- the project defines an MSRV policy;
- dependency or build constraints make the current SHA-256 implementation unsuitable;
- profile work begins.
