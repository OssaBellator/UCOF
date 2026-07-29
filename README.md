# UCOF — Universal Chunked Object Format

> A language-neutral, self-describing, chunk-addressable container format for durable storage, partial access, streaming, verification, and extensible domain profiles.

## Status

**UCOF is an early design and research project.** No released specification exists yet, and files produced during development must be treated as experimental and disposable.

The initial objective is not to replace every existing format with one internal data model. UCOF instead aims to standardize a durable **container envelope** that can hold many kinds of data while preserving efficient, domain-specific representations.

See the [phased implementation plan](docs/IMPLEMENTATION_PLAN.md) for the proposed path from design work to a stable 1.0 specification and reference implementation.

## Motivation

Existing formats tend to optimize for one workload:

- JSON is portable and inspectable, but inefficient for large binary payloads and random access.
- SQLite provides transactions and indexed mutation, but is not a general media, document, or archival interchange format.
- Parquet and Arrow support efficient analytical data access, but do not model every object type or mutable workload.
- HDF5 provides rich scientific datasets and chunking, but has a comparatively large implementation surface.
- ZIP is widely supported, but its metadata, indexing, integrity, schema, and update model are limited.
- MKV and similar media containers stream well, but are specialized around timed media.
- PDF is portable and feature-rich, but its object model, edit history, and security behavior are complex.

UCOF explores whether these strengths can coexist in a smaller, layered architecture without pretending that their trade-offs can be eliminated.

## Design thesis

UCOF is a **universal container, not a universal representation**.

The core format should define:

1. Stable framing and version negotiation.
2. Deterministic metadata encoding.
3. Independently readable typed chunks.
4. Object discovery and random access.
5. Integrity verification and content identity.
6. Safe extension and capability rules.
7. Append-only snapshots and recovery points.
8. Optional compression, encryption, provenance, and secondary indexes.
9. Profiles for documents, media, tabular data, databases, scientific data, and packages.

A basic conforming reader should be able to inspect, verify, enumerate, copy, and extract objects it does not semantically understand. Profile-aware readers provide higher-level interpretation.

## Goals

UCOF is intended to provide:

- **Self-description:** schemas, logical types, codec identifiers, and profile declarations travel with the file.
- **Partial access:** readers can locate and decode selected objects without loading the whole file.
- **Streaming:** independently framed chunks and checkpoints support sequential production and consumption.
- **Determinism:** canonical metadata enables stable hashing, signing, reproducible output, and deduplication.
- **Extensibility:** unknown optional features remain safely skippable and preservable.
- **Integrity:** per-chunk digests and a signed or unsigned root identity detect corruption and substitution.
- **Recoverability:** append-only commits and checkpoint footers preserve the last valid snapshot after interrupted writes.
- **Security:** parsers operate under explicit limits; data is never executable merely because it is embedded.
- **Longevity:** the minimal core remains implementable without a large runtime or external schema registry.
- **Multiple access patterns:** different profiles can select different physical layouts and index structures.

## Non-goals

UCOF does not attempt to:

- Make every workload equally efficient under one layout.
- Eliminate the need for codecs, schemas, or application-specific software.
- Embed executable parsers or automatically run code supplied by a file.
- Guarantee that every reader understands every object type.
- Replace live network protocols, distributed databases, or source-control systems.
- Hide unavoidable trade-offs among compression, mutation, encryption, deduplication, simplicity, and random access.
- Standardize a single schema language for all domains.

## Proposed architecture

The initial architecture is layered so that a minimal reader can remain small.

| Layer | Responsibility |
|---|---|
| Core framing | Magic value, version, feature flags, file identity, chunk framing, footer discovery |
| Canonical metadata | Deterministically encoded structured metadata, initially expected to use deterministic CBOR |
| Object storage | Typed chunks with stable object identifiers and explicit dependencies |
| Primary directory | Object-to-location mapping required for random access |
| Secondary indexes | Optional B+tree, sorted, column/page, Bloom, spatial, temporal, or full-text indexes |
| Schema system | Embedded or referenced versioned schemas and logical-type declarations |
| Transform pipeline | Per-chunk compression, checksums, encryption, and domain encodings |
| Snapshot model | Append-only manifests, commit records, checkpoints, and previous-root links |
| Trust layer | Hash trees, digital signatures, authenticated claims, and provenance events |
| Profiles | Interoperability constraints for documents, media, tables, databases, scientific data, and packages |

### Conceptual file layout

```text
+--------------------------------------------------+
| Fixed header                                     |
| magic | core version | feature flags | file ID   |
+--------------------------------------------------+
| Initial manifest or bootstrap metadata           |
+--------------------------------------------------+
| Chunk: metadata                                  |
+--------------------------------------------------+
| Chunk: payload                                   |
+--------------------------------------------------+
| Chunk: payload                                   |
+--------------------------------------------------+
| ... additional chunks and checkpoints ...        |
+--------------------------------------------------+
| Object directory and optional secondary indexes  |
+--------------------------------------------------+
| Provenance, integrity, and signature records      |
+--------------------------------------------------+
| Footer                                            |
| root offset | root digest | previous root | magic |
+--------------------------------------------------+
```

The exact byte layout is deliberately not frozen yet. It will be finalized only after prototype implementations, corpus testing, parser-hardening work, and independent review.

## Core concepts

### Objects and chunks

An **object** is a logical entity such as a manifest, schema, image, table, index, signature, or document node. A **chunk** is a physical storage unit containing all or part of an object.

Objects may span chunks, but each chunk must be independently bounded and safely skippable. Chunk metadata is expected to include:

- object identifier;
- chunk sequence or logical range;
- object and physical encoding type;
- stored and logical lengths;
- codec and transform identifiers;
- integrity digest;
- dependency references;
- capability requirements.

### Object identity

UCOF is expected to support both:

- **instance identifiers**, which distinguish objects within an evolving file; and
- **content identifiers**, derived from canonical content for verification and deduplication.

The design must avoid assuming that encrypted or mutable content always has a stable public content hash.

### Directory and indexes

Every random-access UCOF file will contain a primary object directory. Workload-specific indexes are optional and profile-defined.

No single index structure is suitable for all workloads:

- B+trees suit point lookups and mutable key-value data.
- Sorted block indexes suit immutable archives.
- Page and column indexes suit analytical scans.
- Bloom filters accelerate negative membership checks.
- Spatial and temporal indexes support geometry and timed streams.

Indexes are accelerators, not sources of truth. Readers must be able to validate or rebuild non-authoritative indexes from authoritative objects.

### Schemas and evolution

Schemas may be embedded or referenced by stable identifier. The core will define compatibility metadata and preservation rules, while profiles may select schema languages.

The format must support:

- versioned schemas;
- unknown-field preservation where the selected schema system permits it;
- optional and required fields;
- logical types layered over physical primitives;
- explicit migration declarations;
- schema fingerprints and dependency identities.

### Binary and text forms

The authoritative representation will be canonical binary. A deterministic textual projection will support diagnostics, review, fixtures, and tooling.

The text form must be lossless for core values, including binary data, references, floating-point edge cases, tags, and unknown extensions. It must not become a second competing source of truth inside a file.

### Compression and transforms

Transforms operate independently per chunk or chunk group. The design is expected to allow:

- no compression;
- Zstandard as a likely general-purpose baseline codec;
- shared compression dictionaries;
- domain-specific media and column encodings;
- checksums or authenticated encryption;
- explicit transform ordering.

A conforming reader must reject unsafe pipelines, excessive expansion, invalid lengths, unsupported required transforms, and ambiguous transform ordering.

### Snapshots and mutation

The proposed mutation model is append-only and copy-on-write:

1. Append new or replacement objects.
2. Append updated indexes.
3. Append a new manifest describing the snapshot.
4. Append a footer that atomically exposes the new root.

Older roots remain available until compaction. An interrupted write should leave the previous valid snapshot readable.

This model favors durability and verifiability over in-place mutation. A database profile may define additional page and concurrency rules while retaining recoverable root commits.

### Integrity, signatures, and provenance

Integrity and authenticity are distinct:

- A digest proves that bytes have not changed relative to an expected identity.
- A signature binds an identity or claim to a signer.
- Provenance records assert how content was created or transformed.

UCOF should distinguish verified signed history, unsigned asserted history, incomplete history, and absent history. Provenance must remain optional because it can expose sensitive information.

### Encryption

Encryption is planned at object or chunk granularity. This can permit public manifests with private payloads, separate recipients, and selective disclosure.

Metadata confidentiality must be explicit. A public directory can leak object names, types, sizes, relationships, and statistics even when payloads are encrypted.

## Capability model

Every extension or feature will be classified as:

- **Required:** interpretation is unsafe or impossible without support.
- **Optional:** a reader may skip the feature while preserving the surrounding file.
- **Advisory:** the feature only improves presentation, performance, or diagnostics.

Readers must fail closed when an unknown required capability is encountered. Writers must not mark a semantically required feature as optional merely to increase apparent compatibility.

## Planned profiles

Profiles constrain the base container for interoperability. Initial candidates are:

- **UCOF Archive:** files, directories, metadata, deduplication, and extraction.
- **UCOF Table:** column chunks, dictionaries, statistics, page indexes, and schema evolution.
- **UCOF Media:** timestamped tracks, cues, subtitles, chapters, and streaming indexes.
- **UCOF Document:** structured content, resources, annotations, accessibility, and optional page layout.
- **UCOF Scientific:** multidimensional arrays, units, coordinates, chunk filters, and experiment metadata.
- **UCOF Database:** mutable logical records, transactional roots, B+tree-style indexes, and compaction.
- **UCOF Package:** dependency metadata, software artifacts, signatures, and installation constraints.

The first implementation should validate the core with **Archive** and **Table** profiles before attempting more complex document, media, or database semantics.

## Reference implementation direction

The proposed reference implementation will use **Rust** because memory safety, explicit binary-layout handling, fuzzing support, and predictable performance are valuable for an untrusted file parser.

This is not a requirement for other implementations. The specification must remain language-neutral and practical to implement in C, C++, Go, Java, JavaScript, Python, and other ecosystems.

Expected workspace components:

```text
crates/
  ucof-core/        Core types, framing, limits, and validation
  ucof-codec/       Reader and writer APIs
  ucof-schema/      Schema descriptors and compatibility checks
  ucof-index/       Primary directory and optional index implementations
  ucof-crypto/      Hashing, signatures, and encryption adapters
  ucof-text/        Canonical diagnostic text projection
  ucof-cli/         inspect, verify, pack, unpack, convert, and repair
profiles/
  archive/
  table/
spec/
  core.md
  registries.md
  profiles/
tests/
  corpus/
  conformance/
  malformed/
```

The repository structure will be introduced incrementally rather than created as empty scaffolding.

## CLI concept

The reference CLI is expected to expose commands similar to:

```console
ucof inspect example.ucof
ucof verify example.ucof
ucof pack ./directory -o archive.ucof
ucof unpack archive.ucof -o ./output
ucof to-text example.ucof -o example.ucof.txt
ucof from-text example.ucof.txt -o rebuilt.ucof
ucof repair damaged.ucof -o recovered.ucof
ucof compact history.ucof -o compacted.ucof
```

Command names and behavior are provisional.

## Security principles

UCOF readers must assume every input is hostile. The specification and reference implementation will require:

- checked arithmetic for offsets and lengths;
- maximum nesting, allocation, object, chunk, and expansion limits;
- cycle detection for object references;
- rejection of overlapping or contradictory ranges;
- bounded decompression and transform execution;
- no implicit network retrieval;
- no automatic execution of embedded code;
- cryptographic verification that does not trust unverified indexes;
- distinguishable warnings for malformed, unverifiable, unsupported, and incomplete files;
- fuzzing, property testing, malformed corpora, and differential testing.

## Compatibility policy

Until the 1.0 specification is published:

- all byte layouts are unstable;
- no backward compatibility is promised;
- experimental files should carry an explicit pre-release marker;
- prototype identifiers must not be registered as permanent values;
- breaking changes should be documented in the repository.

After 1.0, the project intends to preserve the core framing contract and use registries, profiles, and required/optional capabilities for evolution.

## Development roadmap

The project will progress through these broad stages:

1. Define principles, threat model, terminology, and decision process.
2. Specify and prototype minimal framing, canonical metadata, and footer discovery.
3. Implement bounded readers, deterministic writers, and a conformance corpus.
4. Add object directories, streaming checkpoints, recovery, and append-only snapshots.
5. Add transform pipelines, schemas, and canonical text tooling.
6. Add integrity trees, signatures, provenance, and optional encryption.
7. Validate the model with Archive and Table profiles.
8. Build independent implementations and interoperability tests.
9. Freeze the 1.0 wire format only after security and performance review.

Detailed deliverables and exit criteria are in [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md).

## Contributing

The project is currently defining its foundations. High-value contributions include:

- concrete use cases and workload traces;
- adversarial format review;
- comparisons with existing container designs;
- minimal parser prototypes;
- canonicalization test vectors;
- corruption and recovery experiments;
- benchmark corpora;
- schema-evolution examples;
- security and privacy threat analysis.

Large implementation changes should follow an accepted design proposal once the proposal process is established. Prematurely freezing identifiers or byte layouts should be avoided.

## Design principles for contributors

1. Keep the mandatory core smaller than the optional ecosystem.
2. Prefer explicit failure over ambiguous recovery.
3. Make unknown optional data preservable.
4. Treat indexes as rebuildable accelerators.
5. Separate integrity, authenticity, provenance, and confidentiality.
6. Require deterministic representations wherever identity depends on bytes.
7. Benchmark realistic partial reads, not only full-file throughput.
8. Design for hostile inputs before optimizing trusted ones.
9. Document trade-offs instead of claiming to eliminate them.
10. Freeze the wire format only after more than one implementation exists.

## Licensing

No license has been selected yet. Until a license file is added, normal copyright restrictions apply. A permissive license is recommended before accepting substantial external contributions.

## Name and identifiers

“UCOF,” the `.ucof` extension, media types, magic bytes, registry namespaces, and profile identifiers are provisional. Permanent registration should wait until the core format is technically mature and the project has completed a naming and collision review.
