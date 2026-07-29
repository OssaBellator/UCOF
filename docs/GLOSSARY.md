# UCOF Glossary

## 1. Purpose and usage

This glossary establishes the project’s working vocabulary before the binary format is defined. Terms may be refined through proposals, but documents should not use them inconsistently without explicitly introducing a narrower profile-specific meaning.

The words **must**, **must not**, **required**, **should**, **should not**, and **may** become normative only inside an identified normative specification. Their use in this glossary describes intended distinctions rather than freezing wire behavior.

## 2. Core storage terms

### File

A finite byte sequence intended to contain one UCOF container instance.

A file may contain multiple historical snapshots and uncommitted trailing bytes. “File” refers to the physical byte sequence, not merely the active logical state.

### File instance

A particular evolving container lineage, distinguished from copied or independently created files by an instance identity when such an identity is available.

A byte-for-byte copy may preserve the same file instance identity even though it exists at another location.

### Snapshot

A committed logical view of the objects and relationships that constitute the container at a particular point in its append-only history.

A snapshot is selected through a validated root or commit structure. Uncommitted trailing chunks are not part of the active snapshot.

### Manifest

An authoritative object that describes a snapshot’s logical root, required capabilities, profile declarations, object-set roots, and other snapshot-level metadata.

The manifest is not necessarily the object directory. It may reference one or more directories and indexes.

### Root

The validated entry point from which a snapshot’s authoritative structures can be discovered and verified.

A root may include or reference the manifest, integrity information, and a link to a previous root. The precise serialized form is not yet defined.

### Object

A logical entity represented in the container, such as a manifest, schema, directory, table, image, document node, signature, or application payload.

An object may occupy one chunk, span several chunks, or be represented through a profile-defined structure. Object boundaries are logical; chunk boundaries are physical.

### Chunk

An independently bounded physical storage unit carrying all or part of an object, together with enough framing information to locate, skip, and validate the stored region safely.

A chunk does not necessarily correspond one-to-one with an object.

### Payload

The content bytes or logical values carried by a chunk or object after excluding the enclosing framing metadata.

“Stored payload” refers to bytes before inverse transforms. “Logical payload” refers to the result after required transforms are successfully reversed.

### Bootstrap header

A small structure near the beginning of a file that allows a reader to identify UCOF, reject unsupported experimental epochs or major versions, and begin safe discovery.

The bootstrap header is expected to remain smaller and more stable than profile metadata.

### Footer

A bounded trailing structure used to locate and authenticate a candidate active root or checkpoint.

A footer is a discovery mechanism, not automatically authoritative merely because it appears last in the byte sequence.

### Checkpoint

A recoverable, validated publication point that allows a reader to identify a complete earlier state after truncation or interrupted writing.

A checkpoint may be a full snapshot root or a profile-defined intermediate recovery point. The specification must distinguish these cases.

### Compaction

The process of producing a new file that preserves selected logical state while removing unreachable objects, superseded snapshots, redundant chunks, or inefficient physical layout.

Compaction changes physical byte identity and may change instance identity. It must not be described as a lossless history-preserving operation unless historical state is explicitly retained.

## 3. Discovery and access terms

### Directory

The primary mapping needed to discover objects or chunks by identifier, logical range, or other core locator without scanning unrelated payload data.

A directory is part of the container’s discoverability model. Whether it is authoritative or reconstructible must be specified explicitly.

### Index

An auxiliary data structure that accelerates a query or access pattern, such as key lookup, range filtering, spatial search, time seek, or negative membership testing.

Indexes are not authoritative unless a profile explicitly defines otherwise. Non-authoritative indexes must be validated or safely ignored when inconsistent.

### Primary directory

The core object-location structure required for random-access files.

It is distinct from optional workload-specific secondary indexes.

### Secondary index

An optional or profile-required accelerator derived from authoritative data, such as a B+tree, sorted block index, page index, Bloom filter, spatial index, or temporal index.

### Random access

The ability to locate and read selected data using bounded reads without sequentially decoding all preceding payloads.

### Sequential access

Processing a file in byte order without seeking backward or requiring the final file length in advance.

### Partial read

A read operation that retrieves only the metadata, objects, chunks, columns, ranges, or other subset needed for a request.

A partial read may still require integrity or dependency data outside the target payload.

## 4. Identity and integrity terms

### Object identifier

An identifier used to refer to a logical object within a defined scope.

An object identifier is not necessarily derived from content and must not be assumed globally unique unless its identifier scheme says so.

### Instance identity

An identity that distinguishes a mutable object, file lineage, or snapshot instance even when its content changes.

Instance identity supports references to evolving entities but does not by itself prove byte equality.

### Content identity

An identity derived from a defined canonical representation and digest algorithm so that equal canonical content yields the same identity.

Content identity depends on precise canonicalization and algorithm tagging. It may be unavailable, intentionally hidden, or scoped differently for encrypted, mutable, or non-canonical content.

### Digest

The output of an identified cryptographic or non-cryptographic hash function applied to a precisely defined byte sequence or canonical structure.

A digest detects mismatch relative to an expected value. It does not authenticate a creator by itself.

### Checksum

A generally non-authenticating integrity code intended mainly to detect accidental corruption.

The term should not be used interchangeably with a cryptographic digest when adversarial substitution matters.

### Integrity

Confidence that bytes or logical content match an expected identity or relation.

Integrity does not necessarily establish who produced the content or whether the content is trustworthy.

### Signature

A cryptographic value that binds a signer-controlled key to a precisely defined message or claim.

A valid signature proves possession or use of the corresponding signing key under the selected algorithm. Trust in the signer requires an external or embedded trust policy.

### Signed scope

The exact bytes, canonical values, identifiers, context, and algorithm declarations covered by a signature.

Ambiguous signed scope is a security defect.

### Provenance claim

A statement about origin, authorship, transformation, custody, time, software, or relationships among objects.

A provenance claim may be signed or unsigned. Even a validly signed claim is an assertion by the signer, not proof that the asserted event occurred.

### Trust policy

Application or deployment rules that decide which keys, identities, issuers, algorithms, timestamps, and claims are accepted for a purpose.

The core format can carry evidence but cannot define one universal trust policy.

## 5. Schema and type terms

### Schema

A versioned description of permitted structure, fields, constraints, and semantics for one or more objects or metadata values.

A schema may be embedded or referenced. The format may support multiple schema languages.

### Physical type

A low-level representation understood by the core or a selected encoding, such as an integer, byte string, text string, array, map, fixed-width vector, or primitive column encoding.

Physical types describe representation, not complete domain meaning.

### Logical type

A semantic interpretation layered over one or more physical types, such as timestamp, decimal currency, UUID, geographic coordinate, duration, or tensor dimension.

Logical types require explicit identifiers, constraints, and compatibility behavior.

### Field

A named or numbered component of a structured value as defined by a schema or core metadata model.

### Unknown field

A field recognized as structurally present but not semantically understood by a particular reader.

Whether it can be preserved, ignored, or causes rejection depends on the schema and capability rules.

### Schema evolution

A controlled change to a schema and its compatibility rules across versions.

Schema evolution is distinct from changing the core container version.

### Canonical representation

The unique byte representation selected for a value within a defined scope.

Canonical representation exists to support deterministic output, signatures, content identity, deduplication, and reproducible fixtures. It must specify all relevant ordering, encoding, normalization, and edge cases.

### Diagnostic text projection

A deterministic textual representation of UCOF core structures intended for inspection, review, fixtures, and tooling.

It is derived from and convertible to the canonical binary model within its supported scope. It is not an independent competing truth stored implicitly beside the binary form.

## 6. Transform and encoding terms

### Encoding

A mapping between a logical value and a stored representation.

An encoding may be part of a schema, profile, or transform pipeline.

### Transform

An identified, ordered operation applied to payload bytes or values, such as compression, encryption, delta encoding, or a profile-defined packing step.

Transforms must declare enough information for safe reversal, support detection, and resource accounting.

### Transform pipeline

The ordered sequence of transforms applied to stored content.

Order is semantically significant. Readers must not guess transform order.

### Codec

An implementation of an encoding or reversible transform.

A file may name an encoding or transform identifier; it must not assume that embedding arbitrary executable codec code is safe or portable.

### Compression dictionary

Shared data used by a compression transform to improve ratio or speed.

Dictionaries are dependencies subject to identity, availability, size, expansion, and cycle limits.

### Logical length

The size of content after required inverse transforms, measured in the unit defined by the relevant encoding.

### Stored length

The number of bytes occupied by the stored region in the file.

Stored and logical lengths must be distinguished to prevent allocation and expansion errors.

## 7. Extensibility terms

### Capability

A named semantic feature that a file, object, profile, reader, or writer may require or support.

Capabilities allow compatibility decisions more precise than version comparison alone.

### Required capability

A capability whose absence makes safe or correct interpretation impossible. A reader that does not support it must fail closed for the affected scope.

### Optional capability

A capability that may be skipped while preserving safe interpretation of the surrounding scope according to defined fallback rules.

“Optional” does not mean that a writer may mislabel semantically required behavior.

### Advisory capability

A capability that improves presentation, optimization, or diagnostics but does not affect the authoritative meaning of the content.

### Profile

A versioned interoperability contract that constrains the core container for a domain or workload.

A profile selects required object types, schemas, transforms, indexes, access patterns, and conformance behavior. It should not redefine core terms incompatibly.

### Extension

A feature outside the mandatory core introduced through an allocated identifier, profile, schema, transform, or capability.

### Registry

A controlled mapping from stable identifiers to definitions, ownership, status, and compatibility requirements.

### Permanent identifier

An identifier allocated through the registry process for long-lived interoperable use.

Permanent identifiers are never assigned merely because a prototype used a number.

### Experimental identifier

A non-permanent identifier reserved for testing and subject to change or collision outside its declared scope.

## 8. Validation and conformance terms

### Valid

Conforming to all applicable structural and semantic requirements for the claimed core version, profile, capabilities, and scope.

### Well-formed

Satisfying low-level structural rules sufficiently for parsing, without necessarily satisfying all semantic or profile requirements.

A well-formed file may still be invalid.

### Conforming reader

An implementation that satisfies the required reader behavior for a specified core version, profile, capability set, and resource-limit policy.

### Conforming writer

An implementation that emits only valid files for its claimed core version, profile, and capability set and follows required canonicalization and publication rules.

### Strict mode

Validation behavior that rejects violations of applicable normative requirements and does not silently repair or reinterpret malformed content.

### Diagnostic mode

Behavior that reports additional structure or damage information while preserving a clear distinction between valid, unsupported, malformed, and recoverable content.

Diagnostic mode must not upgrade damaged content to valid content implicitly.

### Salvage

Best-effort extraction of trustworthy or useful data from a damaged or incomplete file.

Salvaged output must identify lost guarantees and must not be represented as the original valid snapshot.

### Conformance vector

A fixture with defined expected parsing, validation, canonicalization, identity, or failure behavior used to compare implementations.

### Parser differential

A disagreement between implementations or modes about the structure, validity, identity, or meaning of the same bytes.

Security-relevant differentials must be treated as format or implementation defects, not merely test noise.

## 9. Security and privacy terms

### Trust boundary

A point where data, identities, keys, software, or decisions cross between parties or components with different trust assumptions.

### Resource limit

A caller- or implementation-defined bound on work such as bytes read, allocation, nesting, recursion, object count, dependency depth, transform expansion, or diagnostics.

Resource-limit enforcement is part of safe conformance.

### External reference

A reference to data not embedded in the current file.

External references require explicit retrieval policy, identity verification, privacy analysis, and failure behavior. Parsing alone must not trigger network access.

### Metadata confidentiality

Protection of names, types, lengths, relationships, indexes, statistics, and other descriptive information, independently of payload confidentiality.

Encrypted payloads do not imply confidential metadata.

### Algorithm confusion

A security failure where an identifier, key, digest, signature, or ciphertext is interpreted under the wrong algorithm or parameter set.

### Signature wrapping

A security failure where a valid signature over one object or scope is presented as if it authenticated another object or a broader scope.

## 10. Terms to avoid or qualify

### Universal

Use to describe the container’s intended breadth, not a claim that one physical layout is optimal for every workload.

### Self-describing

Use to mean that schemas, identifiers, and interpretation metadata can travel with the file. It does not mean a reader can understand arbitrary semantics without implementation support.

### Secure

Do not use without identifying the threat, trust policy, supported algorithms, resource limits, and scope.

### Verified

State what was verified: bytes against a digest, a signature against a key, a certificate path, a snapshot root, or a provenance claim. “Verified file” is usually too broad.

### Lossless

State what is preserved: logical objects, byte identity, metadata, unknown fields, history, signatures, or physical layout. These are different guarantees.
