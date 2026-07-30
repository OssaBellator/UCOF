# FCP-0002: Paged Directories, Snapshots, and Recovery

- **Status:** Draft
- **Authors:** UCOF maintainers and contributors
- **Created:** 2026-07-30
- **Last updated:** 2026-07-30
- **Target:** Core experimental epoch
- **Experimental epoch impact:** Defines disposable `UCOF-EXP-0002`
- **Related ADRs:** ADR-0009, ADR-0010, ADR-0011, ADR-0012
- **Supersedes:** None
- **Superseded by:** None

## Summary

FCP-0002 proposes the first experimental UCOF layout with:

- an authenticated paged primary directory;
- append-only complete snapshots;
- exact-end commit publication;
- parent snapshot and previous-footer relationships;
- bounded strict validation and separately requested recovery;
- authenticated lookup and absence results;
- valid-root enumeration and explicit fork ambiguity;
- repair-to-new-file and caller-directed rewrite output.

The proposal requires a new disposable epoch because directory, snapshot, active-root, recovery, and identity semantics determine file validity and are incompatible with `UCOF-EXP-0001`.

Candidate 1 now has exact experimental bytes, deterministic Rust and independent Python implementations, pinned valid and invalid vectors, slice and range-source readers, rewrite output, adversarial tests, portability checks, and continuous fuzzing. This is evidence for the Draft. It does not stabilize the bytes, accept this FCP, or allocate permanent registry values.

The independently implementable byte draft is `docs/spec/EXP_0002_BYTE_CANDIDATE.md`.

## Motivation

EXP-0001 proved that a small framing layer can be validated safely, but its flat fully materialized directory cannot satisfy large-archive workloads. One million zero-byte objects already impose approximately 40 MB of record headers and 52 MB of directory payload before application data.

Phase 3 also needs semantics absent from EXP-0001:

- publish a new root without destroying an earlier complete root;
- distinguish strict active-state validation from recovery scanning;
- authenticate one object without loading every directory entry or unrelated payload;
- enumerate plausible historical roots without silently resolving forks;
- repair or rewrite into a new valid output without modifying damaged input in place;
- preserve explicit assurance and resource boundaries under hostile input.

The principal use cases are UC-02 large archives, UC-04 interrupted append capture, UC-07 damaged files with an earlier valid checkpoint, UC-08 unsupported-profile core readers, and UC-10 malicious resource-exhaustion inputs.

## Scope

FCP-0002 defines or constrains:

1. `UCOF-EXP-0002` file, object, page, snapshot, and footer relationships;
2. the first authenticated paged primary directory;
3. deterministic object, page, snapshot, and commit digest scopes;
4. exact-end strict active-root selection;
5. separately bounded recovery candidate discovery and previous-footer traversal;
6. parent-chain and sequence validation;
7. authenticated lookup and absence results;
8. valid-root enumeration and fork ambiguity;
9. repair and caller-directed rewrite publication rules;
10. required cross-language, invalid-vector, hostile-input, fuzz, portability, and scale evidence.

## Non-goals

Candidate 1 does not define:

- stable Core 1.0 bytes;
- permanent registry allocations;
- transforms, compression, schemas, encryption, signatures, provenance, or external references;
- semantic dependency discovery for arbitrary objects;
- distributed concurrency or multi-writer conflict resolution;
- authenticity, trusted freshness, or protection against replaying an older valid whole file;
- profile-specific secondary indexes;
- a separate weaker progress-checkpoint record.

## Terminology

- **Complete snapshot:** a snapshot whose directory, roots, referenced objects, and publishing footer satisfy every required Candidate 1 check.
- **Commit footer:** the exact-end structure publishing one complete snapshot.
- **Strict validation:** exact-end validation with no recovery fallback.
- **Targeted authenticated lookup:** validation of the active commit, active snapshot, one directory path, and the selected object or absence result.
- **Recovery candidate:** a prefix ending in a possible footer that must pass full strict validation before it can be reported as verified.
- **Structural snapshot identity:** the snapshot digest defined by ADR-0011.
- **File-instance commit identity:** the commit digest defined by ADR-0011.
- **Candidate 1:** the first disposable exact-byte realization of `UCOF-EXP-0002`.

## Candidate 1 byte choices

Candidate 1 currently selects:

- little-endian fixed-width integers;
- a 64-byte bootstrap header;
- a 48-byte object header followed by opaque payload bytes;
- fixed 16 KiB authenticated directory pages;
- 88-byte leaf entries;
- 64-byte internal entries;
- a 160-byte snapshot header followed by packed canonical `u64` arrays;
- a 160-byte exact-end commit footer;
- SHA-256 algorithm identifier `1`;
- separate object, page, snapshot, and commit domain prefixes;
- `u64::MAX` as the absent-offset sentinel;
- zero flags, reserved bytes, and unused page padding.

These are experimental selections, not permanent assignments. Page size, leaf layout, and identifier width remain under comparison.

## Object records

Object records require:

- non-zero kind and identifier;
- unique identifiers in one snapshot directory;
- payload and logical length equality in Candidate 1;
- zero flags and reserved bytes;
- exact record-length agreement;
- an object digest in the authenticating leaf entry.

The digest covers the exact object header and payload under the object domain.

## Paged primary directory

Leaf pages contain sorted unique object identifiers and physical locators. Internal pages contain sorted non-overlapping child ranges, exact child levels, page locators, and child-page digests.

A conforming Candidate 1 reader validates:

- page magic, kind, level, entry size, count, range, sequence, and zero padding;
- strict leaf ordering and non-overlapping child ranges;
- child level, range, length, and digest agreement;
- exact 16 KiB page length;
- page digest over all page bytes, including padding;
- invalid references, repeated offsets, and cycles;
- leaf locator agreement with object headers;
- required physical non-overlap among objects, pages, snapshot, and footer.

The current byte writer rebuilds all directory pages. A persistent COW model proves that one locator replacement can copy one page per level and preserve old roots. At 100 million objects with 16 KiB pages, a four-page path is 64 KiB versus approximately 8.28 GiB for a full rebuild. This model is evidence, not yet a byte-level append writer.

Checkpoint-cadence evidence shows that naive per-object path copying is not universally efficient. Frequent checkpoints benefit from reuse, while sparse checkpoints require batched ancestor sharing or may be cheaper as one final rebuild. A production algorithm must emit only final reachable pages for a batch and bound changed entries, page copies, splits, retained history, and output bytes.

## Snapshot and commit publication

A snapshot authenticates:

- sequence number;
- parent snapshot digest;
- previous-footer offset;
- directory-root offset, level, and digest;
- canonical roots;
- canonical required and optional capability arrays.

Genesis uses sequence zero, a zero parent digest, and the absent previous-footer sentinel. A child sequence is exactly its parent sequence plus one.

A complete 160-byte footer ending at exact file end publishes the active strict snapshot. The commit digest covers exact current-commit bytes and footer semantics under the commit domain. Genesis includes the bootstrap header; append commits begin immediately after the previous footer.

Bytes appended before a complete new footer are unpublished tail state. Strict validation fails at the new end. Explicit recovery may still find and strictly validate an earlier complete prefix.

## Checkpoints

ADR-0012 defines Candidate 1 checkpoints as ordinary complete snapshot commits. Candidate 1 does not introduce weaker progress-checkpoint bytes.

Applications and profiles choose checkpoint cadence according to acceptable unpublished work, storage latency, and metadata-write cost. The format defines publication validity, not one universal cadence.

## Historical references and assurance levels

A later snapshot may reference historical object records. The current commit digest covers only current-commit bytes; each leaf entry authenticates its referenced object record individually.

Consequently:

- full strict validation rehashes every referenced object, including historical objects;
- targeted lookup rehashes only the selected object after authenticating the active commit, snapshot, and one directory path;
- unrelated historical payloads need not be read for targeted lookup;
- APIs and tools must state their assurance level.

Candidate 1 has synchronous bounded seekable-source implementations for targeted lookup and full strict validation. Both stream hashes and enforce read-operation, read-byte, request-size, page, object, and hash budgets.

## Identity scopes

ADR-0011 separates:

- snapshot digest: exact authenticated snapshot structure;
- commit digest: one file-instance publication.

A deterministic genesis repair can preserve structural snapshot identity while changing commit identity because the new bootstrap header changes the genesis commit preimage. This is not preservation of the original file instance.

Repair and rewrite reports expose both scopes and never claim preservation of byte-scoped signatures.

## Strict validation

Strict validation must:

1. apply file and work limits before unsafe allocation or traversal;
2. require one exact-end footer;
3. validate footer structure and ranges;
4. authenticate and parse the referenced snapshot;
5. cross-check sequence, parent, and previous-footer fields;
6. authenticate and traverse the complete directory under explicit limits;
7. validate directory canonicality and physical claims;
8. validate roots and capability arrays;
9. validate every referenced object header, range, and digest;
10. validate the current commit digest and footer semantics;
11. return a verified result only after all required checks succeed.

The Rust implementation provides both slice and bounded seekable-source full validation. The source implementation is continuously checked against all three valid vectors and all thirteen pinned invalid vectors.

Strict mode never invokes backward scanning.

## Authenticated lookup

The narrower lookup assurance level verifies:

- bootstrap header;
- exact-end footer and current commit digest;
- active snapshot and parent link;
- one authenticated root-to-leaf path;
- the selected object header, range, and digest.

It may return authenticated absence. It does not claim unrelated historical objects were rehashed.

The range-source implementation proves a small historical-root lookup can avoid reading an unrelated one-megabyte payload.

## Root enumeration and forks

Enumeration and active-root selection are separate operations. A bounded enumerator classifies verified terminals, verified ancestors, fork terminals, complete checkpoints, integrity failures, unsupported capabilities, truncations, and chain failures.

Equal verified forks are ambiguous. A default reader must not silently select one.

## Recovery

Recovery is explicit and independently limits:

- suffix bytes scanned;
- scan read operations and maximum request size;
- footer-magic matches;
- candidate validations;
- cumulative bytes read across successful and failed candidates;
- previous-chain depth;
- returned results and diagnostics.

Every reported candidate is validated as an exact-end prefix by the full source validator. Footer magic and previous-footer pointers are discovery aids, not authority.

The pinned interrupted-append corpus covers cuts after an object header, before snapshot completion, and within a footer prefix. Both Python and Rust reject them in strict mode; source recovery reports only the earlier complete sequence-zero prefix.

## Repair and caller-directed rewrite

Concrete rewrite operations:

- accept only a strictly verified complete source;
- write a new file rather than modifying damaged input;
- copy authenticated payload ranges;
- limit objects, copied payload bytes, and output bytes;
- require every output root to be retained;
- publish and strictly validate a new genesis commit;
- report source and output snapshot and commit identities;
- report byte-scoped signatures as not preserved.

Caller-selected rewrite is not automatic semantic compaction. Without schema or profile dependency information, the generic container cannot infer every logical dependency.

The experimental CLI therefore exposes `repair-all` and `rewrite-selected`, not an unqualified `compact` command.

## Resource limits

Implementations expose limits appropriate to their assurance level, including:

- file, commit, snapshot, and payload bytes;
- root and capability counts;
- page count, page reads, and depth;
- object count;
- bytes hashed;
- source read operations, bytes read, and request size;
- recovery scan bytes, scan reads, magic matches, validations, cumulative candidate reads, depth, and results;
- rewrite objects, copied bytes, and output bytes;
- diagnostics.

Universal numeric minima remain unresolved and block Review.

## Measured alternatives

### Page size

At 100 million objects using Candidate 1 entries:

| Page size | Depth | Directory bytes | Authenticated path bytes |
|---:|---:|---:|---:|
| 4 KiB | 5 | 9,249,042,432 | 20 KiB |
| 16 KiB | 4 | 8,891,121,664 | 64 KiB |
| 64 KiB | 3 | 8,817,344,512 | 192 KiB |

Candidate 1's 16 KiB pages are a provisional midpoint.

### Leaf locator width

At 100 million objects with 16 KiB pages:

| Leaf layout | Directory size | Depth |
|---|---:|---:|
| 88-byte Candidate 1, 64-bit ID | 8.280 GiB | 4 |
| 72-byte same fields without reserve | 6.778 GiB | 4 |
| 56-byte minimal authenticated, 64-bit ID | 5.264 GiB | 4 |
| 64-byte minimal authenticated, 128-bit ID | 6.007 GiB | 4 |
| 96-byte baseline fields, 128-bit ID | 9.011 GiB | 4 |

Removing 16 reserved bytes per leaf saves approximately 1.50 GiB at this scale. A minimal authenticated 128-bit locator remains smaller than the current 64-bit baseline. The final decision requires cold-cache and range-request measurements because mirrored kind and logical length can avoid object-header reads during metadata inventory.

### Rejected or retained alternatives

- The flat EXP-0001 directory is rejected for promotion.
- A monolithic sorted array remains a measured alternative but rewrites the complete array for small changes.
- Deterministic hash pages remain a measured alternative with unresolved collision, deterministic-rebuild, and range-enumeration rules.
- In-place root replacement is rejected because torn writes can destroy the only authoritative root.
- Implicit recovery fallback is rejected because damage or attacker-selected candidates would alter normal validation semantics.
- One combined identity digest is rejected by ADR-0011.
- Separate progress-checkpoint bytes are deferred by ADR-0012.

## Evidence achieved

- exact Candidate 1 byte specification;
- deterministic Rust genesis and append writers;
- independent Python writer and validator;
- byte-identical genesis, append, and multi-leaf vectors;
- thirteen pinned invalid and interrupted vectors with strict-rejection and diagnostic-layer metadata;
- slice and bounded source strict validators;
- slice and bounded source targeted lookup;
- bounded source recovery charging failed-candidate reads;
- previous-footer chain traversal and fork models;
- every-cut append tests and pinned representative cuts;
- repair-all and caller-selected rewrite output;
- COW reuse and checkpoint-cadence models;
- page-size, locator-width, sorted-array, ordered-tree, and hash-page comparisons;
- 21 layer-targeted adversarial mutations;
- Rust 1.85, 32-bit little-endian, and 64-bit big-endian compilation;
- continuous fuzz targets covering parsing, lookup, source lookup, source strict validation, recovery, writers, rewrite, and Phase 3 models;
- experimental CLI assurance documentation.

## Remaining blockers before Review

1. Implement a deterministic batched byte-level page-reuse writer and publish append-amplification vectors.
2. Measure local cold-cache, HTTP-range, request-count, latency, and source-version behaviour.
3. Select or replace the 88-byte leaf layout and decide object-identifier width.
4. Prototype a bounded external-sort or equivalent deterministic large-writer strategy.
5. Define source-stability, version-token, retry, and remote-mutation rules.
6. Decide normative minimum limits versus caller policy.
7. Define future-field and capability-preservation rules.
8. Define profile-level history retention and semantic compaction inputs.
9. Obtain an independently maintained implementation or independent review outside this repository.
10. Define external trusted freshness without conflating it with integrity.
11. Resolve substantive maintainer objections and complete the proposal review period.

The invalid-vector corpus intentionally requires strict rejection and records coarse diagnostic intent. Exact exception strings remain implementation-local until a separate public error-contract decision is justified.

## Registry allocations requested

None. Candidate values remain local to disposable `UCOF-EXP-0002`.

## Rollout plan

1. Keep Candidate 1 in unpublished `ucof-experiments`.
2. Continue publishing exact vectors, rejected alternatives, work limits, and security findings.
3. Do not describe Candidate 1 files as stable or durable-format commitments.
4. Retire or revise the candidate if evidence invalidates a byte choice.
5. Move this FCP to Review only after the blockers above are resolved and independent evidence exists.

## Rejection or rollback strategy

If Candidate 1 fails, stop emitting its bytes, preserve the corpus for regression and historical analysis, mark the candidate retired, and introduce a new explicit candidate or epoch. Existing bytes must never be silently reinterpreted.

## References

- `docs/spec/EXP_0002_BYTE_CANDIDATE.md`
- `docs/decisions/0009-separate-phase3-research-models.md`
- `docs/decisions/0010-exp0002-first-byte-candidate.md`
- `docs/decisions/0011-exp0002-snapshot-and-commit-identity.md`
- `docs/decisions/0012-exp0002-complete-checkpoints-only.md`
- `docs/experiments/0005-directory-model-comparison.md`
- `docs/experiments/0006-exp0002-page-size-comparison.md`
- `docs/experiments/0007-exp0002-invalid-vector-contract.md`
- `docs/experiments/0008-exp0002-copy-on-write-reuse.md`
- `docs/experiments/0009-exp0002-checkpoint-cadence.md`
- `docs/experiments/0010-exp0002-locator-widths.md`
- `docs/security/EXP_0002_MODEL_FINDINGS.md`
- `docs/security/EXP_0002_BYTE_FINDINGS.md`
- `docs/THREAT_MODEL.md`
- `docs/PHASE_3_STATUS.md`
- `docs/PHASE_3_CLI_GUIDE.md`
- `tests/vectors/exp-0002/manifest.json`
- `tests/vectors/exp-0002-invalid/manifest.json`

## Decision record

- **Decision:** Pending
- **Decision date:**
- **Review period:**
- **Approvers:**
- **Blocking objections and disposition:**
- **Required follow-up:**
