# UCOF Phased Implementation Plan

## 1. Purpose

This document defines a staged path for taking UCOF from a design hypothesis to a stable, independently implementable container specification.

The plan deliberately separates **experimentation**, **validation**, and **standardization**. A file format becomes expensive to correct after real data depends on it, so the wire format must not be frozen merely because the first implementation works.

The project should advance between phases only when the phase exit criteria are met. Work may overlap where dependencies permit, but later features must not be used to conceal unresolved core-format problems.

## 2. Desired end state

A UCOF 1.0 release should include:

- a language-neutral normative core specification;
- stable binary framing and deterministic metadata rules;
- a reference implementation with bounded streaming and random-access readers;
- deterministic writer behavior and canonical test vectors;
- a required primary object directory for random-access files;
- append-only snapshots, checkpoints, recovery, and compaction rules;
- registries for capabilities, object types, transforms, digests, schemas, and profiles;
- at least two useful profiles, initially Archive and Table;
- a command-line tool for inspection, verification, conversion, extraction, repair, and conformance testing;
- a malformed and adversarial test corpus;
- fuzzing and property tests integrated into continuous integration;
- at least one independent implementation or parser sufficient to validate the specification;
- an interoperability test suite and published compatibility matrix;
- a security and privacy analysis reviewed independently of the primary implementation;
- benchmark results covering sequential reads, partial reads, metadata-only inspection, recovery, and memory usage;
- migration rules for pre-1.0 experimental files;
- a documented compatibility policy for post-1.0 evolution.

## 3. Guiding constraints

All phases must respect the following constraints.

### 3.1 The core remains small

The mandatory framing layer must be implementable without supporting every schema, compression codec, cryptographic suite, or profile.

### 3.2 Unknown data remains manageable

A reader must be able to distinguish:

- an unknown optional feature that can be skipped and preserved;
- an unknown advisory feature that may be ignored;
- an unknown required feature that makes safe interpretation impossible.

### 3.3 Parsing is non-executable

Embedded data may describe schemas, codecs, provenance, or rendering behavior, but it must not automatically execute embedded code or fetch network resources.

### 3.4 Indexes are not authoritative

Indexes accelerate access. They must be validated against authoritative objects or be rebuildable from them.

### 3.5 Determinism is scoped and explicit

Canonical bytes are required wherever hashes, signatures, deduplication, or reproducibility depend on byte identity. Non-canonical encodings must not accidentally participate in canonical identities.

### 3.6 Security limits are part of conformance

Maximum allocations, nesting, recursion, expansion, object counts, and transform work must be configurable and enforced. A parser that only succeeds on trusted files is not conforming.

### 3.7 Profiles prove the core

The core architecture is not considered validated until it efficiently supports materially different workloads. Archive and Table are selected first because they exercise hierarchy, metadata, deduplication, streaming, columnar chunks, schema evolution, and partial access without requiring full media or document rendering systems.

## 4. Recommended implementation strategy

The initial reference implementation should be written in Rust while keeping the specification language-neutral.

Recommended workspace evolution:

```text
crates/
  ucof-core/          Core value types, identifiers, limits, errors
  ucof-codec/         Framing, streaming reader, random-access reader, writer
  ucof-index/         Primary directory and optional indexes
  ucof-schema/        Schema descriptors and compatibility logic
  ucof-transform/     Compression and transform pipeline interfaces
  ucof-crypto/        Digests, signatures, encryption adapters
  ucof-text/          Canonical diagnostic text representation
  ucof-cli/           User-facing commands
profiles/
  archive/
  table/
spec/
  core.md
  canonicalization.md
  security.md
  registries.md
  profiles/
tests/
  vectors/
  conformance/
  malformed/
  corpus/
  differential/
benches/
  sequential/
  random_access/
  metadata/
  recovery/
docs/
  decisions/
  proposals/
```

This structure should be created incrementally. Empty crates and directories should not be added merely to resemble a complete project.

## 5. Phase summary

| Phase | Name | Primary outcome |
|---|---|---|
| 0 | Foundations and governance | Shared vocabulary, use cases, threat model, decision process, and repository policy |
| 1 | Minimal wire-format experiment | Small executable prototype for header, chunks, canonical metadata, and footer discovery |
| 2 | Safety-first core codec | Bounded streaming and random-access reader/writer with deterministic vectors |
| 3 | Directory, snapshots, and recovery | Partial object access, append-only commits, checkpoints, repair, and compaction |
| 4 | Transform pipeline and compression | Safe per-chunk transforms with baseline compression and expansion limits |
| 5 | Schemas and text projection | Versioned schemas, logical types, compatibility checks, and lossless diagnostic text |
| 6 | Integrity, signatures, and provenance | Content identities, root verification, signed claims, and append-only provenance |
| 7 | Encryption and selective disclosure | Authenticated encryption, recipient keys, metadata modes, and leakage documentation |
| 8 | Archive and Table profiles | Two interoperable profiles proving distinct access patterns |
| 9 | Interoperability, tooling, and hardening | Independent implementation, conformance harness, benchmarks, and adversarial review |
| 10 | Specification freeze and 1.0 | Stable format, registries, releases, compatibility commitment, and adoption package |

---

# Phase 0 — Foundations and governance

## Objective

Turn the concept into a reviewable engineering project before committing to a byte layout.

## Deliverables

### Project policy

- Select and add an open-source license.
- Add `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, and `SECURITY.md`.
- Define supported communication and decision channels.
- Establish semantic versioning for software and a separate version scheme for the specification.
- Define how experimental format versions are identified and invalidated.

### Terminology

Publish a glossary for at least:

- file;
- snapshot;
- manifest;
- object;
- chunk;
- payload;
- directory;
- index;
- schema;
- physical type;
- logical type;
- transform;
- capability;
- profile;
- digest;
- content identity;
- instance identity;
- signature;
- provenance claim;
- checkpoint;
- compaction.

### Use-case corpus

Document concrete scenarios rather than generic claims. Each scenario should describe file size, object count, expected access patterns, mutability, trust boundaries, and failure modes.

Minimum scenarios:

1. Small human-created archive with hundreds of files.
2. Multi-gigabyte archive with millions of objects and duplicate payloads.
3. Analytical table with selective column and row-group reads.
4. Append-only sensor capture interrupted during writing.
5. Signed document package with private attachments.
6. Encrypted data where directory metadata must also remain confidential.
7. Damaged file with a valid earlier checkpoint.
8. Reader that understands the core but not the file's profile.
9. Schema evolution with old and new readers.
10. Malicious file designed to trigger excessive allocation, decompression, recursion, or index traversal.

### Threat model

Cover at least:

- integer overflow and offset wraparound;
- forged lengths and overlapping ranges;
- decompression bombs;
- deeply nested metadata;
- recursive and cyclic object graphs;
- malicious indexes;
- hash substitution and algorithm confusion;
- signature wrapping and ambiguous signed scope;
- encrypted metadata leakage;
- parser differential behavior;
- external-reference confusion;
- unbounded dictionary or schema expansion;
- recovery logic that accepts stale or attacker-selected roots.

### Decision process

Introduce:

- Architecture Decision Records for implementation-local choices.
- Format Change Proposals for normative wire-format decisions.
- A registry-allocation policy.
- A rule that permanent numeric identifiers are not assigned until the related proposal is accepted.
- A compatibility-impact section in every format proposal.

## Exit criteria

Phase 0 is complete when:

- the repository has a license and contribution/security policies;
- terminology is consistent across documents;
- at least ten representative use cases are reviewed;
- the initial threat model is published;
- the proposal and decision templates are accepted;
- disputed requirements are recorded as open decisions rather than hidden assumptions;
- the team agrees on what must be proven before UCOF 1.0.

## Key decisions

- Project license.
- Specification license.
- Governance model and proposal approval authority.
- Whether the project accepts a single reference implementation as normative evidence or requires independent implementation before stabilization.

---

# Phase 1 — Minimal wire-format experiment

## Objective

Create the smallest possible end-to-end format experiment that can encode, locate, inspect, and verify a handful of objects without prematurely adding profiles or advanced features.

## Scope

The prototype should contain only:

- a fixed bootstrap header;
- an experimental format marker;
- bounded chunk framing;
- deterministic core metadata;
- object identifiers;
- one minimal manifest;
- a trailing footer that locates the active root;
- a simple primary directory representation;
- one baseline digest algorithm;
- no compression, encryption, signatures, or external references.

## Deliverables

### Experimental specification

Write an intentionally provisional document defining:

- byte order;
- integer encoding rules;
- alignment and padding behavior;
- header and footer discovery;
- chunk length semantics;
- object identity fields;
- metadata encoding requirements;
- error behavior for unknown required capabilities;
- truncation and trailing-data behavior;
- how the active snapshot is selected.

### Prototype codec

Implement:

- an in-memory writer;
- a sequential reader;
- a random-access reader over a seekable source;
- an inspector that prints file structure without interpreting profile data;
- strict validation and a separate diagnostic mode;
- explicit limits passed through a reader configuration object.

### Golden vectors

Publish small fixtures for:

- the smallest valid file;
- multiple objects;
- unknown optional metadata;
- unknown required capability;
- zero-length payload;
- truncated header;
- truncated chunk;
- invalid footer offset;
- mismatched digest;
- integer overflow attempt;
- duplicate object identifier;
- overlapping directory entries.

Each valid fixture should include:

- source description;
- binary file;
- annotated hexadecimal layout;
- canonical diagnostic output;
- expected object inventory;
- expected root digest.

## Experiments required before exit

- Compare fixed-width and variable-width length fields.
- Measure footer discovery with large trailing payloads.
- Test whether deterministic CBOR meets all required metadata semantics without custom canonicalization exceptions.
- Confirm that a minimal parser can enumerate unknown object types without profile code.
- Simulate interrupted writes at every byte boundary near the footer and root commit.
- Verify that an old valid root remains unambiguous after an interrupted append.

## Exit criteria

- Valid fixtures round-trip deterministically.
- Invalid fixtures produce stable, categorized errors without panics.
- Random access locates an object without scanning unrelated payload chunks.
- Sequential reading works without seeking when the file is written in stream-compatible order.
- Footer discovery is robust against truncation and unrelated trailing bytes according to the documented policy.
- The experimental specification contains no undefined byte ranges or implicit host-language behavior.
- At least one design alternative has been rejected with documented evidence for each irreversible framing decision.

## Key decisions

- Header size and fixed versus extensible bootstrap fields.
- Footer locator strategy.
- Byte order.
- Core integer representation.
- Deterministic metadata encoding and permitted subset.
- Object and chunk identifier widths.
- Baseline digest algorithm and algorithm-tagging rules.

---

# Phase 2 — Safety-first core codec

## Objective

Convert the Phase 1 experiment into a maintainable, hostile-input-resistant core library without yet freezing the wire format.

## Deliverables

### Reader APIs

Provide distinct APIs for:

- sequential event reading;
- random-access object lookup;
- metadata-only inspection;
- strict conformance validation;
- salvage-oriented diagnostics that never silently upgrade damaged data to valid data.

The API must support user-provided limits for:

- total bytes read;
- logical decoded bytes;
- object and chunk counts;
- metadata nesting;
- string and byte-string lengths;
- dependency depth;
- allocation size;
- diagnostic count;
- transform expansion, even before transforms are implemented.

### Writer APIs

Provide:

- deterministic output mode;
- streaming output mode;
- seekable optimized output mode;
- explicit finalization;
- duplicate-identifier prevention;
- validation before root publication;
- reproducible fixture generation.

### Error model

Use structured errors that distinguish:

- malformed bytes;
- unsupported required capability;
- unsupported optional feature;
- resource limit exceeded;
- integrity failure;
- unavailable external dependency;
- invalid schema or profile;
- recoverable truncation;
- internal implementation error.

### Continuous integration

Add checks for:

- formatting and linting;
- unit tests;
- property tests;
- corpus tests;
- fuzz target compilation;
- documentation examples;
- minimum supported Rust version, once selected;
- 32-bit and 64-bit arithmetic assumptions where practical;
- little-endian and big-endian simulation tests even if only one wire byte order is used.

### Fuzzing

Create targets for:

- header parsing;
- footer discovery;
- chunk iteration;
- canonical metadata decoding;
- directory parsing;
- full-file validation;
- writer-reader round trips;
- mutation of valid golden files.

## Quality requirements

- Parsing untrusted input must not use unchecked offset arithmetic.
- Library code must not panic for malformed input.
- A reader must not allocate based only on untrusted declared lengths.
- Error paths must be tested, not only successful decoding.
- Unknown optional data should be preservable when rewriting where the API promises preservation.
- Public types must not expose unstable wire-layout internals unnecessarily.

## Exit criteria

- Fuzzing runs continuously or on a documented scheduled workflow.
- Property tests cover deterministic round trips and truncation behavior.
- A metadata-only inspection of a large sparse test file does not read payload bodies.
- Configured limits reliably stop oversized metadata and allocation attempts.
- No known parser panic or unbounded allocation remains in the malformed corpus.
- API documentation clearly separates trusted convenience APIs from bounded untrusted-input APIs.

## Key decisions

- Sync I/O abstraction and optional async support strategy.
- Zero-copy borrowing versus owned values.
- Preservation model for unknown fields and objects.
- Whether canonical validation is always enforced or exposed as an explicit strict mode for some structures.

---

# Phase 3 — Directory, snapshots, and recovery

## Objective

Deliver the access and durability properties that distinguish UCOF from a simple chunked archive.

## Deliverables

### Primary directory

Define a mandatory directory capable of locating objects and their chunks. At minimum, entries should describe:

- object identifier;
- object type;
- physical offset;
- stored length;
- logical length when known;
- chunk count or range;
- required capabilities;
- transform pipeline reference;
- schema reference where applicable;
- integrity reference.

The directory format must support efficient lookup without requiring every entry to be decoded for a single-object query.

### Snapshot model

Define:

- root manifest identity;
- parent-root link;
- snapshot sequence or ordering rules;
- commit completeness criteria;
- active-root selection;
- behavior when multiple candidate roots exist;
- behavior after file concatenation or copying;
- whether history retention is mandatory, optional, or profile-controlled.

### Checkpoints

Support periodic checkpoints for long-running streams. A checkpoint must state whether it is:

- a complete independently readable snapshot;
- a partial progress marker;
- a recovery hint that still depends on earlier state.

### Repair tooling

Implement:

- footer scanning under bounded policy;
- valid-root enumeration;
- directory reconstruction where authoritative data permits;
- orphan-object reporting;
- extraction from the last valid snapshot;
- repair into a new file rather than destructive in-place mutation by default.

### Compaction

Implement a compactor that:

- selects a root;
- copies only reachable objects unless history retention is requested;
- rebuilds indexes;
- preserves or intentionally reissues provenance according to documented rules;
- does not falsely preserve signatures whose signed byte scope changes.

## Required tests

- Crash simulation after each append step.
- Truncation at every byte boundary around root publication.
- Duplicate footer candidates.
- Valid older root followed by corrupt newer root.
- Directory pointing outside the file.
- Directory cycles or self-reference.
- Million-object synthetic directory lookup.
- Compaction equivalence at the logical object-graph level.
- Recovery with unknown optional objects.

## Exit criteria

- A reader can locate a selected object without scanning unrelated payloads.
- The previous valid snapshot survives every simulated interrupted append before root publication.
- Recovery never silently selects an invalid newer root over a valid older root.
- Non-authoritative indexes can be rebuilt.
- Compaction preserves the selected logical snapshot and reports invalidated signatures or provenance consequences.
- Directory memory use can be bounded or paged for very large object counts.

## Key decisions

- Directory physical structure: paged sorted entries, B+tree, or another layout.
- Root commit atomicity assumptions on ordinary filesystems and object stores.
- History retention defaults.
- Footer scan bounds and stale-root selection rules.
- Object reachability and garbage definition.

---

# Phase 4 — Transform pipeline and compression

## Objective

Add independently decodable data transforms without turning the core reader into an unrestricted plugin runtime.

## Deliverables

### Transform model

Define a transform pipeline with:

- stable transform identifiers;
- explicit ordering;
- input and output length semantics;
- required versus optional support;
- bounded parameter metadata;
- dictionary and dependency references;
- integrity scope before and after transforms;
- rules for deterministic output.

### Baseline transforms

Implement:

- identity/no transform;
- one lightweight checksum for accidental corruption where useful;
- Zstandard as the initial general-purpose compression option;
- optional shared dictionaries with explicit identity and size limits.

The core specification should not require every implementation to support Zstandard unless a selected profile mandates it.

### Safety controls

Enforce:

- maximum decoded size;
- maximum expansion ratio;
- maximum dictionary size;
- maximum transform chain length;
- rejection of recursive transform dependencies;
- early cancellation;
- exact output-length checks when declared;
- no codec autodetection from payload bytes.

### Codec registry

Define allocation ranges for:

- permanent standard transforms;
- experimental transforms;
- vendor or private-use transforms;
- deprecated identifiers.

## Benchmarks

Measure:

- full sequential decompression;
- partial chunk reads;
- small-object overhead;
- dictionary effectiveness;
- random access under different chunk sizes;
- peak memory under adversarial declared sizes;
- cost of digest verification before and after decompression.

The project should publish results across several chunk sizes rather than declaring one universal default.

## Exit criteria

- Each compressed chunk can be decoded independently when its declared dependencies are available.
- Unsupported required transforms fail before payload interpretation.
- Expansion limits stop bomb-style inputs without large allocations.
- Transform order and integrity scope are unambiguous.
- Deterministic compression settings are documented for reproducible mode.
- Benchmarks demonstrate the trade-off curve between chunk size, ratio, and random-access cost.

## Key decisions

- Whether digests cover stored bytes, logical bytes, or both.
- Dictionary lifetime and sharing rules.
- Baseline compression profile requirements.
- Whether transform parameters are inline or referenced objects.

---

# Phase 5 — Schemas and canonical text projection

## Objective

Make structured objects self-describing and safely evolvable while providing a human-inspectable representation that does not compete with canonical binary storage.

## Deliverables

### Core physical value model

Define the smallest interoperable set of physical values needed for manifests and generic inspection, including:

- unsigned and signed integers;
- byte strings;
- Unicode text;
- arrays;
- maps with deterministic key rules;
- booleans and null;
- floating-point values, including explicit handling of NaN and negative zero;
- tagged logical values;
- object references.

### Schema descriptor model

Define how a schema declares:

- stable schema identity;
- version;
- schema language;
- fingerprint;
- required interpreter capability;
- compatibility claims;
- dependencies;
- embedded versus external availability;
- migration references;
- logical-type registry use.

### Initial schema support

Implement one simple schema mechanism suitable for core and profile metadata. Other schema languages may be embedded or referenced, but the core must not attempt to unify every schema system.

### Compatibility checker

Support at least:

- field addition and removal rules;
- required versus optional fields;
- type widening or narrowing declarations;
- enum evolution;
- unknown-field preservation expectations;
- explicit incompatible changes;
- schema fingerprint comparison.

### Canonical text projection

Define a lossless text form for:

- all core physical values;
- byte strings;
- tagged values;
- object and content references;
- floating-point edge cases;
- unknown extensions;
- canonical key order;
- comments, if allowed, with clear exclusion from canonical identity.

Implement:

```console
ucof to-text input.ucof -o output.ucof.txt
ucof from-text input.ucof.txt -o output.ucof
ucof diff left.ucof right.ucof
```

The semantic diff should distinguish physical byte differences from logical object-graph differences.

## Required tests

- Canonical map ordering.
- Unicode normalization policy.
- Every floating-point edge case.
- Binary-to-text-to-binary preservation.
- Unknown tagged value preservation.
- Schema upgrade and downgrade cases.
- External schema unavailable.
- Conflicting embedded schema identity.
- Maliciously recursive schema dependencies.

## Exit criteria

- Canonical text round-trips every valid core value without loss.
- Schema compatibility results are deterministic and explainable.
- A basic reader can enumerate schema-bound objects without understanding the schema language.
- External schemas are never fetched implicitly.
- Unknown optional logical types remain preservable.
- Canonicalization has complete test vectors and no dependence on locale or host map ordering.

## Key decisions

- Unicode normalization policy.
- Map key constraints.
- Float canonicalization.
- Initial schema language or minimal descriptor syntax.
- Whether comments are allowed in the diagnostic text form.

---

# Phase 6 — Integrity, signatures, and provenance

## Objective

Provide clear, composable verification semantics without conflating corruption detection, authorship, and edit history.

## Deliverables

### Integrity model

Define:

- stored-byte digest;
- logical-content digest;
- object digest;
- manifest/root digest;
- digest tree or equivalent aggregation;
- algorithm identifiers and deprecation;
- canonical signed scope;
- behavior for unknown or weak algorithms.

### Signature model

Prefer established cryptographic message structures rather than designing novel signature primitives. Define:

- what bytes or canonical structures are signed;
- signer identity representation;
- certificate or key-reference handling;
- countersignatures;
- timestamp claims;
- multiple signers;
- detached verification material;
- signature invalidation after compaction or rewriting;
- status categories: valid, invalid, unverifiable, unsupported, expired, revoked where revocation data is available.

### Provenance model

Represent provenance as append-only claims containing:

- claim type;
- actor or software identity;
- input identities;
- output identities;
- operation description;
- previous claim link;
- timestamp assertion;
- optional signature;
- disclosure and privacy classification.

Readers must not describe an unsigned claim as verified history.

### Verification CLI

Implement output suitable for both humans and automation:

```console
ucof verify file.ucof
ucof verify --json file.ucof
ucof provenance file.ucof
```

Machine output must preserve distinctions among malformed, corrupt, unsigned, untrusted, unverifiable, and verified states.

## Required tests

- Digest algorithm substitution attempt.
- Signature wrapping around a different manifest.
- Valid payload with forged directory digest.
- Valid older signed root followed by unsigned newer root.
- Compaction of signed content.
- Multiple signatures over different scopes.
- Provenance chain with missing intermediate claim.
- Clock or timestamp ambiguity.
- Unknown signature algorithm.

## Exit criteria

- Signed scope is byte-for-byte unambiguous.
- A verifier never treats an unverified index as proof of content.
- Integrity can be checked incrementally for selected objects.
- Provenance UI and APIs distinguish claims from verified facts.
- Compaction and rewriting rules explicitly state which identities and signatures survive.
- Cryptographic agility exists without permitting silent downgrade.

## Key decisions

- Digest tree structure.
- Baseline digest and signature suites.
- COSE adoption details if selected.
- Timestamp and revocation model.
- Whether object-level signatures are profile-specific or core-defined.

---

# Phase 7 — Encryption and selective disclosure

## Objective

Add confidentiality without breaking random access, integrity semantics, or metadata transparency expectations.

## Deliverables

### Encryption model

Define authenticated encryption at object or chunk granularity, including:

- algorithm identifiers;
- nonce construction and uniqueness requirements;
- associated-data scope;
- ciphertext and plaintext length treatment;
- recipient key envelopes;
- multiple recipients;
- key rotation;
- encrypted dictionaries and schemas;
- deterministic encryption prohibition unless a future profile explicitly and safely defines it.

### Metadata modes

Specify at least:

1. Public directory and public metadata, encrypted payload.
2. Public bootstrap with encrypted object metadata and payload.
3. Minimal public envelope with encrypted private directory.

For each mode, document leaked information such as:

- total size;
- chunk count;
- object count;
- object types;
- names;
- relationships;
- update timing;
- compression characteristics;
- access patterns outside the file format's control.

### Selective disclosure

Support separate keys for independently decryptable objects or groups. Define behavior when a reader can access only part of the object graph.

### Key handling API

The library should request keys through a caller-provided resolver. It must not embed platform-specific secret storage into the core crate.

## Required tests

- Nonce reuse detection in writer logic.
- Modified ciphertext and associated data.
- Public directory pointing to swapped ciphertext.
- Partial recipient access.
- Missing key versus corrupt ciphertext error distinction.
- Encrypted schema required to interpret visible data.
- Compaction and re-encryption.
- Metadata-confidential mode recovery.

## Exit criteria

- Every encrypted chunk is authenticated.
- Encryption does not rely on unauthenticated public offsets or identifiers without binding them as associated data.
- Readers can report inaccessible objects without presenting them as corrupt.
- Metadata leakage is explicitly documented for each mode.
- Key rotation and compaction behavior are defined.
- The core library remains independent of any single key-management system.

## Key decisions

- Baseline AEAD suite.
- Recipient envelope standard.
- Associated-data fields.
- Private-directory bootstrap strategy.
- Whether encrypted object names are profile-defined or core-defined.

---

# Phase 8 — Archive and Table profiles

## Objective

Validate the container against two substantially different workloads and produce the first useful interoperable applications.

## 8A. Archive profile

### Required features

- hierarchical paths;
- files, directories, symbolic-link policy, and metadata;
- portable timestamp representation;
- permission and ownership portability rules;
- content deduplication;
- sparse-file representation where supported;
- optional compression by chunk;
- safe extraction rules;
- path normalization and traversal prevention;
- optional signatures and encrypted entries;
- external references only when explicitly allowed.

### CLI scope

```console
ucof pack SOURCE -o archive.ucof
ucof list archive.ucof
ucof unpack archive.ucof -o DESTINATION
ucof cat archive.ucof path/to/file
```

### Archive exit criteria

- Round-trip a cross-platform corpus with documented metadata loss where platforms differ.
- Prevent absolute-path, parent-traversal, device-file, and unsafe-link extraction by default.
- Extract one object without decompressing unrelated objects.
- Demonstrate deduplication of repeated content.
- Recover the last valid checkpoint from an interrupted archive stream.

## 8B. Table profile

### Required features

- schema and schema evolution;
- row groups or equivalent horizontal partitions;
- independently readable column chunks;
- null representation;
- dictionary encoding;
- statistics with privacy and trust warnings;
- optional Bloom filters;
- page or chunk indexes;
- predicate and column projection planning;
- canonical logical types for common timestamps, decimals, and identifiers;
- explicit ordering and collation metadata where relevant.

### CLI scope

```console
ucof table schema data.ucof
ucof table scan data.ucof --columns id,total
ucof table filter data.ucof --where 'total > 100'
ucof table import input.csv -o data.ucof
ucof table export data.ucof -o output.csv
```

The initial query language may remain intentionally limited. It is a validation tool, not a database engine.

### Table exit criteria

- Column projection avoids reading unrelated column payloads.
- Row-group pruning uses validated statistics without treating them as authoritative data.
- Schema evolution test cases produce predictable results.
- Benchmarks compare full scans and selective reads with representative existing formats.
- An independent reader can decode the normative profile fixtures.

## Shared profile deliverables

- Normative profile specifications.
- Profile identifiers and versioning rules.
- Conformance levels.
- Golden and malformed fixtures.
- Performance corpus.
- Conversion tools with documented information loss.

## Phase exit criteria

- Archive and Table each have at least one complete end-to-end workflow.
- Both profiles reuse the same core reader and object model without profile-specific core exceptions.
- Profile constraints reveal no unresolved ambiguity in capability negotiation, schema binding, transforms, or integrity scope.
- The two profiles demonstrate different indexing and access strategies.

## Key decisions

- Minimum required transforms per profile.
- Path and metadata portability rules.
- Table physical encodings and statistics scope.
- Profile version compatibility policy.

---

# Phase 9 — Interoperability, tooling, and hardening

## Objective

Prove that UCOF is a specification rather than merely the serialization behavior of one codebase.

## Deliverables

### Independent implementation

Create or sponsor a second implementation in another language. It does not need every optional feature, but it must independently implement:

- core framing;
- canonical metadata;
- object directory lookup;
- core verification;
- Archive or Table profile fixtures;
- error handling for unknown required capabilities.

A minimal C, Go, or Python parser may be appropriate, provided it is written from the specification rather than translated from reference implementation internals.

### Conformance harness

The harness should:

- run readers against valid and malformed fixtures;
- run writers and compare canonical outputs;
- verify error categories;
- test unknown-feature behavior;
- perform cross-implementation round trips;
- publish machine-readable results.

### CLI completion

Target commands:

- `inspect`;
- `verify`;
- `list`;
- `extract`;
- `pack`;
- `to-text`;
- `from-text`;
- `diff`;
- `repair`;
- `compact`;
- `conformance`.

### Benchmark suite

Measure against realistic corpora:

- sequential read and write throughput;
- first-object and random-object latency;
- metadata-only inspection cost;
- selective column reads;
- archive extraction of one small object from a large file;
- compression ratio;
- index size;
- memory use;
- recovery scan time;
- append and compaction cost.

Publish environment details and avoid presenting one hardware result as universal.

### Security hardening

- Continuous fuzzing.
- Differential fuzzing across implementations.
- Static analysis and dependency review.
- External format and cryptography review.
- Malformed corpus expansion.
- Denial-of-service resource testing.
- Security response process rehearsal.

### Documentation

- Implementer guide.
- Profile author guide.
- Security considerations.
- Migration guide for experimental files.
- Annotated binary examples.
- Registry process.
- FAQ explaining trade-offs and non-goals.

## Exit criteria

- Two implementations agree on every normative valid vector.
- Divergent parsing behavior is resolved or explicitly declared non-conforming.
- No critical or high-severity unresolved security finding remains.
- Benchmarks validate partial-access benefits for the selected profiles.
- The reference CLI can inspect and verify files without profile-specific code.
- The specification can be implemented without reading reference implementation source.
- All normative requirements are testable or have a documented reason they cannot be mechanically tested.

## Key decisions

- Which implementation becomes the second conformance anchor.
- Required versus recommended 1.0 algorithms.
- Performance regressions that block release.
- Supported platforms for the reference implementation.

---

# Phase 10 — Specification freeze and UCOF 1.0

## Objective

Publish a stable core format and initial profiles with a long-term compatibility commitment.

## Freeze prerequisites

The wire format must not be frozen until:

- core framing has survived multiple prototype revisions;
- recovery behavior has been tested through systematic fault injection;
- canonicalization vectors are complete;
- two implementations interoperate;
- Archive and Table profiles are demonstrably useful;
- cryptographic scopes have independent review;
- registry and extension rules are operational;
- the security model has no unresolved critical ambiguity;
- experimental identifier collisions and naming issues have been reviewed;
- migration from experimental versions is documented.

## Deliverables

### Specification set

- UCOF Core 1.0.
- Canonical Metadata and Text 1.0.
- Security and Privacy Considerations 1.0.
- Registry Policy 1.0.
- Archive Profile 1.0.
- Table Profile 1.0.

### Software releases

- Reference library 1.0.
- CLI 1.0.
- Conformance suite 1.0.
- Corpus and test-vector release.
- Second implementation release or compatibility statement.

### Project operations

- Stable website or documentation location.
- Published release signing process.
- Vulnerability reporting and supported-version policy.
- Deprecation process for algorithms and extensions.
- Proposal process for future profiles.
- Decision on formal media type, file extension, and magic-value registration where appropriate.

## 1.0 compatibility promise

The final policy should commit to:

- permanent meaning for core 1.0 framing fields;
- no reassignment of registered identifiers;
- safe skipping of unknown optional and advisory features;
- explicit failure on unknown required features;
- versioned profiles;
- preserved ability to inspect and extract understood objects from future files where no unknown required capability blocks access;
- documented deprecation rather than silent semantic change.

## Exit criteria

- All freeze prerequisites are met.
- Release artifacts are reproducible and signed.
- Normative documents and conformance fixtures agree.
- The compatibility matrix is published.
- No experimental marker remains in 1.0 output.
- A final release-candidate period produces no wire-format-blocking issue.

---

# 6. Cross-cutting workstreams

## 6.1 Specification quality

Every normative requirement should use consistent requirement language and have:

- a rationale;
- at least one valid example where applicable;
- at least one invalid example where applicable;
- a conformance test or an explanation of why testing is manual;
- compatibility and security consequences.

## 6.2 Testing strategy

The full testing pyramid should include:

1. Unit tests for primitive encodings and validation.
2. Property tests for round trips, ordering, truncation, and limits.
3. Golden vectors for normative behavior.
4. Malformed corpus tests for known attack classes.
5. Mutation fuzzing from valid files.
6. Structure-aware generation of valid and invalid files.
7. Differential tests between implementations.
8. Crash and fault injection around append and root publication.
9. Benchmarks and regression thresholds.
10. Real-world corpus conversion tests.

## 6.3 Security review gates

Security review should occur before, not only after, the following are frozen:

- offset and length encoding;
- canonicalization;
- root selection and recovery;
- transform expansion rules;
- signed scope;
- encryption associated data;
- metadata confidentiality modes;
- external-reference behavior;
- archive extraction semantics.

## 6.4 Performance discipline

Performance claims should report:

- corpus characteristics;
- access pattern;
- chunk size;
- compression settings;
- cache state;
- storage medium;
- CPU and memory environment;
- integrity-verification mode;
- whether indexes were preloaded;
- peak memory, not only elapsed time.

Optimization must not weaken validation or make canonical behavior platform-dependent.

## 6.5 Registry management

Registries should define:

- identifier width and encoding;
- allocation authority;
- permanent, provisional, experimental, and private-use ranges;
- naming rules;
- required documentation;
- security considerations;
- deprecation status;
- collision handling;
- whether an identifier implies mandatory support.

## 6.6 Backward-compatibility testing

After each stable pre-release milestone, retain fixtures indefinitely and ensure new readers continue to:

- identify the experimental version correctly;
- either read it as documented or reject it explicitly;
- never misinterpret old bytes as a newer stable format;
- preserve unknown optional data where promised.

# 7. Critical decision backlog

The following decisions should remain visible until resolved by evidence:

1. Exact bootstrap header and footer layout.
2. Fixed-width versus variable-width offsets and lengths.
3. Canonical CBOR subset and any prohibited forms.
4. Object identity and content identity construction.
5. Primary directory structure.
6. Root publication and active-snapshot selection.
7. Digest coverage of stored and logical bytes.
8. Default chunk-size guidance by workload.
9. Schema descriptor mechanism.
10. Unknown-field preservation guarantees.
11. Text projection syntax and Unicode policy.
12. Signature container and signed scope.
13. Provenance privacy and redaction model.
14. Encryption algorithms, key envelopes, and associated data.
15. Private-directory bootstrap.
16. External-reference policy.
17. Archive path and link semantics.
18. Table statistics trust and privacy rules.
19. Core versus profile ownership of object types.
20. Requirements for declaring 1.0 stable.

# 8. Initial issue breakdown

After Phase 0 policies are in place, the implementation work can be split into issues resembling:

1. Draft terminology and object model.
2. Document representative workloads.
3. Publish threat model.
4. Create format proposal template.
5. Evaluate deterministic CBOR constraints.
6. Prototype header, chunk, and footer alternatives.
7. Define reader limits and structured errors.
8. Implement annotated golden-vector generator.
9. Add footer truncation and crash simulator.
10. Prototype primary directory alternatives.
11. Build streaming and seekable readers.
12. Add mutation fuzz targets.
13. Specify append-only root commits.
14. Implement recovery and repair proof of concept.
15. Benchmark chunk-size trade-offs.
16. Specify transform registry and Zstandard adapter.
17. Define schema descriptor and compatibility cases.
18. Specify canonical text projection.
19. Define digest tree and signed scope.
20. Draft Archive profile.
21. Draft Table profile.
22. Build independent parser.
23. Create cross-implementation conformance harness.
24. Commission security and format review.

Issues should remain small enough to review and should link to the proposal or phase exit criterion they advance.

# 9. Definition of done for implementation tasks

A task that changes normative or parser behavior is complete only when it includes, as applicable:

- specification update;
- implementation update;
- valid fixture;
- invalid or boundary fixture;
- unit or property tests;
- limit and error behavior;
- security analysis;
- compatibility impact;
- benchmark impact for performance-sensitive changes;
- user-facing documentation.

Code alone is not sufficient evidence for a format decision.

# 10. Major project risks

| Risk | Consequence | Mitigation |
|---|---|---|
| Mandatory core grows too large | Few independent implementations; poor longevity | Gate every mandatory feature; move domain behavior into profiles |
| Wire format freezes too early | Permanent complexity and compatibility debt | Require multiple prototypes and an independent implementation |
| Canonicalization is ambiguous | Broken hashes, signatures, and reproducibility | Publish exhaustive vectors and differential tests |
| Recovery accepts attacker-selected roots | Rollback or substitution attacks | Define root ordering and signed scope; test stale and corrupt roots |
| Metadata leaks despite encryption | False confidentiality expectations | Publish explicit metadata modes and leakage tables |
| Extension system becomes executable | Parser compromise and ecosystem fragmentation | Declarative registries only; no automatic embedded-code execution |
| Indexes become trusted truth | Data omission or substitution | Validate indexes; keep them rebuildable |
| Profiles require core exceptions | Loss of architectural coherence | Treat repeated exceptions as evidence the core model is wrong |
| Reference implementation defines accidental behavior | Other implementations cannot interoperate | Normative vectors and independent implementation before 1.0 |
| Project claims universality without proof | Misleading adoption and poor design choices | Benchmark defined workloads and document non-goals |
| Provenance is mistaken for truth | Misleading trust decisions | Separate asserted, signed, verified, incomplete, and absent history |
| Codec dependencies become permanent liabilities | Security and maintenance burden | Keep codecs optional, versioned, bounded, and registry-controlled |

# 11. Recommended immediate next actions

The next repository changes should implement Phase 0 in this order:

1. Select a permissive license before external contributions begin.
2. Add contribution, security, and conduct policies.
3. Add proposal and architecture-decision templates.
4. Draft the terminology document.
5. Draft the workload and use-case document.
6. Draft the initial threat model.
7. Create the first format proposal comparing header, chunk, directory, and footer alternatives.
8. Only then initialize the minimal Rust workspace and Phase 1 prototype.

This ordering prevents the first code sketch from silently becoming the specification.
