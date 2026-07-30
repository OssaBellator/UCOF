# UCOF — Universal Chunked Object Format

> A language-neutral, self-describing, chunk-addressable container format for durable storage, partial access, streaming, verification, recovery, and extensible domain profiles.

## Project status

**UCOF is an early design and research project.** No stable specification, production wire format, or production-compatible file exists yet.

Current implementation stage: **Phase 3 — first concrete directory, snapshot, recovery, and rewrite candidate under active research**.

Two disposable epochs are represented in the repository:

- `UCOF-EXP-0001` proves minimal framing and Phase 2 safety-first reader/writer APIs;
- `UCOF-EXP-0002` Candidate 1 tests authenticated paged directories, append snapshots, exact-end publication, complete checkpoints, bounded source access, linked history, recovery, repair, and caller-selected rewrite.

Neither epoch is stable. Candidate 1 works across Rust and an independent in-repository Python implementation, but FCP-0002 remains Draft and its bytes may be retired. Current evidence has already found one architectural blocker: the page sequence field prevents byte-for-byte historical page reuse.

- [Phase 3 status](docs/PHASE_3_STATUS.md)
- [Phase 3 experimental CLI guide](docs/PHASE_3_CLI_GUIDE.md)
- [EXP-0002 Candidate 1 byte specification](docs/spec/EXP_0002_BYTE_CANDIDATE.md)
- [FCP-0002 proposal](docs/proposals/0002-exp-0002-directory-snapshots.md)
- [EXP-0002 concrete security findings](docs/security/EXP_0002_BYTE_FINDINGS.md)
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

## Current experimental cores

### UCOF-EXP-0001

The first executable framing experiment includes:

- a 32-byte fixed bootstrap header;
- contiguous 40-byte record headers;
- opaque object records;
- canonical metadata for the manifest and flat directory;
- an exact-end 80-byte footer;
- SHA-256 over all bytes before the footer;
- strict structural and directory cross-validation;
- deterministic writing and object lookup;
- independently generated valid and invalid vectors.

Phase 2 adds bounded implementation-local APIs over those bytes:

- metadata-only inspection over borrowed and seekable sources;
- strict source validation with bounded block hashing;
- non-seeking sequential events with bounded payload chunks;
- strict diagnostics that never convert integrity failure into validity;
- explicitly unverified prefix salvage;
- deterministic streaming and seekable writers with explicit finalization;
- separate accounting for reads, hashes, allocations, logical bytes, and diagnostics;
- property tests, sparse virtual-source tests, cross-language corpus checks, and continuous fuzzing.

EXP-0001 deliberately excludes append history and has a measured flat-directory scale failure. It remains useful evidence, not a promotion candidate.

### UCOF-EXP-0002 Candidate 1

The first Phase 3 byte candidate includes:

- a 64-byte bootstrap header;
- 48-byte object headers followed by opaque payloads;
- fixed 16 KiB authenticated directory pages;
- 88-byte leaf entries and 64-byte internal entries;
- domain-separated object, page, snapshot, and commit SHA-256 digests;
- variable snapshot records with parent and previous-footer relationships;
- 160-byte exact-end commit footers;
- deterministic genesis and append writers;
- strict full validation over slices and bounded random-access sources;
- authenticated single-object lookup and absence results;
- verified linked-history enumeration;
- separately requested bounded recovery;
- complete checkpoints represented as ordinary valid commits;
- repair-to-new-file and caller-directed object-selection rewrite;
- strong caller-supplied source-version checking for mutable range sources;
- byte-identical Rust/Python valid vectors;
- thirteen pinned invalid and interrupted vectors;
- twenty-one fuzz targets plus layer-targeted adversarial tests.

The assurance modes are intentionally distinct:

- full strict validation rehashes every object referenced by the active directory;
- targeted lookup verifies the active commit, snapshot, one page path, and selected object or absence;
- linked history validates the exact previous-footer chain one strict prefix at a time;
- recovery scans only when requested and reports only candidates that pass full strict-prefix validation;
- caller-selected rewrite does not claim automatic semantic compaction.

A cold-cache localhost HTTP Range benchmark over an append containing an unrelated 1 MiB historical object measured 7 requests and 16,993 transferred bytes for targeted lookup, versus 26 requests and 1,065,673 bytes for full validation. Targeted lookup did not read the large historical object; full validation did.

Candidate 1 also exposes important negative evidence:

- the 88-byte leaf entry costs approximately 8.28 GiB at 100 million objects;
- sixteen reserved bytes per leaf cost about 1.50 GiB at that scale;
- the page sequence field is authenticated and required to equal the active snapshot sequence, so unchanged historical pages cannot be reused byte-for-byte;
- complete-only checkpoint cadence requires a batched writer strategy rather than naive per-object path copying;
- bounded external sorting is feasible, but is not yet integrated into the byte writer.

Candidate 1 still excludes transforms, compression, encryption, signatures, provenance, external references, schemas, profiles, and semantic dependency discovery. It has no trusted freshness mechanism.

No field width, magic value, page size, digest, footer rule, identifier width, or identity scope is stable merely because the implementation works.

## Quick start

The reference implementation has a provisional Rust 1.85 minimum supported version.

Run the complete locked Rust suite:

```console
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
```

Exercise the EXP-0001 CLI assurance levels:

```console
cargo run --locked -p ucof-cli --bin ucof -- make-demo demo.ucof
cargo run --locked -p ucof-cli --bin ucof -- inspect demo.ucof
cargo run --locked -p ucof-cli --bin ucof -- verify demo.ucof
cargo run --locked -p ucof-cli --bin ucof -- diagnose demo.ucof
cargo run --locked -p ucof-cli --bin ucof -- salvage demo.ucof
```

| Command | Meaning |
|---|---|
| `inspect` | Structural metadata inventory; payload integrity is not checked |
| `verify` | Strict bounded committed-prefix integrity validation |
| `diagnose` | Strict verified-or-invalid status with categorized failures |
| `salvage` | Unverified complete-prefix record discovery; never a conformance claim |
| `make-demo` | Deterministic EXP-0001 output through explicit finalization |

Exercise the isolated EXP-0002 Candidate 1 CLI:

```console
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- verify archive.ucof
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- roots archive.ucof
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- history archive.ucof
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- lookup archive.ucof 42
cargo run --locked -p ucof-experiments --bin ucof-exp0002 -- recover damaged.ucof
```

Repair and rewrite commands require a new output path and explicit 16-byte file identity and nonce values. See the [Phase 3 CLI guide](docs/PHASE_3_CLI_GUIDE.md) before using them.

Verify the independent EXP-0002 implementation and evidence:

```console
python3 tools/exp0002_codec.py --self-test
python3 tools/exp0002_codec.py --verify-vectors tests/vectors/exp-0002
python3 tools/exp0002_invalid_vectors.py --verify-vectors tests/vectors/exp-0002-invalid
python3 tools/test_exp0002_adversarial.py
python3 tools/experiment_exp0002_page_sequence_reuse.py
python3 tools/experiment_exp0002_http_range.py
python3 tools/experiment_exp0002_external_sort.py
```

Canonical hexadecimal fixtures and expected metadata are stored under `tests/vectors/`. Generated ad hoc `.ucof` binaries are ignored by Git.

## Current repository structure

```text
crates/
  ucof-core/         Bounded EXP-0001 readers, validators, diagnostics, and writers
  ucof-cli/          EXP-0001 inspect, verify, diagnose, salvage, and make-demo commands
  ucof-experiments/  Unpublished Phase 3 models, Candidate 1 codec, source APIs, and CLI
fuzz/                 Standalone cargo-fuzz package and model/parser/writer targets
spec/
  experimental/      Disposable experimental specifications
docs/
  spec/              Provisional independently implementable candidate specifications
  decisions/         Architecture Decision Records
  proposals/         Normative Format Change Proposals
  experiments/       Reproducible measurements and rejected alternatives
  security/          Executable security findings and supporting evidence
tests/
  vectors/           Reproducible valid and invalid byte fixtures
tools/                Independent codecs, generators, adversarial tests, and experiments
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

Every input byte is assumed hostile. The project explicitly considers checked arithmetic, forged or overlapping ranges, bounded metadata and allocation, decompression expansion, recursive object graphs, malicious indexes, parser differentials, digest and algorithm confusion, signature wrapping, metadata leakage, stale-root selection, external-reference confusion, source mutation, temporary-spill leakage, and safe extraction or repair.

The primary threat model incorporates executable EXP-0001 and EXP-0002 findings, including validation-order effects, exact-end versus recovery separation, authenticated page and object cross-checks, range-source work budgets, stable source views, page-identity limitations, identity scopes, repair boundaries, fuzzing, and portability evidence.

See the [threat model](docs/THREAT_MODEL.md), [EXP-0002 concrete findings](docs/security/EXP_0002_BYTE_FINDINGS.md), and [security policy](SECURITY.md).

## Decision process

Implementation-local choices use Architecture Decision Records. Normative changes affecting serialized bytes, canonicalization, required behavior, compatibility, profiles, registries, or security-critical interpretation use Format Change Proposals.

A working implementation is evidence, not the specification. Permanent identifiers are not assigned before the defining proposal is accepted.

## Versioning and compatibility

Reference software will use Semantic Versioning after releases begin. Stable core and profile specifications will use separate `MAJOR.MINOR` versions.

Pre-stable incompatible layouts use monotonic experimental epochs such as `UCOF-EXP-0001` and `UCOF-EXP-0002`. Unknown epochs must be reported as unsupported rather than guessed. Experimental epochs never become stable specification version numbers.

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
