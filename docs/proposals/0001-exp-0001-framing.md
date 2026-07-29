# FCP-0001: UCOF-EXP-0001 minimal framing experiment

- **Status:** Review
- **Created:** 2026-07-29
- **Authors:** UCOF maintainers
- **Target:** Experimental epoch `UCOF-EXP-0001`
- **Specification:** `spec/experimental/UCOF-EXP-0001.md`
- **Implementation:** `crates/ucof-core`, `crates/ucof-cli`
- **Registry allocations:** None; all identifiers are experimental
- **Supersedes:** None
- **Superseded by:** None

## 1. Summary

Define a disposable, end-to-end UCOF wire experiment containing:

- a fixed bootstrap header;
- sequentially framed records;
- opaque objects;
- one canonical metadata manifest;
- a canonical metadata primary directory;
- a trailing footer locating the directory and active manifest;
- SHA-256 integrity over the committed prefix;
- exact-end footer discovery;
- explicit hostile-input limits;
- no transforms, encryption, signatures, mutation, external references, or profiles.

The experiment exists to test assumptions. It is not a candidate for durable storage and does not allocate permanent wire identifiers.

## 2. Motivation and problem statement

UCOF needs evidence that a small generic reader can identify an epoch, enumerate unknown objects, locate a manifest and directory, verify committed bytes, reject malformed ranges, and provide random access without understanding a domain profile.

A prose architecture is insufficient. The first experiment must expose concrete costs and ambiguities before more features are added.

Relevant use cases include:

- UC-01 small archive inspection;
- UC-02 large object inventory and direct lookup;
- UC-04 interrupted sequential capture, limited in this epoch to truncation detection;
- UC-07 damaged file with a previously valid root, reserved for later append experiments;
- UC-08 core-only reader encountering unknown object semantics;
- UC-10 malicious resource-exhaustion input.

Relevant threats include offset overflow, forged lengths, overlap, excessive metadata nesting, malicious directories, digest mismatch, parser differentials, and ambiguous root selection.

## 3. Scope

This proposal specifies only the experimental bootstrap envelope and its minimum metadata structures.

### Included

- little-endian fixed-width framing integers;
- a 32-byte file header;
- a 40-byte record header;
- opaque, manifest, and directory record kinds;
- unsigned 64-bit object identifiers;
- a restricted deterministic CBOR metadata language;
- a directory that describes all records before the directory;
- an 80-byte footer at the exact end of the file;
- SHA-256 covering all bytes before the footer;
- strict validation and categorized errors;
- checked-in valid and invalid hexadecimal vectors.

### Non-goals

- stable compatibility;
- append-only updates or historical roots;
- streaming publication before the final footer;
- compression or transform pipelines;
- encryption, signatures, trust, or provenance;
- external object references;
- schemas or profiles;
- media-type registration;
- permanent registry identifiers;
- optimized table or database indexes;
- salvage or repair semantics.

## 4. Terminology

This proposal uses the Phase 0 glossary. In this experiment:

- a **record** is the physical framing unit called a chunk in the broader architecture;
- an **opaque object record** carries bytes whose semantics are unknown to the core;
- the **manifest** identifies root objects and capability declarations;
- the **directory** maps object identifiers to record locations;
- the **committed prefix** is every byte before the final footer;
- the **active manifest** is the unique manifest identified by the footer.

The term `record` is experimental and may be replaced by `chunk` in a later epoch.

## 5. Detailed decision

### 5.1 Byte order

All fixed-width framing integers use little-endian byte order. Canonical CBOR integer arguments use network byte order as required by CBOR.

### 5.2 Header

The file starts with a fixed 32-byte header containing magic bytes, epoch number, flags, header length, and zeroed reserved bytes.

### 5.3 Records

Records follow immediately after the header and are contiguous. Each record has a 40-byte fixed header and an exact payload length. No padding is permitted.

Record kinds are experimental:

- `1`: opaque object;
- `2`: manifest;
- `3`: primary directory.

Object identifier zero is reserved for structural records. Opaque and manifest records must use unique non-zero identifiers. The directory uses identifier zero and must be the final record before the footer.

### 5.4 Manifest

The manifest payload is a deterministic CBOR map with exactly these keys:

- `roots`: array of non-zero object identifiers;
- `required`: array of experimental capability identifiers;
- `optional`: array of experimental capability identifiers.

Readers for this epoch support no required capabilities. A non-empty `required` array therefore causes an unsupported-required-capability error. Unknown optional identifiers are retained as manifest values but do not change structural interpretation.

### 5.5 Directory

The directory payload is a deterministic CBOR map containing one `entries` array. Each entry describes one preceding non-directory record using:

- `id`;
- `kind`;
- `offset`;
- `stored_len`;
- `logical_len`.

The directory is an accelerator, not independent authority. A strict reader scans the framing sequence and requires the directory to match the actual records exactly.

### 5.6 Footer

The footer is exactly 80 bytes and must occupy the final 80 bytes of the file. It contains:

- footer magic and length;
- flags;
- directory offset and total record length;
- active manifest object identifier;
- total record count including the directory;
- SHA-256 of the complete committed prefix.

Trailing bytes are rejected in this epoch. Searching backward through arbitrary trailing data is deferred until append and recovery experiments establish safe selection rules.

### 5.7 Integrity scope

SHA-256 covers every byte from offset zero through the final byte of the directory record. The footer itself is not covered by that digest.

The digest detects accidental or malicious modification relative to the stored footer, but this experiment provides no authenticity. An attacker able to replace the file can replace both bytes and footer digest.

### 5.8 Canonical metadata subset

The experiment permits only:

- unsigned integers;
- byte strings;
- UTF-8 text strings;
- definite-length arrays;
- definite-length maps;
- `false`, `true`, and `null`.

Negative integers, tags, floating-point values, undefined, other simple values, and indefinite-length items are rejected.

Map keys are ordered by the length of their deterministic encoded bytes and then lexicographically by those bytes. Duplicate or non-increasing keys are rejected. Integer and length arguments must use the shortest permitted representation.

### 5.9 Error behavior

Strict validation distinguishes at least:

- truncated input;
- invalid magic;
- unsupported epoch;
- invalid or non-zero reserved fields;
- unsupported flags;
- range overflow or out-of-bounds range;
- duplicate object identifier;
- invalid record order;
- non-canonical metadata;
- directory mismatch;
- missing or invalid manifest;
- unknown required capability;
- digest mismatch;
- configured resource limit exceeded.

No strict error may be silently downgraded to a warning.

## 6. Compatibility impact

### Existing valid files read by new readers

No stable UCOF files exist. Readers may support this epoch explicitly or reject it.

### New files read by old readers

Pre-proposal readers have no compatibility promise. Epoch mismatch must be explicit.

### Unknown required capabilities

The reader fails closed and reports the first unsupported required identifier.

### Unknown optional data

Unknown optional capability identifiers remain parseable. This epoch does not define round-trip editing of arbitrary unknown manifest fields; the manifest schema is exact.

### Profiles

No profiles exist in this epoch.

### Canonical identity and signatures

The SHA-256 prefix digest is not yet a permanent UCOF content identity and is not a signature scope.

### Experimental epoch changes

Any incompatible change creates a new `UCOF-EXP-####` epoch. Files from retired epochs may become unsupported.

### Stable versions

Not applicable.

### Migration and coexistence

The CLI may inspect or reject files by epoch. No automatic migration is promised.

## 7. Security and privacy impact

Attacker-controlled values include all framing fields, lengths, identifiers, metadata values, offsets, counts, and digest bytes.

The parser must use checked arithmetic before slicing or allocation. It must enforce caller-provided limits for file size, record count, payload size, metadata depth, metadata item count, text and byte-string length, and total metadata bytes.

The directory is verified against framing and cannot cause the parser to treat unverified ranges as objects. The footer cannot select a directory outside the committed prefix.

Metadata is public. The experiment provides no confidentiality and leaks object count, kinds, sizes, locations, roots, and capability declarations.

The format never executes embedded bytes and never performs network retrieval.

## 8. Resource-limit impact

The reference reader exposes limits rather than relying only on implementation defaults. Default limits are conservative for tests, not normative ceilings.

At minimum, limits cover:

- maximum total file bytes;
- maximum records;
- maximum single payload bytes;
- maximum metadata bytes;
- maximum metadata nesting depth;
- maximum array or map entries;
- maximum text or byte-string bytes.

Limit exhaustion is a distinct error category.

## 9. Streaming and random-access impact

Records can be scanned sequentially from the header. Full validation requires the final footer and directory.

Random access uses directory offsets only after the directory has been validated against framing. The experiment does not yet support a reader that validates a completed stream without retaining or seeking to earlier bytes; that API and checkpoint design are deferred.

## 10. Recovery, truncation, and compaction impact

A missing or partial footer makes the file invalid. No earlier-root recovery exists because this epoch contains one footer and one active root.

Truncation at any point must produce a deterministic structural error rather than selecting an earlier accidental byte pattern.

Compaction is not applicable because no historical snapshots exist.

## 11. Alternatives considered

### Variable-width framing integers

Rejected for the first experiment because they complicate random field access and canonical validation before their space benefit is measured. A later experiment should benchmark fixed and variable framing separately.

### Big-endian framing

Rejected provisionally because the reference environment and likely implementations commonly use little-endian arithmetic. CBOR remains big-endian internally, which ensures the experiment still exercises mixed encoding boundaries.

### Footer search through trailing bytes

Deferred because backward searching creates false-positive and attacker-selected-root questions. Exact-end discovery is the safer baseline.

### Trusting the directory without a framing scan

Rejected because a malicious accelerator must not become authority. Later modes may permit authenticated directory-first reads with different verification guarantees.

### JSON manifest and directory

Rejected for the binary experiment because it introduces number, duplicate-key, Unicode, and canonicalization complexity while inflating offsets. JSON remains useful as diagnostic output.

### Full CBOR

Deferred. Tags, negative integers, floats, indefinite forms, and broad simple values are unnecessary for the first structural proof and increase differential-parser risk.

### Custom metadata TLV

Not selected because a deterministic CBOR subset tests reuse of an established data model while remaining implementable in a small parser.

### CRC32 instead of SHA-256

Rejected because Phase 1 needs to exercise algorithm-sized digests and integrity failure paths closer to later cryptographic identities. SHA-256 here still provides no authenticity.

## 12. Unresolved questions

- Whether the stable framing should use fixed or variable-width lengths.
- Whether a stable footer is exact-end, backward-searchable, or linked through checkpoints.
- Whether the stable format uses deterministic CBOR, another canonical metadata system, or fixed schemas for bootstrap structures.
- Whether object identifiers remain 64-bit integers.
- Whether the primary directory should include itself.
- Whether record and chunk are distinct stable terms.
- Whether stable integrity uses SHA-256, BLAKE3, another algorithm, or an algorithm registry from the first version.
- How a truly streaming reader validates a root with bounded buffering.

## 13. Implementation plan

1. Add the minimal Rust workspace.
2. Implement checked framing encode/decode.
3. Implement the restricted deterministic CBOR codec.
4. Implement deterministic manifest and directory construction.
5. Implement strict full-file validation and object lookup.
6. Add inspect, verify, and demo CLI commands.
7. Add a separate Python vector generator.
8. Check in annotated hexadecimal vectors and expected outcomes.
9. Add CI formatting, lint, and test jobs.
10. Run truncation, mutation, and differential experiments before accepting the FCP.

## 14. Required evidence before acceptance

- the reference workspace builds and tests on stable Rust;
- valid vectors round-trip deterministically;
- the Python generator reproduces the same valid bytes;
- invalid vectors map to stable error categories;
- every byte-boundary truncation of the valid vectors fails safely;
- directory lookup does not scan unrelated payload bytes after validation;
- unknown opaque kinds can be inventoried without profile code;
- unknown required capabilities fail closed;
- memory and work limits are tested;
- at least one second parser reproduces structure before any stable descendant is frozen.

## 15. Registry allocations requested

None. Values in this proposal belong only to `UCOF-EXP-0001`.

## 16. Migration and rollout

The implementation and CLI identify the experimental epoch visibly. Generated files carry no stability promise. A later epoch may provide an explicit converter, but no converter is required before retiring this experiment.

## 17. Rejection or rollback strategy

If the experiment reveals unacceptable ambiguity or complexity:

- mark FCP-0001 Rejected or Superseded;
- keep vectors and findings as evidence;
- retire reader support after a documented transition;
- allocate a new experimental epoch for the replacement;
- never reinterpret existing `UCOF-EXP-0001` bytes under new semantics.

## 18. References

- RFC 8949, Concise Binary Object Representation (CBOR)
- FIPS 180-4, Secure Hash Standard
- UCOF Phase 0 glossary, use cases, and threat model
