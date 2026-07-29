# UCOF — Universal Chunked Object Format

> A language-neutral, self-describing, chunk-addressable container format for durable storage, partial access, streaming, verification, recovery, and extensible domain profiles.

## Project status

**UCOF is an early design and research project.** No stable specification, wire format, reference implementation, or production-compatible file exists yet.

Current stage: **Phase 0 — Foundations and Governance, in progress**.

The project has published its initial governance, terminology, use-case corpus, threat model, versioning rules, and proposal process. Phase 0 is not considered complete until those materials receive substantive review.

- [Phase 0 status](docs/PHASE_0_STATUS.md)
- [Detailed implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Open design decisions](docs/OPEN_DECISIONS.md)

Do not use experimental UCOF files for durable storage. Pre-stable wire experiments will use explicit `UCOF-EXP-####` epochs and may become unreadable when an epoch is retired.

## Design thesis

UCOF is a **universal container, not a universal representation**.

The project does not attempt to force documents, media, analytical tables, databases, scientific arrays, and archives into one physical layout. It aims to standardize a small durable envelope that lets specialized profiles preserve their efficient representations while sharing common framing, discovery, integrity, compatibility, and safety rules.

A basic conforming reader should eventually be able to:

- identify the core version or experimental epoch;
- discover the active snapshot;
- enumerate objects and required capabilities;
- validate structural and integrity evidence;
- skip and preserve unknown optional data;
- extract opaque stored payloads where safe;
- explain what it cannot interpret;
- operate under explicit hostile-input resource limits.

Profile-aware readers add domain semantics.

## Why another container format?

Existing formats make different trade-offs:

- JSON is portable and inspectable but inefficient for large binary payloads and random access.
- SQLite provides indexed mutation and transactions but is not a general interchange container.
- Parquet and Arrow optimize analytical data access but do not model every object or mutable workload.
- HDF5 provides rich chunked scientific datasets with a large implementation surface.
- ZIP is widely supported but has limited schema, integrity, indexing, and update semantics.
- media containers stream timed data well but specialize around tracks and timestamps.
- PDF is portable and feature-rich but has a complex object, update, and security model.

UCOF explores whether selected strengths can coexist in a smaller layered architecture. It explicitly documents trade-offs rather than claiming to eliminate them.

## Goals

UCOF is intended to provide:

- **Self-description** — schemas, logical types, encodings, and profile declarations can travel with the file.
- **Partial access** — selected objects or ranges can be located without loading unrelated payloads.
- **Streaming** — independently bounded chunks and checkpoints support sequential production and consumption.
- **Determinism** — canonical representations support stable identity, signatures, reproducible output, and deduplication.
- **Extensibility** — unknown optional features remain safely skippable and preservable.
- **Integrity** — precisely scoped digests and roots detect corruption and substitution.
- **Recoverability** — append-only publication preserves an earlier valid snapshot after interrupted writes.
- **Security** — readers assume hostile input and enforce caller-controlled limits.
- **Longevity** — the mandatory core remains implementable without a large runtime or online registry.
- **Multiple access patterns** — profiles can select different layouts and secondary indexes.

## Non-goals

UCOF does not attempt to:

- make every workload equally efficient under one layout;
- eliminate application-specific codecs, schemas, or software;
- embed executable parsers or automatically run code supplied by a file;
- guarantee that every reader understands every object type;
- replace live network protocols, distributed databases, or source control;
- make encryption, mutation, deduplication, compression, simplicity, and random access free of trade-offs;
- standardize one schema language for every domain;
- define one universal trust policy for signatures or provenance.

## Proposed layered architecture

| Layer | Responsibility |
|---|---|
| Bootstrap framing | Magic, version or experimental epoch, bounded discovery fields |
| Canonical metadata | Deterministic structured metadata, with deterministic CBOR as a candidate |
| Object storage | Logical objects represented by independently bounded chunks |
| Primary directory | Object-to-location discovery for random-access files |
| Secondary indexes | Optional profile-specific lookup and filtering accelerators |
| Schema system | Versioned schemas, physical types, logical types, and compatibility metadata |
| Transform pipeline | Compression, encryption, checks, and domain encodings applied in explicit order |
| Snapshot model | Append-only manifests, roots, checkpoints, and previous-root relationships |
| Trust layer | Digests, signatures, authenticated claims, and optional provenance |
| Profiles | Interoperability constraints for concrete domains and access patterns |

### Conceptual file shape

```text
+--------------------------------------------------+
| Small bootstrap header                           |
+--------------------------------------------------+
| Manifest, metadata, object, and payload chunks   |
| ...                                              |
+--------------------------------------------------+
| Primary directory and optional indexes           |
+--------------------------------------------------+
| Integrity, provenance, and signature objects     |
+--------------------------------------------------+
| Root or checkpoint discovery footer              |
+--------------------------------------------------+
```

This is conceptual only. No byte layout, field width, byte order, magic value, footer strategy, digest, or metadata subset has been accepted.

## Core distinctions

### Objects and chunks

An **object** is a logical entity such as a manifest, schema, image, table, index, signature, or document node. A **chunk** is a physical storage unit containing all or part of an object.

Object boundaries and chunk boundaries are not assumed to be identical.

### Directory and indexes

The primary directory enables object discovery. Workload-specific indexes accelerate queries.

Indexes must not silently become sources of truth. A malicious or stale index must not be able to change authoritative meaning without validation.

### Instance and content identity

UCOF distinguishes:

- **instance identity**, which refers to an evolving file or object lineage; and
- **content identity**, which is derived from a defined canonical representation.

The design must account for encrypted, mutable, privacy-sensitive, or non-canonical data that cannot safely expose a stable public content hash.

### Integrity, authenticity, provenance, and confidentiality

These are separate guarantees:

- a digest compares content with an expected identity;
- a signature binds an exact scope to a signing key;
- a provenance claim asserts something about origin or transformation;
- encryption protects selected data from unauthorized readers.

A valid signature does not make a claim true, and encrypted payloads do not imply confidential metadata.

### Capability model

Features will be classified as:

- **Required** — safe interpretation is impossible without support.
- **Optional** — may be skipped while preserving the surrounding scope according to defined rules.
- **Advisory** — affects presentation, optimization, or diagnostics only.

Readers must fail closed for unknown required behavior.

## Initial profiles

The current candidates are:

- **Archive** — hierarchy, metadata, extraction, deduplication, and snapshots.
- **Table** — columns, row groups, dictionaries, statistics, indexes, and schema evolution.
- **Media** — timed tracks, cues, chapters, subtitles, and streaming indexes.
- **Document** — structured content, resources, annotations, accessibility, and optional layout.
- **Scientific** — multidimensional arrays, units, coordinates, and chunk transforms.
- **Database** — logical records, transactional roots, indexes, and compaction.
- **Package** — software artifacts, dependencies, signatures, and installation constraints.

Archive and Table are the provisional first profiles because they exercise materially different access patterns without requiring a full renderer or database engine.

## Security posture

Every input byte is assumed hostile.

The design requires attention to:

- checked offset and length arithmetic;
- forged and overlapping ranges;
- bounded metadata, recursion, allocation, and diagnostics;
- decompression and transform expansion;
- cyclic object, schema, and dictionary relationships;
- malicious indexes and forged statistics;
- parser differential behavior;
- digest and algorithm confusion;
- signature wrapping and ambiguous scope;
- metadata leakage and encryption misuse;
- stale-root and rollback confusion;
- external references and unintended network access;
- safe archive extraction and repair behavior.

See the [initial threat model](docs/THREAT_MODEL.md) and [security policy](SECURITY.md).

## Phase 0 foundation documents

| Document | Purpose |
|---|---|
| [Governance](docs/GOVERNANCE.md) | Roles, authority, consensus, review periods, and independent implementation requirement |
| [Versioning](docs/VERSIONING.md) | Software versions, specification versions, experimental epochs, and retirement |
| [Glossary](docs/GLOSSARY.md) | Shared technical vocabulary and important distinctions |
| [Use-case corpus](docs/USE_CASES.md) | Ten concrete workloads with scale, access, trust, and failure criteria |
| [Threat model](docs/THREAT_MODEL.md) | Adversaries, trust boundaries, threats, controls, and residual risks |
| [Proposal process](docs/PROPOSAL_PROCESS.md) | Normative Format Change Proposal workflow |
| [Registry policy](docs/REGISTRY_POLICY.md) | Permanent, experimental, private-use, and deprecated identifier rules |
| [Open decisions](docs/OPEN_DECISIONS.md) | Explicitly resolved, provisional, blocked, and undecided items |
| [ADR process](docs/decisions/README.md) | Implementation-local decision records |
| [FCP index](docs/proposals/README.md) | Normative proposal records and template |
| [Phase 0 status](docs/PHASE_0_STATUS.md) | Current deliverables and remaining exit gates |

## Decision process

Implementation-local decisions use Architecture Decision Records.

Normative decisions use Format Change Proposals when they affect bytes, canonicalization, required behavior, compatibility, profiles, registries, security-critical interpretation, or permanent identifiers.

Permanent identifiers are not assigned before the defining FCP is accepted. A working prototype is evidence, not the specification.

## Versioning and compatibility

Reference software will use Semantic Versioning after releases begin. The core specification and each profile use separate `MAJOR.MINOR` versions.

Pre-stable incompatible byte layouts use monotonic experimental epochs:

```text
UCOF-EXP-0001
UCOF-EXP-0002
```

An unknown epoch must be reported as unsupported rather than guessed. Experimental epochs never become stable specification version numbers.

## Planned reference implementation

Rust is the provisional language for the reference implementation because hostile-input parsing benefits from memory safety, checked binary handling, fuzzing support, and predictable performance.

The specification must remain practical to implement independently in other languages. UCOF Core 1.0 requires at least one independent parser that does not merely wrap the reference library.

Repository structure will be introduced incrementally as phases require it. Empty crates and directories will not be added simply to resemble a complete implementation.

## Roadmap

| Phase | Outcome |
|---|---|
| 0 | Foundations, governance, terminology, use cases, and threat model |
| 1 | Minimal disposable framing experiment |
| 2 | Bounded safety-first core reader and writer |
| 3 | Directory, snapshots, checkpoints, recovery, and compaction |
| 4 | Transform pipeline and chunked compression |
| 5 | Schemas and lossless diagnostic text projection |
| 6 | Integrity, signatures, and provenance |
| 7 | Encryption and selective disclosure |
| 8 | Archive and Table profiles |
| 9 | Independent implementation, conformance, benchmarks, and hardening |
| 10 | Core 1.0 specification freeze and adoption package |

Detailed deliverables and exit criteria are in the [implementation plan](docs/IMPLEMENTATION_PLAN.md).

## Contributing

Review, counterexamples, hostile test ideas, workload traces, format comparisons, and implementation experiments are welcome.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening substantial work. Use the issue forms for bugs, design questions, and proposal intake. Do not disclose unpatched vulnerabilities publicly.

## License

UCOF repository material is available under the [MIT License](LICENSE) unless a file states otherwise.

“UCOF,” the `.ucof` extension, media types, magic bytes, registry namespaces, and profile identifiers remain provisional until their technical and naming reviews are complete.
