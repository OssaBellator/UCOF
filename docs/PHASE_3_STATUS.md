# Phase 3 Status — Directory, Snapshots, and Recovery

**Status:** In progress; first concrete EXP-0002 byte candidate implemented and passing CI  
**Started:** 2026-07-30  
**Working branch:** `phase-3/directory-snapshots-recovery`  
**Stacked pull request:** #3  
**Depends on:** Phase 2 pull request #2

## Objective

Deliver bounded random access, append publication, snapshots, previous-root recovery, repair, and compaction while preserving the rule that damaged, recovered, or merely plausible state never silently becomes valid state.

## Current experimental decisions

- Phase 3 uses disposable epoch `UCOF-EXP-0002` because directory and active-root semantics change validity.
- Candidate 1 has exact provisional bytes defined in `docs/spec/EXP_0002_BYTE_CANDIDATE.md`.
- The candidate uses little-endian fixed fields, 16 KiB authenticated pages, fixed binary entries, domain-separated SHA-256 digests, variable-length snapshot records, and 160-byte exact-end footers.
- Strict validation remains exact-end and never silently invokes recovery.
- Recovery is explicit and independently bounds scan bytes, magic matches, candidate validations, results, and chain depth.
- The flat EXP-0001 directory is not a promotion candidate.
- Ordered pages remain experimental; 4 KiB and 64 KiB alternatives retain measured evidence.
- Structural snapshot identity and file-instance commit identity are separate scopes under ADR-0011.
- Repair and compaction accept only strictly verified complete snapshots and always publish a new commit identity.
- All concrete byte APIs remain in unpublished `ucof-experiments`; no stable compatibility promise exists.

## Implemented frontiers

### Proposal, specification, and decisions

| Frontier | Status | Evidence |
|---|---|---|
| EXP-0002 scope and invariants | Drafted | FCP-0002 |
| First independently implementable bytes | Implemented | provisional byte specification |
| Research isolation | Accepted | ADR-0009 |
| First byte candidate | Accepted experimentally | ADR-0010 |
| Snapshot versus commit identity | Accepted experimentally | ADR-0011 |
| Strict versus recovery separation | Specified and implemented | strict and recovery APIs |
| Security findings | Model and concrete-byte findings published | security documents |

### Concrete file, object, page, snapshot, and footer codec

| Frontier | Status | Evidence |
|---|---|---|
| 64-byte bootstrap header | Implemented and tested | Rust and Python codecs |
| 48-byte opaque object records | Implemented and tested | deterministic writers and strict readers |
| 16 KiB authenticated leaf/internal pages | Implemented and tested | multi-leaf vectors and corruption tests |
| 88-byte leaf entries | Implemented and tested | exact vector agreement |
| 64-byte internal entries | Implemented and tested | exact vector agreement |
| Variable snapshot record | Implemented and tested | roots and capability-array parsing |
| 160-byte commit footer | Implemented and tested | exact-end validation |
| Domain-separated object/page/snapshot/commit digests | Implemented and tested | mutation and adversarial cases |
| Genesis writer | Implemented and deterministic | Rust/Python equality |
| Append writer | Implemented and deterministic | parent-linked append vector |
| Strict exact-end validator | Implemented and bounded | `validate_strict` |
| Reserved-byte and zero-padding enforcement | Implemented | adversarial corpus |
| Physical overlap rejection | Implemented | strict and targeted readers |

### Cross-language vectors

The pinned corpus under `tests/vectors/exp-0002` contains:

| Vector | Purpose |
|---|---|
| `genesis-two-object` | deterministic genesis with root and non-root object |
| `append-add-third` | parent-linked append reusing historical objects |
| `multi-leaf-400` | authenticated multi-leaf directory and internal root |

For every valid vector:

- Python writes the canonical bytes;
- Python verifies the stored bytes and manifest hashes;
- Rust rebuilds the same file byte-for-byte;
- Rust strictly validates the stored file.

### Authenticated lookup

| Frontier | Status | Evidence |
|---|---|---|
| Root-to-leaf authenticated path | Implemented | `lookup_authenticated` |
| Authenticated absence result | Implemented and tested | missing-key tests |
| Selected historical object rehash after append | Implemented and tested | old-object mutation test |
| Page-read and hash-work limits | Implemented | lookup limits |
| Page/snapshot/footer overlap rejection | Implemented | targeted range isolation |
| Pinned-vector lookup tests | Implemented | multi-leaf and append integration tests |
| Random-access source without full slice | Implemented and tested | `lookup_authenticated_at` |

The targeted lookup verifies the active commit, snapshot, one page path, and selected object. It does not claim that unrelated historical object records were rehashed. The range-source implementation streams commit and object hashing under read-operation, byte, request-size, page, and hash budgets; a test proves that lookup of a small historical root does not read an unrelated one-megabyte historical payload. The range-source implementation streams commit and object hashing under read-operation, byte, request-size, page, and hash budgets; a test proves that lookup of a small historical root does not read an unrelated one-megabyte historical payload.

### Concrete append and recovery

| Frontier | Status | Evidence |
|---|---|---|
| Footer-only publication | Implemented | append writer and publication model |
| Every incomplete latest footer rejected by strict mode | Implemented and tested | every-cut append tests |
| Bounded backward candidate discovery | Implemented | `scan_valid_prefixes` |
| Candidate magic has no authority | Implemented | strict-prefix candidate validation |
| Previous-footer chain enumeration | Implemented | `enumerate_previous_chain` |
| Candidate-storm limits | Implemented and tested | recovery tests and fuzz target |
| Scan-window non-guessing | Implemented and tested | bounded-window test |
| Concrete progress checkpoint bytes | Pending | candidate 1 defines complete snapshots only |

### Repair and compaction

| Frontier | Status | Evidence |
|---|---|---|
| Abstract reachability and orphan planning | Implemented | Phase 3 graph model |
| Verified-source repair to new file | Implemented | `repair_all_to_new_file` |
| Caller-directed object-selection rewrite | Implemented | `rewrite_selected_to_new_file` |
| Object, payload, and output-byte limits | Implemented | rewrite limits |
| Damaged-source rejection | Implemented and tested | strict source gate |
| Root-retention checks | Implemented and tested | rewrite tests |
| Snapshot/commit identity report | Implemented | ADR-0011 fields |
| Byte-scoped signature non-preservation | Explicit | rewrite report |
| Automatic semantic dependency discovery | Pending | requires schemas/profiles or supplied graph |
| Copy-on-write page reuse | Pending | writer currently rebuilds the full directory |

A deterministic genesis repair may preserve the structural snapshot digest while always changing the file-instance commit digest. Append repair becomes a new genesis and changes both scopes.

### Independent and adversarial evidence

- independent Python model fixtures cover directory shapes, selection, recovery, forks, checkpoints, compaction, and cycles;
- independent Python concrete codec implements candidate 1 without importing Rust;
- 21 layer-targeted adversarial cases mutate headers, objects, pages, padding, child links, snapshots, parents, footers, exact-end state, and append truncations;
- outer digests are recomputed where necessary so mutations reach deeper validation layers;
- every-cut Rust tests cover interrupted append publication;
- page-size Experiment 0006 compares 4 KiB, 16 KiB, and 64 KiB pages using the actual entry widths;
- Experiment 0005 retains ordered-tree, sorted-array, and deterministic-hash alternatives.

## Continuous verification

The concrete branch passes:

- locked dependency resolution;
- stable Rust formatting and clippy with warnings denied;
- all workspace unit, integration, and documentation tests;
- independent EXP-0001 parser and adversarial corpus;
- independent Phase 3 model corpus;
- independent EXP-0002 codec self-tests and stored-vector verification;
- the EXP-0002 layer-targeted adversarial corpus;
- all reproducible framing, footer, scale, directory-model, and page-size experiments;
- Rust 1.85 MSRV compilation;
- 32-bit `i686-unknown-linux-gnu` library compilation;
- big-endian `powerpc64-unknown-linux-gnu` library compilation;
- nineteen cargo-fuzz target builds and bounded pull-request smoke campaigns.

The nineteen fuzz targets consist of six inherited Phase 2 byte targets, seven Phase 3 algorithm-model targets, and six concrete EXP-0002 targets for strict validation, recovery, writer round trips, in-memory lookup, range-source lookup, and rewrite output.

All permanent workflows use read-only repository permissions.

## Measured page-size finding

At 100 million objects with the candidate entry widths:

| Page size | Tree depth | Directory bytes | Authenticated path bytes |
|---:|---:|---:|---:|
| 4 KiB | 5 | 9,249,042,432 | 20 KiB |
| 16 KiB | 4 | 8,891,121,664 | 64 KiB |
| 64 KiB | 3 | 8,817,344,512 | 192 KiB |

The provisional 16 KiB page is a middle point, not an accepted constant. The 88-byte leaf entry dominates total directory size; reducing entry width may provide more value than increasing page size.

## Properties demonstrated

### Validity and integrity

- header, footer, snapshot, page, and object ranges use checked arithmetic;
- reserved bytes and page padding must be zero;
- object, page, snapshot, and commit digests have separate domains;
- directory claims are cross-checked against referenced pages and object headers;
- page levels, ranges, ordering, fanout, digests, and cycles fail closed;
- required exact-end state is published only after a complete footer;
- targeted lookup cannot treat a structural page or footer range as an object record;
- repair cannot operate on a damaged source.

### Bounded work

- file, commit, snapshot, page, depth, object, payload, root, capability, hash, lookup, recovery, rewrite, and output limits are caller controlled;
- recovery candidate storms fail under configured limits;
- lookup reads one page per path level after commit and snapshot verification;
- rewrite bounds object count and copied/output bytes before accepting output.

### Identity and recovery

- snapshot digest identifies exact authenticated snapshot structure;
- commit digest identifies one published file-instance commit;
- parent snapshot digest and previous-footer locator are cross-checked;
- interrupted append tails do not replace an earlier complete root;
- repair always produces a new commit identity;
- no freshness or authenticity claim is inferred from hashes or sequence numbers.

## Current limitations

- full strict validation, backward recovery scanning, and rewrite currently operate on in-memory byte slices; authenticated lookup also has a bounded range-source implementation;
- strict and targeted readers are synchronous, and range-source validation assumes one stable source view for the operation;
- the current append writer rebuilds all directory pages;
- historical object records may be referenced by later snapshots, but no page reuse exists yet;
- capability identifiers are structurally encoded, but candidate 1 defines no non-zero capability allocation;
- progress checkpoint bytes are undefined;
- automatic dependency discovery for compaction is unavailable without profile/schema semantics;
- independent invalid vectors are generated through mutation tests but are not yet pinned as separate files;
- no HTTP-range, cold-cache, or real-storage benchmark exists;
- no authenticity, signature, provenance, encryption, metadata confidentiality, or external freshness mechanism exists;
- replacement of the whole file with an older valid copy remains undetectable without external state.

## Completed tasks from the previous frontier list

- first candidate bootstrap, object, page, snapshot, and footer bytes are specified;
- Rust and independent Python writers/readers exist;
- deterministic cross-language vectors are pinned;
- page sizes are compared using concrete entry widths;
- concrete exact-end validation and previous-root recovery exist;
- authenticated single-object lookup and absence proof exist over both slices and bounded random-access sources;
- repair-to-new-file and object-selection rewrite output exist;
- concrete parsers, slice and range-source lookup, recovery, writers, and rewrite paths are continuously fuzzed;
- concrete-byte adversarial findings are executable in CI;
- snapshot and commit identity scopes are resolved experimentally.

## Next frontier tasks

1. move full strict validation and recovery scanning onto bounded random-access sources without materialising the whole file;
2. pin invalid and interrupted-append byte vectors with expected failure categories;
3. implement copy-on-write page reuse and measure append rewrite amplification;
4. define complete-checkpoint bytes and evaluate whether progress checkpoints are justified;
5. add HTTP-range, cold-cache, and realistic object-count benchmarks;
6. evaluate narrower leaf locators and alternative object-identity widths;
7. define CLI assurance surfaces for root enumeration, recovery, repair, and compaction;
8. obtain a second independently maintained implementation rather than only an independent in-repository Python implementation;
9. resolve remaining FCP-0002 questions before moving the proposal to Review.

## Exit rule

Phase 3 is not complete until the selected experimental layout demonstrates bounded source-based lookup, append publication, previous-root recovery, unambiguous root selection, valid-root enumeration, repair, and compaction with cross-language valid and invalid vectors, hostile-input evidence, continuous fuzzing, realistic range-I/O measurements, documented rejected alternatives, and maintainer review of FCP-0002.
