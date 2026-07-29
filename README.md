# UCOF — Universal Chunked Object Format

> A language-neutral, self-describing, chunk-addressable container format for durable storage, partial access, streaming, verification, recovery, and extensible domain profiles.

## Project status

**UCOF is an early design and research project.** No stable specification, production wire format, or production-compatible file exists yet.

Current implementation stage: **Phase 2 — Safety-First Core Codec, technical implementation under review**.

Phase 1's disposable framing proposal remains in public review. Phase 2 exercises those bytes through bounded hostile-input APIs without making them stable.

- [Phase 2 status](docs/PHASE_2_STATUS.md)
- [Phase 2 bounded API guide](docs/PHASE_2_API_GUIDE.md)
- [Phase 1 status](docs/PHASE_1_STATUS.md)
- [UCOF-EXP-0001 specification](spec/experimental/UCOF-EXP-0001.md)
- [FCP-0001 framing proposal](docs/proposals/0001-exp-0001-framing.md)
- [Detailed implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Phase 0 status](docs/PHASE_0_STATUS.md)

Do not use experimental UCOF files for durable storage. `UCOF-EXP-####` epochs are disposable and may be retired without migration support.

## Design thesis

UCOF is a **universal container, not a universal representation**.

It does not force documents, media, analytical tables, databases, scientific arrays, and archives into one physical layout. It aims to standardize a small durable envelope while allowing specialized profiles to preserve efficient domain-specific representations.

A future conforming core reader should be able to identify a version, discover the active snapshot, enumerate objects and required capabilities, validate structure and integrity, preserve unknown optional data, extract safe opaque payloads, and explain unsupported semantics under explicit resource limits.

## Current experimental core

`UCOF-EXP-0001` is the first executable framing experiment. It includes:

- a 32-byte fixed bootstrap header;
- contiguous 40-byte record headers;
- opaque object records;
- a canonical metadata manifest;
- a canonical metadata primary directory;
- an exact-end 80-byte footer;
- SHA-256 over all bytes before the footer;
- strict structural and directory cross-validation;
- caller-controlled hostile-input limits;
- deterministic writing and object lookup;
- independently generated valid and invalid vectors.

Phase 2 adds implementation-local safety APIs over those bytes:

- metadata-only inspection over borrowed, seekable, or future range-backed sources;
- strict source validation with bounded block hashing;
- non-seeking sequential event reading with bounded payload chunks;
- strict diagnostics that never convert integrity failure into validity;
- explicitly unverified prefix salvage;
- deterministic streaming and seekable writers with explicit finalization;
- separate accounting for reads, hashes, allocations, logical bytes, and diagnostics;
- property tests, sparse virtual-source tests, cross-language corpus checks, and continuous fuzz targets.

The epoch deliberately excludes compression, encryption, signatures, provenance, external references, append history, schemas, and profiles.

No field width, magic value, byte order, metadata choice, footer rule, identifier width, or digest in this experiment is stable merely because the implementation works.

## Quick start

The reference implementation has a provisional Rust 1.85 minimum supported version.

```console
cargo test --locked --workspace
cargo run --locked -p ucof-cli --bin ucof -- make-demo demo.ucof
cargo run --locked -p ucof-cli --bin ucof -- inspect demo.ucof
cargo run --locked -p ucof-cli --bin ucof -- verify demo.ucof
cargo run --locked -p ucof-cli --bin ucof -- diagnose demo.ucof
cargo run --locked -p ucof-cli --bin ucof -- salvage demo.ucof
```

The command assurance levels are deliberately different:

| Command | Meaning |
|---|---|
| `inspect` | Structural metadata inventory; payload integrity is not checked |
| `verify` | Strict bounded committed-prefix integrity validation |
| `diagnose` | Strict verified-or-invalid status with categorized failures |
| `salvage` | Unverified complete-prefix record discovery; never a conformance claim |
| `make-demo` | Deterministic experimental output through explicit finalization |

Generate the independent vector corpus with:

```console
python3 tools/generate_exp_0001_vectors.py
```

Generated `.ucof` binaries are ignored by Git. Canonical hexadecimal fixtures and expected outcomes are stored in `tests/vectors/exp-0001/`.

## Current repository structure

```text
crates/
  ucof-core/        Bounded experimental readers, validators, diagnostics, and writers
  ucof-cli/         inspect, verify, diagnose, salvage, and make-demo commands
fuzz/                Standalone cargo-fuzz package and parser/writer targets
spec/
  experimental/     Disposable wire specifications
docs/
  decisions/        Implementation-local Architecture Decision Records
  proposals/        Normative Format Change Proposals
  security/         Executable security findings and supporting evidence
tests/
  vectors/          Reproducible valid and invalid byte fixtures
tools/               Independent generators and experiment tooling
```

Future crates and profile directories will be added only when their responsibilities are validated.

## Why another container format?

Existing formats make different trade-offs:

- JSON is portable and inspectable but inefficient for large binary payloads and random access.
- SQLite provides indexed mutation and transactions but is not a general interchange container.
- Parquet and Arrow optimize analytical data access but do not model every object or mutable workload.
- HDF5 provides rich chunked scientific datasets with a large implementation surface.
- ZIP is widely supported but has limited schema, integrity, indexing, and update semantics.
- media containers stream timed data well but specialize around tracks and timestamps.
- PDF is portable and feature-rich but has a complex object, update, and security model.

UCOF explores whether selected strengths can coexist in a smaller layered architecture while documenting rather than denying the trade-offs.

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

UCOF does not attempt to make every workload equally efficient under one layout, eliminate application-specific codecs or schemas, execute embedded parsers, guarantee universal semantic understanding, replace network protocols or distributed databases, or eliminate unavoidable trade-offs among compression, mutation, encryption, deduplication, simplicity, and random access.

## Proposed layered architecture

| Layer | Responsibility |
|---|---|
| Bootstrap framing | Magic, version or experimental epoch, bounded discovery fields |
| Canonical metadata | Deterministic structured metadata |
| Object storage | Logical objects represented by independently bounded chunks |
| Primary directory | Object-to-location discovery for random-access files |
| Secondary indexes | Optional profile-specific lookup accelerators |
| Schema system | Versioned schemas, physical and logical types, compatibility metadata |
| Transform pipeline | Compression, encryption, checks, and domain encodings in explicit order |
| Snapshot model | Append-only manifests, roots, checkpoints, and previous-root relationships |
| Trust layer | Digests, signatures, authenticated claims, and optional provenance |
| Profiles | Interoperability constraints for concrete domains and access patterns |

## Security posture

Every input byte is assumed hostile. The project explicitly considers checked arithmetic, forged or overlapping ranges, bounded metadata and allocation, decompression expansion, recursive object graphs, malicious indexes, parser differentials, digest and algorithm confusion, signature wrapping, metadata leakage, stale-root selection, external-reference confusion, and safe extraction or repair.

The primary threat model now incorporates executable EXP-0001 findings, including validation-order effects, exact-end versus recovery separation, the flat-directory scale failure, bounded source behaviour, fuzzing, and portability evidence.

See the [threat model](docs/THREAT_MODEL.md) and [security policy](SECURITY.md).

## Decision process

Implementation-local choices use Architecture Decision Records. Normative changes affecting serialized bytes, canonicalization, required behavior, compatibility, profiles, registries, or security-critical interpretation use Format Change Proposals.

A working implementation is evidence, not the specification. Permanent identifiers are not assigned before the defining proposal is accepted.

## Versioning and compatibility

Reference software will use Semantic Versioning after releases begin. Stable core and profile specifications will use separate `MAJOR.MINOR` versions.

Pre-stable incompatible layouts use monotonic experimental epochs such as `UCOF-EXP-0001`. Unknown epochs must be reported as unsupported rather than guessed. Experimental epochs never become stable specification version numbers.

## Planned profiles

Initial candidates are Archive, Table, Media, Document, Scientific, Database, and Package. Archive and Table remain the first intended validation profiles because they exercise materially different access patterns without requiring a full renderer or database engine.

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

## Contributing

Review, counterexamples, hostile test ideas, workload traces, format comparisons, and independent parsers are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) before substantial work. Use the issue forms for bugs, design questions, and proposal intake. Do not disclose unpatched vulnerabilities publicly.

## License

Repository material is available under the [MIT License](LICENSE) unless a file states otherwise.

“UCOF,” the `.ucof` extension, media types, magic bytes, registry namespaces, and profile identifiers remain provisional until technical and naming reviews are complete.
