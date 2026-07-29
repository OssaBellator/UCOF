# FCP-0002: Paged Directories, Snapshots, and Recovery

- **Status:** Draft
- **Authors:** UCOF maintainers and contributors
- **Created:** 2026-07-30
- **Last updated:** 2026-07-30
- **Target:** Core experimental epoch
- **Experimental epoch impact:** Defines disposable `UCOF-EXP-0002`
- **Related issues:** Phase 3 review and implementation work
- **Related ADRs:** ADR-0009, ADR-0010, ADR-0011
- **Supersedes:** None
- **Superseded by:** None

## Summary

This proposal introduces the first experimental UCOF layout with:

- an authenticated paged primary directory;
- append-only complete snapshots;
- exact-end commit publication;
- parent snapshot and previous-footer relationships;
- bounded strict validation and separately requested recovery;
- authenticated single-object lookup and absence results;
- valid-root enumeration;
- repair-to-new-file and compaction output.

The proposal requires a new disposable epoch because directory, snapshot, active-root, recovery, and identity semantics determine file validity and are incompatible with `UCOF-EXP-0001`.

**Candidate 1 now has exact experimental bytes, two independent in-repository implementations, pinned vectors, hostile-input tests, range-source lookup, and continuous fuzzing.** Those results are evidence for this Draft. They do not make the bytes stable, accept this FCP, or assign permanent registry values.

The independently implementable candidate is defined by `docs/spec/EXP_0002_BYTE_CANDIDATE.md`. This FCP records the motivation, compatibility and security requirements, evidence, rejected alternatives, and remaining decisions that must be resolved before Review.

## Motivation

EXP-0001 demonstrated safe framing but also established that a flat, fully materialized directory cannot satisfy UC-02-scale archives. One million zero-byte objects already impose approximately 40 MB of record headers and 52 MB of directory payload before application data. Raising parser limits does not solve the architectural problem.

Phase 3 must also support failure and history semantics absent from EXP-0001:

- publish a new root without invalidating an earlier complete root;
- distinguish strict active-state validation from recovery scanning;
- discover and authenticate one object without materializing every directory entry;
- report every plausible historical root without silently resolving forks;
- repair or compact into a new valid output without rewriting damaged input in place;
- preserve explicit assurance boundaries under hostile input.

The relevant Phase 0 cases include UC-02 large archives, UC-04 interrupted append capture, UC-07 damaged files with an earlier valid checkpoint, UC-08 unsupported-profile core readers, and UC-10 malicious resource-exhaustion inputs.

## Scope

FCP-0002 defines or constrains:

1. the relationship between `UCOF-EXP-0002` files, complete snapshots, directory roots, and commit footers;
2. the first paged primary-directory representation;
3. deterministic object, page, snapshot, and commit digest scopes;
4. exact-end strict active-root selection;
5. separately bounded recovery candidate discovery and previous-footer traversal;
6. parent-chain and sequence validation;
7. authenticated lookup and absence results;
8. valid-root enumeration and fork ambiguity;
9. repair and compaction publication rules;
10. required cross-language vectors, hostile-input tests, fuzzing, portability, and scale evidence.

## Non-goals

This proposal does not define:

- stable Core 1.0 bytes;
- permanent numeric registry allocations;
- transforms, compression, schemas, encryption, signatures, provenance, or external references;
- semantic dependency discovery for arbitrary objects;
- distributed concurrency or multi-writer conflict resolution;
- authenticity, trusted freshness, or protection against replaying an older valid whole file;
- profile-specific secondary indexes;
- general progress-checkpoint bytes in Candidate 1.

## Terminology

This proposal uses the project glossary. Additional experimental terms are:

- **Complete snapshot:** a snapshot whose directory, roots, referenced objects, and publishing footer satisfy all Candidate 1 checks.
- **Commit footer:** the exact-end structure that publishes one complete snapshot.
- **Strict validation:** validation of the footer ending at exact file end, with no recovery fallback.
- **Recovery candidate:** a prefix ending in a possible footer that must be strictly validated before it can be reported as verified.
- **Structural snapshot identity:** the snapshot digest defined by ADR-0011.
- **File-instance commit identity:** the commit digest defined by ADR-0011.
- **Candidate 1:** the first disposable exact-byte realization of `UCOF-EXP-0002`.

## Detailed specification

### Experimental epoch

Candidate 1 uses the experimental epoch `UCOF-EXP-0002`. Unknown epochs are unsupported. Candidate bytes may be retired rather than migrated.

The complete field tables, offsets, byte order, magic values, padding rules, digest preimages, physical order, and strict validation order are defined in `docs/spec/EXP_0002_BYTE_CANDIDATE.md`.

### Candidate 1 byte choices

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
- distinct object, page, snapshot, and commit domain prefixes;
- `u64::MAX` as the absent-offset sentinel;
- zero required flags, reserved bytes, and unused page padding.

These are experimental selections, not permanent registry assignments.

### Object records

Object records contain a fixed header and opaque payload. Candidate 1 requires:

- non-zero object kind and identifier;
- unique object identifiers in one snapshot directory;
- payload and logical length equality;
- zero flags and reserved bytes;
- exact record-length agreement;
- an object digest stored in the authenticating leaf entry.

The object digest covers the exact object header and payload under the object domain.

### Paged primary directory

Candidate 1 uses an ordered authenticated tree.

Leaf pages contain sorted unique object identifiers and physical locators. Internal pages contain sorted non-overlapping child ranges, exact child levels, physical page locators, and child-page digests.

A reader must validate:

- page magic, kind, level, fixed entry size, count, range, sequence, and zero padding;
- strict key ordering and non-overlapping ranges;
- child level and range agreement;
- exact 16 KiB page length;
- page digest over all page bytes, including padding;
- invalid references, repeated offsets, and cycles;
- leaf locator agreement with referenced object headers;
- physical non-overlap with objects, pages, snapshot, and footer as required by the active assurance level.

The current deterministic writer rebuilds every directory page for each snapshot. Page reuse is not yet defined.

### Snapshot record

A Candidate 1 snapshot authenticates:

- sequence number;
- parent snapshot digest;
- previous-footer offset;
- directory-root locator, level, and digest;
- canonical root object identifiers;
- canonical required and optional capability arrays.

Genesis uses sequence zero, a zero parent digest, and the absent previous-footer sentinel. A child sequence is exactly its parent's sequence plus one.

Candidate 1 defines no non-zero capability allocation. Writers therefore emit empty capability arrays, while readers still enforce the structural representation.

### Commit publication

A complete 160-byte footer ending at exact file end publishes the active strict snapshot.

The footer authenticates:

- the current commit byte range;
- snapshot locator and digest;
- sequence;
- previous-footer offset;
- current-commit object-record count;
- digest algorithm and reserved fields.

The commit digest covers the exact current commit bytes and footer semantics under the commit domain. Genesis includes the file header. Append commits begin immediately after the previous footer.

Bytes appended before a complete new footer are unpublished tail state. Strict validation fails at the new end; explicit recovery may still find and strictly validate an earlier complete prefix.

### Historical references

A new snapshot may reference historical object records. The current commit digest covers only bytes written by the current commit, while leaf entries authenticate historical object records individually.

Consequently:

- full strict validation rehashes every referenced object;
- targeted lookup rehashes the selected object;
- unrelated historical payloads need not be read for one authenticated lookup;
- implementations must state which assurance level they provide.

### Identity scopes

ADR-0011 defines two scopes:

- snapshot digest identifies the exact authenticated snapshot structure;
- commit digest identifies one file-instance publication.

A deterministic genesis repair may preserve the structural snapshot digest while changing the commit digest because the new file header changes the genesis commit preimage. This is not file-instance equality.

No repair or compaction API may imply preservation of byte-scoped signatures when the signed commit bytes change.

### Strict validation

Strict validation must:

1. apply file and resource limits before parsing;
2. require one exact-end footer;
3. validate footer structure and ranges;
4. hash the exact commit range and footer semantics;
5. authenticate and parse the referenced snapshot;
6. cross-check sequence, parent, and previous-footer fields;
7. authenticate the directory root and traverse pages under explicit limits;
8. validate directory canonicality and physical claims;
9. validate roots and capability arrays;
10. validate referenced object records and digests under the selected assurance level;
11. return a verified result only after every required check succeeds.

Strict mode must never silently invoke backward scanning.

### Authenticated lookup

Candidate 1 supports a narrower authenticated lookup assurance level. It verifies:

- bootstrap header;
- exact-end footer and current commit digest;
- active snapshot and parent link;
- one authenticated root-to-leaf page path;
- the selected object header, range, and digest.

It may return authenticated absence when the path proves no matching key. It does not claim unrelated historical objects were rehashed.

The current range-source implementation streams commit and selected-object hashes, enforces read-operation, read-byte, request-size, page, and hash budgets, and demonstrates that a one-megabyte unrelated historical payload is not read.

### Root enumeration and fork handling

Enumeration and selection are separate operations.

A bounded enumerator may classify candidates as verified terminals, verified ancestors, fork terminals, progress checkpoints, integrity failures, unsupported-capability candidates, truncations, or chain failures.

Equal-priority verified forks are ambiguous. A default reader must not silently select one.

### Recovery

Recovery is explicit and independently bounded. Candidate discovery must limit:

- bytes scanned;
- footer-magic matches;
- candidate validations;
- previous-chain depth;
- returned results;
- diagnostic output.

Every candidate is validated as an exact-end prefix. Footer magic and previous-footer pointers are discovery aids, not authority.

The current implementation validates every cut across an interrupted append and recovers the earlier complete genesis prefix when it remains within the configured scan window.

### Repair and compaction

Repair and compaction:

- accept only a strictly verified complete source snapshot;
- write a new file rather than mutating the damaged source;
- copy only authenticated payload ranges;
- bound object count, payload bytes, and output bytes;
- require every output root to exist in the retained set;
- publish and strictly validate a new genesis commit;
- report source and output snapshot and commit digests separately;
- never claim byte-scoped signature preservation.

Caller-directed object selection is not automatic semantic compaction. Without profile or schema dependency information, the core cannot infer every logical dependency.

## Compatibility impact

### Existing files with new readers

Candidate 1 readers may continue to report `UCOF-EXP-0001` separately. They must not reinterpret EXP-0001 bytes as EXP-0002.

### New files with old readers

EXP-0001 readers must report EXP-0002 as unsupported. There is no fallback interpretation.

### Unknown capabilities and data preservation

Candidate 1 defines no non-zero capability allocation. Future capability-bearing candidates must distinguish required, optional, and advisory behavior and define safe preservation rules before Review.

### Profile and schema compatibility

Profiles and schemas are outside Candidate 1. Object kinds are experimental and must not be treated as permanent semantic identifiers.

### Canonical identity and signatures

Object, page, snapshot, and commit digests have explicit domains. They provide integrity relative to stored values, not authenticity. Signature and provenance envelopes remain future proposals.

### Version or experimental epoch impact

This proposal creates `UCOF-EXP-0002`. Candidate 1 bytes may be retired if evidence invalidates the choices. A materially incompatible replacement must not silently reuse the same candidate bytes.

### Migration and coexistence

There is no normative EXP-0001-to-EXP-0002 migration. Experimental tools may extract objects and rewrite a new file while reporting changed identity scopes.

## Security impact

Candidate 1 adds attacker-controlled:

- page graphs and key ranges;
- page and object locators;
- parent and previous-footer relationships;
- footer candidate storms;
- large commit ranges and historical-object references;
- root and capability arrays;
- repair-selection inputs.

Required controls include checked arithmetic, exact ranges, zero padding, domain-separated digests, cycle detection, stable-source assumptions, independent budgets, strict/recovery separation, fork ambiguity, and explicit assurance levels.

Detailed executable findings are recorded in:

- `docs/security/EXP_0002_MODEL_FINDINGS.md`;
- `docs/security/EXP_0002_BYTE_FINDINGS.md`;
- the primary threat model.

SHA-256 does not provide signer trust, freshness, rollback resistance, or confidentiality. A valid older whole file can be replayed without external trusted state.

## Privacy impact

Candidate 1 leaves header identifiers, object identifiers, key ranges, root identifiers, directory shape, snapshot sequence, previous-footer relationships, object lengths, and equality through digests visible.

Encryption and protected metadata discovery are outside scope. Profiles must not assume that Candidate 1 hides names, relationships, sizes, history, or access patterns.

## Resource-limit impact

Implementations must expose limits appropriate to their assurance level, including:

- file and commit bytes;
- snapshot bytes and array counts;
- page reads, page count, and tree depth;
- objects and payload bytes;
- bytes hashed;
- source read operations, bytes read, and maximum request size;
- recovery scan bytes, magic matches, validations, depth, and results;
- rewrite objects, copied bytes, and output bytes;
- diagnostics.

The proposal does not yet prescribe universal numeric defaults. That remains a Review blocker.

## Streaming impact

Candidate 1 writers can append without seeking when object lengths are known before their headers are emitted. The current implementation writes to memory and rebuilds the directory.

Large production writers will require bounded external sorting or another deterministic directory-construction strategy. This remains unresolved.

## Random-access impact

Strict validation may require hashing the current commit and all referenced objects. Authenticated lookup requires the exact-end footer, snapshot, one page per tree level, and the selected object record.

Candidate 1 now has a bounded synchronous range-source implementation. Its source view must remain stable for one operation; version tokens, retry rules, and remote mutation handling remain unresolved.

## Recovery, truncation, and compaction

- strict mode accepts only the exact-end footer;
- every incomplete latest append is invalid in strict mode;
- explicit recovery can validate earlier complete prefixes;
- equal verified forks are reported as ambiguous;
- repair and compaction publish a new output file;
- old bytes remain until a separate rewrite discards them;
- progress checkpoint bytes are not defined by Candidate 1.

## Canonicalization and identity

Candidate 1 canonicalizes:

- physical write order;
- fixed-width field encoding;
- zero flags, reserved bytes, and padding;
- ascending unique root and capability arrays;
- ascending leaf keys and non-overlapping child ranges;
- exact digest domains and preimages.

Identity scopes follow ADR-0011. Snapshot identity is structural and includes physical locators. Commit identity is publication-specific.

## Alternatives considered

### Flat directory

Rejected for promotion because its scale failure is measured and architectural.

### Monolithic sorted array

Retained as a measured alternative but rejected as the primary candidate because one small update rewrites the complete array and recovery publication cannot localize page work.

### Deterministic hash pages

Retained as a measured alternative. They can provide expected constant lookup but require unresolved collision, ordering, deterministic rebuild, and range-enumeration rules.

### 4 KiB pages

At 100 million objects, the concrete entry layout requires five page levels and approximately 9.25 GB of directory bytes, but only 20 KiB per authenticated path.

### 64 KiB pages

At 100 million objects, the layout requires three page levels and approximately 8.82 GB of directory bytes, but 192 KiB per authenticated path and ideal path-copy update.

### 16 KiB pages

Candidate 1 uses four levels, approximately 8.89 GB, and 64 KiB per authenticated path at that scale. It is a provisional midpoint, not an accepted constant.

### One identity digest

Rejected by ADR-0011 because structural snapshot equality and file-instance publication equality are distinct useful scopes.

### In-place root replacement

Rejected because torn writes can destroy the only authoritative root and complicate recovery reasoning.

### Implicit recovery fallback

Rejected because it would let damage or attacker-selected candidates alter normal validation semantics.

## Unresolved questions

The following still block movement to Review:

1. Are 88-byte leaf entries and 64-bit object identifiers acceptable, or should Candidate 2 test narrower locators and identity widths?
2. Should 16 KiB pages remain the candidate after real local and HTTP-range benchmarks?
3. What exact copy-on-write page-reuse algorithm preserves deterministic output and bounds append amplification?
4. What bytes define a complete checkpoint, what cadence is recommended, and are progress checkpoints justified at all?
5. What history-retention and compaction policy should profiles expose?
6. Which resource limits become normative minima, and which remain caller policy?
7. How are unknown optional fields and future capability allocations preserved?
8. What bounded external-sort strategy is required for very large deterministic writers?
9. What source-stability, version-token, retry, and remote-mutation contract is required for range readers?
10. What evidence is required from an independently maintained implementation outside this repository?
11. Should invalid vectors pin exact public error categories or only verified-invalid outcomes?
12. How should external freshness state identify the latest acceptable commit without conflating integrity and trust?

## Implementation plan

### Completed evidence

- non-normative paged-directory, selection, enumeration, publication, recovery, compaction, and repair models;
- ordered-tree, sorted-array, hash-page, and page-size comparisons;
- exact Candidate 1 byte specification;
- deterministic Rust genesis and append writers;
- strict Rust validator;
- independent Python writer and validator;
- pinned cross-language valid vectors;
- every-cut append tests;
- bounded recovery scanning and previous-chain traversal;
- in-memory and bounded range-source authenticated lookup;
- repair-to-new-file and caller-directed selection rewrite;
- layer-targeted adversarial corpus;
- stable, Rust 1.85, 32-bit, and big-endian checks;
- continuous model and byte fuzz targets;
- model and concrete-byte security findings.

### Remaining implementation

- copy-on-write page reuse;
- complete-checkpoint bytes and experiments;
- pinned invalid/interrupted vectors and expected outcomes;
- realistic local and HTTP-range benchmarks;
- narrower entry candidates;
- bounded external-sort writer prototype;
- CLI inspection, root enumeration, recovery, repair, and compaction commands;
- independently maintained implementation and interoperability report.

## Evidence and validation

### Achieved

- Rust and Python agree byte-for-byte on genesis, append, and multi-leaf valid vectors;
- both implementations validate the stored corpus;
- 21 layer-targeted concrete adversarial cases fail closed;
- outer digests are recomputed where necessary to reach deeper checks;
- every tested interrupted append cut preserves recovery of the earlier complete root;
- candidate storms and scan windows are bounded;
- authenticated lookup proves selected-object integrity and absence without reading unrelated historical payloads;
- repair rejects damaged sources and validates new output;
- page alternatives are measured through 100 million objects;
- the implementation compiles at the provisional MSRV, on 32-bit little-endian, and on 64-bit big-endian targets;
- continuous fuzzing covers strict parsing, recovery, writers, lookup, range-source lookup, and rewrite output in addition to inherited and model targets.

### Still required before Review

- pinned invalid-vector corpus with independent expected outcomes;
- copy-on-write append vectors and amplification measurements;
- checkpoint vectors;
- remote-range benchmarks and stable-view experiments;
- second independently maintained implementation or review;
- resolution of all questions above that prevent independent implementation.

## Registry allocations requested

No permanent allocations are requested while the FCP is Draft. Candidate values are experimental and local to `UCOF-EXP-0002`.

## Rollout plan

1. Keep Candidate 1 implementation in unpublished `ucof-experiments`.
2. Continue publishing exact vectors, rejected alternatives, and security findings.
3. Do not describe generated files as durable or stable.
4. Retire or revise the candidate when evidence invalidates a byte choice.
5. Move this FCP to Review only when unresolved implementation-blocking questions are closed and independent evidence exists.

## Rejection or rollback strategy

If Candidate 1 fails, stop emitting its bytes, preserve the corpus for regression and historical analysis, mark the candidate retired, and introduce a new explicit experimental candidate or epoch. Do not silently reinterpret existing files.

## References

- `docs/spec/EXP_0002_BYTE_CANDIDATE.md`
- `docs/decisions/0009-isolate-phase3-research-models.md`
- `docs/decisions/0010-exp0002-first-byte-candidate.md`
- `docs/decisions/0011-exp0002-snapshot-and-commit-identity.md`
- `docs/experiments/0005-directory-model-comparison.md`
- `docs/experiments/0006-exp0002-page-size-comparison.md`
- `docs/security/EXP_0002_MODEL_FINDINGS.md`
- `docs/security/EXP_0002_BYTE_FINDINGS.md`
- `docs/THREAT_MODEL.md`
- `docs/USE_CASES.md`
- `docs/PHASE_3_STATUS.md`
- `tests/vectors/exp-0002/manifest.json`

## Decision record

Completed by maintainers when the proposal is decided.

- **Decision:** Pending
- **Decision date:**
- **Review period:**
- **Approvers:**
- **Blocking objections and disposition:**
- **Required follow-up:**
