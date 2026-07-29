# ADR-0001: Use Rust for the initial reference implementation

- **Status:** Accepted
- **Date:** 2026-07-29
- **Owners:** UCOF maintainers
- **Related issues:** None yet
- **Related FCPs:** None
- **Supersedes:** None
- **Superseded by:** None

## Context

UCOF needs an executable reference implementation to test framing, bounded parsing, deterministic writing, recovery, conformance vectors, fuzzing, and benchmarks.

The implementation will process hostile binary input and must make offset arithmetic, resource accounting, ownership, and error boundaries visible. The project also needs a language with mature tooling for fuzzing, property testing, command-line applications, and cross-platform library distribution.

This is an implementation-local choice. Rust behavior is not normative, Rust types must not leak into the specification, and independent implementations in other languages remain required before UCOF Core 1.0.

## Decision drivers

- memory safety for untrusted binary parsing;
- checked integer and slice handling;
- explicit ownership and bounded allocation design;
- zero-copy and streaming APIs where justified;
- mature unit, property, fuzz, benchmark, and documentation tooling;
- predictable native performance;
- practical library and CLI distribution;
- ability to expose a small language-neutral specification rather than depend on runtime reflection.

## Options considered

### Option A — Rust

**Advantages**

- strong memory-safety model without a garbage collector;
- expressive enums and result types for structured parser errors;
- mature package, formatting, linting, testing, documentation, and fuzzing ecosystem;
- good control over allocations, slices, byte order, and I/O;
- suitable for both a reusable library and a command-line tool.

**Disadvantages**

- steeper contributor learning curve than Python or Go;
- compile times and dependency selection require discipline;
- unsafe code and complex lifetimes can still introduce risk if used carelessly;
- a Rust-first design can accidentally become difficult to reproduce in simpler languages.

### Option B — C or C++

**Advantages**

- broad platform reach and mature binary-format ecosystem;
- direct control over memory and I/O;
- easy integration into many existing systems.

**Disadvantages**

- substantially higher memory-safety risk for hostile parsers;
- more implementation effort for safe ownership, checked arithmetic, and error handling;
- reference behavior could depend on undefined or implementation-defined language behavior unless carefully constrained.

### Option C — Go

**Advantages**

- simple language and toolchain;
- good concurrency, I/O, testing, and cross-compilation;
- easier contributor onboarding.

**Disadvantages**

- less direct control over allocation and memory layout;
- garbage-collector behavior can complicate strict memory-budget benchmarking;
- some zero-copy and ownership constraints are less explicit.

### Option D — Python

**Advantages**

- fastest language for experiments and readable executable specifications;
- strong ecosystem for fixture generation and analysis;
- accessible to many contributors.

**Disadvantages**

- not representative of bounded native parser performance;
- integer and memory behavior can hide constraints faced by C, Rust, Java, or JavaScript implementations;
- unsuitable as the only evidence for a hostile-input production parser.

## Decision

Use Rust for the initial reference library and command-line implementation.

The workspace will be created incrementally during Phase 1 and later phases. It will begin with only the crates required by the current experiment rather than the complete proposed final hierarchy.

The implementation must:

- use checked arithmetic for all untrusted offsets, lengths, counts, and alignments;
- avoid `unsafe` code in core parsing unless separately justified by an ADR and measured need;
- expose explicit reader limits rather than hidden global limits;
- keep strict validation separate from diagnostic or salvage behavior;
- generate language-neutral conformance vectors and annotated layouts;
- avoid serializing Rust-specific enum layouts, struct representations, or platform-dependent values;
- document behavior precisely enough for independent implementations.

## Consequences

### Positive

- the first hostile-input parser starts from a memory-safe default;
- structured error and limit models can be represented explicitly;
- fuzzing and property tests can be integrated early;
- native performance is suitable for streaming and random-access experiments;
- one language can support the library, CLI, fixtures, and benchmarks.

### Negative

- contributors unfamiliar with Rust face additional onboarding cost;
- care is required to keep lifetime and zero-copy optimizations from complicating the public API prematurely;
- Rust’s implementation convenience must not dictate wire semantics that are awkward in other languages;
- an independent implementation is still required, so Rust does not reduce the need for cross-language work.

### Neutral or operational

- Python or other languages may still be used for corpus generation, independent prototypes, and test orchestration;
- C-compatible bindings may be considered later but are not a Phase 1 requirement;
- dependency count and minimum supported Rust version will be decided when the workspace is created.

## Security and reliability impact

Rust reduces common memory-corruption risks but does not prevent logical vulnerabilities, denial of service, parser differentials, cryptographic misuse, unbounded allocation, or unsafe filesystem behavior.

Security review must continue to focus on checked arithmetic, limits, canonicalization, active-root selection, transform expansion, graph traversal, diagnostics, and trust boundaries.

## Validation

This decision will be validated by:

- implementing the Phase 1 in-memory writer and sequential/random-access readers;
- fuzzing every framing entry point;
- measuring peak allocations and reads against Phase 0 use cases;
- generating vectors consumable by a non-Rust parser;
- documenting any Rust-specific assumption found during independent implementation.

## Revisit conditions

Reconsider this ADR if:

- the Rust toolchain prevents required target support;
- implementation complexity materially blocks independent specification work;
- another language demonstrates substantially safer or simpler conformance evidence;
- the project changes from a reference implementation to a multi-language implementation strategy;
- a future standards organization requires a different normative tooling approach.

## Notes

Selecting Rust does not make the reference implementation normative. Released specifications and accepted FCPs remain authoritative according to the governance document.
