# EXP-0002 Candidate 1 Byte Security Findings

## Status

This document records executable findings from the first concrete `UCOF-EXP-0002` byte candidate. The candidate is disposable and non-stable. These findings constrain FCP-0002 but do not accept the proposal, allocate permanent identifiers, or provide authenticity, freshness, or compatibility guarantees.

The earlier `EXP_0002_MODEL_FINDINGS.md` remains the record of algorithm-model evidence. This companion covers exact serialized bytes and concrete implementations.

## Evidence surface

The findings come from:

- `docs/spec/EXP_0002_BYTE_CANDIDATE.md`;
- ADR-0010 and ADR-0011;
- Rust `ucof-experiments::exp0002`;
- Rust authenticated lookup, recovery, and rewrite modules;
- independent `tools/exp0002_codec.py`;
- pinned vectors under `tests/vectors/exp-0002`;
- `tools/test_exp0002_adversarial.py`;
- page-size Experiment 0006;
- exact every-cut append tests;
- five concrete EXP-0002 fuzz targets;
- stable, Rust 1.85, 32-bit, and big-endian CI.

## Confirmed byte-level controls

### Bootstrap and fixed-field parsing

The concrete readers enforce:

- exact file, object, page, snapshot, and footer magic;
- experimental version and fixed header/footer lengths;
- little-endian integer decoding independent of host byte order;
- exact digest algorithm identifier;
- zero required flags;
- zero reserved bytes;
- checked offset-plus-length arithmetic before slicing or host-size conversion;
- exact-end footer discovery for strict validation;
- caller limits before large allocation or traversal.

A non-zero reserved byte is rejected even when outer digests have been recomputed to authenticate the malformed structure.

### Domain-separated integrity

Candidate 1 uses separate SHA-256 domains for:

- object records;
- directory pages;
- snapshot records;
- commit publication.

The adversarial corpus demonstrates that a mutation can be driven to each layer by recomputing outer digests while retaining an invalid inner claim. Readers therefore cannot rely on the commit digest alone; they must still validate page, snapshot, object, canonical-order, padding, range, and semantic invariants.

### Page canonicality

Concrete page readers reject:

- zero or excessive entry counts;
- wrong fixed entry sizes;
- non-zero padding;
- unordered or duplicate leaf keys;
- invalid or overlapping child ranges;
- inconsistent child levels;
- forged child digests;
- root or child range mismatches;
- page cycles or repeated offsets during full traversal;
- page digests that do not match all 16 KiB, including padding.

Zero padding is part of the page digest. This removes an otherwise unconstrained covert byte region and preserves deterministic output.

### Object locator cross-checks

A leaf entry does not become authoritative merely because its page is authenticated. Strict validation also checks:

- object magic and header length;
- non-zero object kind and identifier;
- zero object flags and reserved bytes;
- payload and logical length equality for candidate 1;
- exact record length;
- entry/header identifier and kind agreement;
- object-record digest;
- in-bounds physical range;
- non-overlap with other object records, pages, snapshot, and footer.

The targeted lookup API applies the same selected-record isolation against every authenticated page on its path, the active snapshot, and the exact-end footer.

## Commit and snapshot findings

### Exact-end publication

A commit has authority only when a complete 160-byte footer ends at exact file end. Every tested truncation before footer completion fails strict latest validation. Appending arbitrary trailing bytes also fails strict validation.

The footer publishes:

- commit range;
- snapshot locator;
- sequence;
- previous-footer locator;
- object-record count for the current commit;
- snapshot digest;
- commit digest.

Footer semantics are included in the commit digest, so a modified sequence, locator, previous pointer, or snapshot digest requires a new commit digest and still remains subject to independent structural checks.

### Historical object references

An append commit hashes bytes written after the previous footer, not every historical object payload. A later snapshot can reference an earlier object through its authenticated leaf entry and object digest.

Consequences:

- full strict validation rehashes every referenced object, including historical records;
- authenticated single-object lookup rehashes the selected historical object;
- unrelated historical payloads are not required for a targeted lookup;
- the current in-memory strict validator still receives the whole file slice, while the next source-based API must preserve these work boundaries over range reads.

### Identity scopes

ADR-0011 records two distinct scopes:

- snapshot digest: exact authenticated snapshot structure;
- commit digest: one file-instance publication.

A deterministic genesis repair with identical object layout can preserve the snapshot digest while changing the commit digest because the new file header changes the genesis commit preimage. This is intentional structural equality, not preservation of the original file instance.

Repair and compaction reports therefore expose both scopes and never claim byte-scoped signature preservation.

## Recovery findings

Concrete recovery scanning:

- searches only when explicitly requested;
- independently bounds scan bytes, magic matches, candidate validations, results, and previous-chain depth;
- validates each candidate as an exact-end prefix;
- never promotes footer magic or a previous pointer without strict prefix validation;
- reports no candidate outside the configured scan window;
- recovers the earlier complete genesis across every tested cut of an interrupted append;
- revalidates each previous-footer ancestor during chain enumeration.

The previous-footer pointer remains a discovery aid. It does not prove that the referenced footer, snapshot, directory, or objects are valid.

## Authenticated lookup findings

The targeted lookup API establishes a narrower assurance level than full strict validation. It verifies:

1. bootstrap header;
2. exact-end footer structure;
3. active commit digest;
4. active snapshot digest and structure;
5. parent footer sequence and snapshot-digest linkage;
6. one authenticated directory path;
7. selected object header, locator, range isolation, and object digest.

It can also return authenticated absence when the selected page path proves no key exists.

It does **not** claim that unrelated historical object records were read or rehashed. Documentation and API names must preserve this distinction.

## Repair and compaction findings

Concrete rewrite operations:

- first strictly validate the complete source snapshot;
- reject damaged sources before copying;
- copy only authenticated payload ranges;
- independently limit object count, payload bytes, and output bytes;
- require every output root to be retained;
- publish a new genesis commit;
- strictly validate generated output before returning success;
- report source/output snapshot and commit digests separately;
- state that byte-scoped signatures are not preserved.

Caller-directed object selection is not automatic semantic compaction. Without schema/profile dependency information, the core experiment cannot infer whether an unretained object is logically required.

## Independent implementation findings

Python and Rust independently produce byte-for-byte identical files for:

- two-object genesis;
- append adding a third object while reusing earlier records;
- a 400-object multi-leaf directory.

Each pinned vector has a published file length, footer offset, and whole-file SHA-256 in its manifest. Both implementations validate the stored corpus continuously.

This is useful differential evidence but is not yet a second independently maintained implementation. Both implementations live in the same repository and can share specification misunderstandings.

## Adversarial findings

The layer-targeted Python suite rejects 21 cases including:

- header magic, reserved bytes, and digest identifier;
- object payload digest, logical length, and reserved bytes;
- root page digest;
- authenticated leaf padding;
- child page digest and overlapping child ranges;
- snapshot reserved bytes;
- parent snapshot digest;
- forward previous-footer pointer;
- snapshot/footer sequence mismatch;
- footer magic, reserved bytes, and commit digest;
- strict trailing bytes;
- representative append truncations around object, snapshot, and footer publication.

Where necessary, the suite recomputes page, snapshot, and commit digests after mutation. Passing these cases therefore requires validation beyond the first outer hash mismatch.

## Page-size finding

Using the exact 88-byte leaf and 64-byte internal entries at 100 million objects:

| Page size | Depth | Directory bytes | Authenticated path bytes |
|---:|---:|---:|---:|
| 4 KiB | 5 | 9,249,042,432 | 20 KiB |
| 16 KiB | 4 | 8,891,121,664 | 64 KiB |
| 64 KiB | 3 | 8,817,344,512 | 192 KiB |

The provisional 16 KiB page remains a middle point. The leaf entry width dominates total directory size, so page-size tuning alone does not solve metadata overhead.

## Continuous fuzz evidence

The branch runs eighteen fuzz targets. Five exercise concrete EXP-0002 bytes:

1. raw strict validation;
2. bounded recovery scanning;
3. writer-generated genesis and append round trips;
4. authenticated lookup;
5. repair and object-selection rewrite output.

All targets compile under nightly and complete bounded pull-request smoke campaigns. Scheduled workflows retain read-only repository permissions.

## Residual risks

Candidate 1 does not yet resolve:

- range-based source APIs for concrete readers;
- remote source mutation during validation;
- copy-on-write page reuse;
- realistic HTTP-range and cache behaviour;
- pinned invalid-vector files and expected failure categories;
- complete checkpoint bytes or long-running progress checkpoints;
- narrower leaf entries and identifier widths;
- streaming external sort for very large writers;
- authenticity, signer trust, provenance, encryption, or protected metadata;
- external freshness and whole-file rollback resistance;
- transforms, compression, or expansion limits;
- profile-driven dependency discovery for automatic compaction.

SHA-256 integrity relative to stored values is not authenticity. Sequence and previous pointers are not freshness. A valid old file can be replayed unless an external trusted state records a newer identity.

## Required next evidence

Before FCP-0002 can enter Review:

- implement bounded range-source lookup and recovery;
- pin invalid and interrupted byte vectors with expected outcomes;
- measure HTTP-range and storage-cache behaviour;
- implement and measure copy-on-write page reuse;
- decide complete-checkpoint bytes and whether progress checkpoints remain in scope;
- evaluate narrower leaf entry layouts;
- obtain an independently maintained implementation or review;
- integrate these findings into the primary threat model;
- resolve every remaining FCP-0002 question that blocks independent implementation.
