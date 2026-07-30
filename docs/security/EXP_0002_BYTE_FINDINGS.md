# Security Findings from UCOF-EXP-0002 Candidate 1

## Status and scope

This document records executable findings from the first disposable EXP-0002 byte candidate. It complements the model-level findings in `EXP_0002_MODEL_FINDINGS.md` and the primary threat model.

Candidate 1 remains experimental. Passing tests do not stabilize the bytes or imply production suitability.

## Implemented byte surface

Candidate 1 currently includes:

- a 64-byte bootstrap header;
- 48-byte opaque object headers;
- fixed 16 KiB authenticated leaf and internal pages;
- 88-byte leaf and 64-byte internal entries;
- variable-length snapshot records;
- 160-byte exact-end commit footers;
- append commits linked by parent snapshot digest and previous-footer offset;
- object, page, snapshot, and commit SHA-256 domains;
- strict slice and bounded random-access source validation;
- targeted authenticated lookup and absence results;
- bounded source recovery and verified history enumeration;
- repair and caller-selected rewrite output.

## Confirmed controls

### Exact structure and canonical bytes

The Rust and independent Python implementations reject:

- incorrect magic, epoch, version, fixed lengths, flags, or algorithm identifiers;
- non-zero reserved bytes and unused page padding;
- zero or duplicate object identifiers;
- zero object kind;
- object logical/stored length disagreement;
- unordered leaf identifiers;
- overlapping or unordered internal child ranges;
- invalid page level or entry-size relationships;
- malformed canonical root and capability arrays;
- trailing bytes after the exact-end footer.

All offsets and lengths use checked arithmetic before range construction or host-size conversion.

### Domain-separated integrity

Object, page, snapshot, and commit digests use separate domain prefixes. The digest algorithm is identified explicitly rather than inferred from digest length.

Layer-targeted mutations recompute outer digests where required. The readers still reject authenticated inner inconsistencies including:

- non-zero page padding;
- forged child ranges;
- inconsistent child levels;
- invalid parent links;
- object-header and leaf-locator disagreement;
- page, object, snapshot, and footer physical overlap.

An authenticated outer layer therefore does not suppress required inner structural validation.

### Strict exact-end publication

Only a complete 160-byte footer ending at exact file end publishes the active Candidate 1 snapshot. Every tested incomplete append fails strict validation.

Strict validation never invokes backward scanning. The active result is returned only after the footer, snapshot, complete directory graph, every referenced object, roots, parent relationship, and current commit digest have passed.

### Bounded random-access source validation

`validate_strict_at` performs full validation over a synchronous `ReadAt`-style source without materializing the entire file. It streams commit and object hashes and bounds:

- file, commit, snapshot, and payload bytes;
- source read operations and cumulative bytes;
- maximum read request;
- bytes hashed;
- page reads, page count, and depth;
- object, root, and capability counts.

All three valid vectors pass full source validation. All thirteen pinned invalid vectors fail.

### Targeted lookup assurance

Targeted lookup authenticates:

- the bootstrap header;
- exact-end footer and current commit;
- active snapshot and parent relationship;
- one root-to-leaf path;
- the selected object record.

It may return authenticated absence. It explicitly does not claim that unrelated historical object records were rehashed.

A localhost HTTP Range benchmark over a 1,082,529-byte append file measured:

| Assurance | Requests | Bytes transferred | Pages | Objects hashed |
|---|---:|---:|---:|---:|
| Targeted lookup | 7 | 16,993 | 1 | 1 |
| Full strict validation | 26 | 1,065,673 | 1 | 3 |

The targeted lookup made no request overlapping the unrelated 1 MiB historical record. Full validation did read that record.

### Stable remote or mutable source view

ADR-0013 introduces an implementation-local versioned-source adapter. A caller maps strong storage evidence into an opaque 32-byte token. The adapter requires the same token before and after every length or range read.

Tests confirm:

- a stable token permits full validation;
- a token change during validation fails;
- a token change before a read fails without consuming bytes.

This prevents silent mixed-version reads when the underlying transport provides honest strong version evidence. It does not prove that the version is the newest trusted version.

### Explicit bounded recovery

Source recovery independently bounds:

- suffix bytes scanned;
- scan read operations and request size;
- footer-magic matches;
- candidate validations;
- cumulative candidate bytes read;
- linked-chain depth;
- returned results.

Reads spent rejecting failed candidates are charged. Footer magic and previous-footer pointers have no authority by themselves. Every reported candidate is validated as an exact-end prefix by the full source validator.

The pinned interrupted append vectors cover cuts after an object header, before snapshot completion, and within a footer prefix. Recovery reports only the earlier complete sequence-zero prefix.

### Verified linked history

`enumerate_previous_chain_at` validates the exact-end active file and every linked ancestor as independent strict prefixes. It cross-checks:

- the child's previous-footer offset;
- the parent's exact footer location;
- parent snapshot digest;
- sequence decrement by exactly one;
- chain cycles;
- depth and cumulative read budgets.

Each reported commit includes roots, previous-footer offset, parent snapshot digest, snapshot digest, and commit digest. Enumeration does not search for unlinked candidates and does not resolve forks implicitly.

### Repair and caller-selected rewrite

Concrete rewrite operations:

- accept only strictly verified sources;
- write a new file rather than modifying damaged input;
- copy authenticated payload ranges;
- enforce object, copied-payload, and output-byte limits;
- require retained output roots;
- build and strictly validate the new genesis output;
- expose source and output snapshot and commit identities;
- report byte-scoped signatures as not preserved.

The experimental CLI creates outputs with create-new semantics and requests filesystem synchronization after writing. Caller-selected rewrite is not described as semantic compaction.

## Validation-order findings

One byte mutation does not imply one universal error category. A malformed outer structure may fail before digest comparison; a payload mutation preserving framing can reach object or commit digest failure; an authenticated inner inconsistency can reach page, directory, parent, or object cross-validation.

The invalid corpus therefore promises strict rejection and records coarse diagnostic intent. Exact exception strings and some validation-order details remain implementation-local until a public error contract is justified.

## Page identity blocker

Experiment 0011 proves that Candidate 1 cannot reuse an unchanged historical page byte-for-byte.

Each page stores the active snapshot sequence, the page digest authenticates that sequence, and strict validation requires equality with the active snapshot. Two unchanged historical leaves become byte-identical only when the sequence field is masked. A fully reauthenticated append that references the exact historical page is rejected at the page/snapshot sequence check.

Consequences:

- every append must rewrite every directory page under current semantics;
- rewriting only the sequence changes each page digest;
- changed leaf digests propagate through every ancestor;
- the persistent COW algorithm model cannot be realized by Candidate 1 bytes.

A later candidate must revise page identity, reinterpret the field as page birth generation, move publication context outside immutable page bytes, or explicitly accept full-directory rewrite amplification.

Removing sequence equality must not permit unauthenticated page splicing. A revised candidate must define file/epoch domain separation, cross-file sharing, physical-locator identity, cycle rejection, equality leakage, and repair identity reporting.

## Scale and layout findings

### Page size

At 100 million objects with 88-byte leaves:

| Page size | Depth | Directory bytes | Path bytes |
|---:|---:|---:|---:|
| 4 KiB | 5 | 9,249,042,432 | 20 KiB |
| 16 KiB | 4 | 8,891,121,664 | 64 KiB |
| 64 KiB | 3 | 8,817,344,512 | 192 KiB |

The provisional 16 KiB page is a midpoint, not an accepted constant.

### Locator width

At 100 million objects with 16 KiB pages:

| Leaf layout | Directory size |
|---|---:|
| 88-byte Candidate 1, 64-bit ID | 8.280 GiB |
| 72-byte same fields without reserve | 6.778 GiB |
| 56-byte minimal authenticated, 64-bit ID | 5.264 GiB |
| 64-byte minimal authenticated, 128-bit ID | 6.007 GiB |

Sixteen reserved bytes per leaf cost about 1.50 GiB at this scale. Removing mirrored kind and logical length reduces directory size but can increase remote metadata-inventory requests. The final decision requires measured inventory workloads.

### Checkpoint cadence

Candidate 1 checkpoints are ordinary complete commits. Measured cadence models show a strategy crossover:

- frequent checkpoints make repeated full-directory rebuilds dominant;
- sparse checkpoints can make naive per-object path copying worse than one final rebuild;
- a reusable-page writer must batch changes, share copied ancestors, and serialize only final reachable pages.

## Large deterministic writer evidence

Experiment 0013 sorts 200,003 exact 88-byte locator-shaped records through bounded spill runs and a k-way merge.

Confirmed properties:

- sub-megabyte run buffers;
- exact spill and output byte accounting;
- output independent of run size;
- complete identifier coverage;
- duplicate rejection within and across runs;
- no requirement to retain the complete locator ledger in memory.

Residual spill risks include temporary-file path safety, file-descriptor limits, disk exhaustion, cleanup after failure, concurrent writer collisions, and metadata leakage. Encrypted profiles need an explicit protected-spill policy; normal deletion is not guaranteed physical erasure.

The external sort is an algorithm model, not yet integrated into the byte writer or footer publication path.

## Differential, adversarial, portability, and fuzz evidence

- Rust and independent Python implementations produce byte-identical genesis, append, and multi-leaf vectors.
- Both implementations continuously validate the valid corpus.
- Python regenerates the thirteen-file invalid corpus; Rust independently rejects every file.
- 21 layer-targeted adversarial cases exercise headers, objects, pages, padding, child links, snapshots, parent links, footers, exact-end state, and append truncation.
- Every-cut tests cover interrupted append publication.
- Page-size, locator-width, COW, checkpoint, page-reuse, HTTP-range, and external-sort experiments run continuously.
- Twenty-one fuzz targets cover inherited Phase 2 byte paths, Phase 3 algorithm models, and concrete Candidate 1 strict parsing, recovery, writing, lookup, rewrite, source lookup, source strict/recovery, and linked history.
- The workspace compiles at Rust 1.85, on 32-bit little-endian, and on 64-bit big-endian targets.
- The isolated experimental CLI has end-to-end tests for verify, roots, history, lookup, recovery, repair, rewrite, and output non-overwrite.

## Residual risks and blockers

- SHA-256 integrity is not authenticity, signer trust, confidentiality, or external freshness.
- A valid older whole file can be replayed without trusted external state.
- Candidate 1 page bytes prohibit historical page reuse and force directory rewrites.
- The final page identity, leaf layout, and object-identifier width remain unresolved.
- Source APIs are synchronous.
- Versioned stable-view protection depends on honest strong transport evidence and does not eliminate storage-system time-of-check/time-of-use windows unless conditional reads are atomic.
- Rewrite operations currently materialize input and output in memory.
- External sorting is not integrated into the byte writer.
- Default limits are implementation-local; normative minima remain unresolved.
- Automatic semantic dependency discovery is unavailable without schemas, profiles, or caller input.
- Both current byte implementations share one repository and may share a specification misunderstanding.
- Profiles, transforms, compression, schemas, signatures, provenance, encryption, and external references remain outside Candidate 1.

## Review consequence

FCP-0002 must not move to Review while Candidate 1 page identity prevents its intended reuse model, locator and identifier choices remain unresolved, the bounded external-sort prototype is not integrated into a failure-safe byte writer, concrete conditional remote-source adapters are absent, normative limits are undecided, and independent stewardship is missing.
