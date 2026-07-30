# Phase 3 Status — Directory, Snapshots, and Recovery

**Status:** In progress; EXP-0002 Candidate 1 is executable, extensively tested, and still disposable  
**Started:** 2026-07-30  
**Working branch:** `phase-3/directory-snapshots-recovery`  
**Stacked pull request:** #3  
**Depends on:** Phase 2 pull request #2

## Objective

Deliver bounded random access, append publication, snapshots, previous-root recovery, repair, and compaction while preserving the rule that damaged, recovered, historical, or merely plausible state never silently becomes active valid state.

## Current experimental decisions

- Phase 3 uses disposable epoch `UCOF-EXP-0002` because directory and active-root semantics change validity.
- Candidate 1 exact bytes are defined in `docs/spec/EXP_0002_BYTE_CANDIDATE.md`.
- Candidate 1 uses little-endian fixed fields, 16 KiB authenticated pages, fixed binary entries, domain-separated SHA-256 digests, variable-length snapshot records, and 160-byte exact-end footers.
- Strict validation is exact-end and never invokes recovery implicitly.
- Recovery is explicitly requested and independently bounds suffix bytes, scan reads, footer-magic matches, candidate validations, cumulative successful and failed candidate reads, results, and chain depth.
- Structural snapshot identity and file-instance commit identity are separate scopes under ADR-0011.
- Candidate 1 checkpoints are ordinary complete snapshot commits under ADR-0012; no weaker progress-checkpoint bytes are defined.
- Mutable or remote range sources require a caller-provided strong stable-view token under ADR-0013. Stable view is not trusted freshness.
- Repair and caller-selected rewrite accept only fully verified sources and always publish a new commit identity.
- All concrete Candidate 1 APIs remain in unpublished `ucof-experiments`; no stable compatibility promise exists.

## Implemented frontiers

### Proposal, specification, and decisions

| Frontier | Status | Evidence |
|---|---|---|
| EXP-0002 scope and invariants | Draft | FCP-0002 |
| Independently implementable Candidate 1 bytes | Implemented | provisional byte specification |
| Research isolation | Accepted | ADR-0009 |
| First byte candidate | Accepted experimentally | ADR-0010 |
| Snapshot versus commit identity | Accepted experimentally | ADR-0011 |
| Complete-only checkpoints | Accepted experimentally | ADR-0012 |
| Versioned stable-view source adapter | Accepted implementation policy | ADR-0013 |
| Strict versus recovery separation | Specified and implemented | source and slice APIs |
| Security findings | Published | model, byte, and threat-model documents |

### Concrete codec

| Frontier | Status | Evidence |
|---|---|---|
| 64-byte bootstrap header | Implemented and tested | Rust and Python codecs |
| 48-byte opaque object records | Implemented and tested | deterministic writers and strict readers |
| 16 KiB authenticated leaf/internal pages | Implemented and tested | multi-leaf vectors and corruption tests |
| 88-byte leaf entries | Implemented as Candidate 1; under comparison | exact vectors and Experiment 0010 |
| 64-byte internal entries | Implemented and tested | exact vectors |
| Variable snapshot record | Implemented and tested | roots and capability arrays |
| 160-byte commit footer | Implemented and tested | exact-end validation |
| Domain-separated object/page/snapshot/commit digests | Implemented and tested | mutation and adversarial cases |
| Genesis writer | Deterministic | Rust/Python byte equality |
| Append writer | Deterministic | parent-linked append vector |
| Strict slice validator | Implemented and bounded | `validate_strict` |
| Strict random-access source validator | Implemented and bounded | `validate_strict_at` |
| Reserved-byte, padding, and physical-overlap rejection | Implemented | adversarial and corpus tests |

### Valid and invalid vector corpora

The valid corpus under `tests/vectors/exp-0002` contains:

| Vector | Purpose |
|---|---|
| `genesis-two-object` | deterministic genesis with root and non-root object |
| `append-add-third` | parent-linked append reusing historical object records |
| `multi-leaf-400` | authenticated multi-leaf directory and internal root |

For every valid vector, Python writes and verifies the canonical bytes, Rust rebuilds the same bytes exactly, and both strict validators accept the stored file.

The public invalid corpus under `tests/vectors/exp-0002-invalid` contains thirteen deterministic files covering bootstrap, object, page, physical-layout, snapshot, parent-chain, exact-end, and interrupted-publication failures. Python verifies corpus reproducibility; Rust independently rejects every file. Coarse diagnostic layers are recorded, while exact exception strings remain implementation-local.

### Authenticated lookup and source access

| Frontier | Status | Evidence |
|---|---|---|
| Root-to-leaf authenticated lookup | Implemented | slice and source APIs |
| Authenticated absence | Implemented and tested | missing-key tests |
| Historical object rehash after append | Implemented and tested | mutation tests |
| Page, hash, read, and request limits | Implemented | source limits |
| Structural-range overlap rejection | Implemented | targeted range isolation |
| Full source strict validation | Implemented and corpus-bound | `validate_strict_at` |
| Stable-view version adapter | Implemented and tested | `Exp0002StableSource` |

Targeted lookup authenticates the active commit, snapshot, one page path, and selected object. It does not claim unrelated historical objects were rehashed.

A localhost HTTP Range benchmark over an append file with an unrelated 1 MiB historical object measured:

| Assurance mode | Requests | Bytes transferred | Pages | Objects hashed |
|---|---:|---:|---:|---:|
| Targeted lookup | 7 | 16,993 | 1 | 1 |
| Full strict validation | 26 | 1,065,673 | 1 | 3 |

Targeted lookup made no request overlapping the large historical object. Full validation read it.

### Append, history, and recovery

| Frontier | Status | Evidence |
|---|---|---|
| Footer-only publication | Implemented | append writer and publication model |
| Every incomplete latest footer rejected by strict mode | Implemented and tested | every-cut and pinned-cut tests |
| Bounded backward candidate discovery | Implemented | hardened source recovery facade |
| Failed candidate work charged | Implemented and tested | cumulative source-read accounting |
| Candidate magic has no authority | Implemented | strict-prefix validation |
| Verified linked-history enumeration | Implemented | `enumerate_previous_chain_at` |
| Root and identity reporting per verified prefix | Implemented | recovery and history reports |
| Candidate-storm and scan-window limits | Implemented and tested | recovery tests and fuzzing |
| Complete checkpoints | Implemented as normal commits | ADR-0012 and cadence experiment |
| Separate progress-checkpoint bytes | Deferred | no Candidate 1 allocation |

Linked-history enumeration validates the exact-end active file and every referenced ancestor as an independent strict prefix. It cross-checks previous-footer offsets, parent snapshot digests, and exact sequence increments while bounding depth and cumulative source reads.

### Repair and rewrite

| Frontier | Status | Evidence |
|---|---|---|
| Abstract reachability and orphan planning | Implemented | graph model |
| Verified-source repair to new file | Implemented | `repair_all_to_new_file` |
| Caller-directed object-selection rewrite | Implemented | `rewrite_selected_to_new_file` |
| Object, payload, and output-byte limits | Implemented | rewrite limits |
| Damaged-source rejection | Implemented and tested | strict source gate |
| Root-retention checks | Implemented and tested | rewrite tests |
| Snapshot/commit identity report | Implemented | ADR-0011 fields |
| Byte-scoped signature non-preservation | Explicit | rewrite reports |
| Automatic semantic dependency discovery | Pending | requires schemas, profiles, or supplied graph |

A deterministic genesis repair may preserve structural snapshot identity while always changing file-instance commit identity. Append repair becomes a new genesis and changes both scopes.

### Experimental CLI

The separate `ucof-exp0002` binary exposes distinct commands:

- `verify` — full exact-end validation;
- `roots` — active roots after full validation;
- `history` — exact linked history, each ancestor strictly validated;
- `lookup` — targeted authenticated object or absence;
- `recover` — bounded discovery of strict prefixes without candidate selection;
- `repair-all` — verified source to new genesis output;
- `rewrite-selected` — caller-selected output without a semantic-compaction claim.

End-to-end tests exercise pinned vectors, history, recovery, rewrite output validation, and create-new output protection. Assurance boundaries are documented in `docs/PHASE_3_CLI_GUIDE.md`.

## Architectural findings

### Candidate 1 page sequence prevents historical page reuse

The persistent COW model proves the desired O(depth) update algorithm, but Experiment 0011 demonstrates that Candidate 1 bytes prohibit exact historical page reuse. Every page stores and authenticates the active snapshot sequence, and validation requires equality with the current snapshot. Unchanged leaves therefore must still be copied, causing new digests to propagate through every ancestor.

This is a Candidate 1 byte-design blocker, not merely a missing writer optimization. A later candidate must revise page identity or intentionally accept full-directory rewrite amplification.

### Page-size and locator-width results

At 100 million objects with Candidate 1 88-byte leaves:

| Page size | Depth | Directory bytes | Authenticated path bytes |
|---:|---:|---:|---:|
| 4 KiB | 5 | 9,249,042,432 | 20 KiB |
| 16 KiB | 4 | 8,891,121,664 | 64 KiB |
| 64 KiB | 3 | 8,817,344,512 | 192 KiB |

At 16 KiB pages:

| Leaf layout | 100M directory size | Depth |
|---|---:|---:|
| 88-byte Candidate 1, 64-bit ID | 8.280 GiB | 4 |
| 72-byte no-reserve, same fields | 6.778 GiB | 4 |
| 56-byte minimal authenticated, 64-bit ID | 5.264 GiB | 4 |
| 64-byte minimal authenticated, 128-bit ID | 6.007 GiB | 4 |

The 16-byte per-entry reserve alone costs roughly 1.50 GiB at this scale. Final layout selection requires range-request and inventory measurements, not directory size alone.

### Checkpoint strategy crossover

Complete-only checkpoint evidence shows no universal writer strategy:

- frequent checkpoints make repeated full-directory rebuilds dominant;
- sparse checkpoints can make naive per-object path copying more expensive than one final rebuild;
- a future reusable-page writer must batch changes, share ancestors, and serialize only final reachable pages.

### Bounded external sort is feasible

Experiment 0013 deterministically sorts 200,003 exact 88-byte locator-shaped entries using sub-megabyte spill runs and a k-way merge. Two run sizes produce identical output; missing and duplicate keys fail closed. Byte-producing writer integration and secure spill policy remain pending.

## Continuous verification

The branch passes:

- locked dependency resolution;
- stable Rust formatting and clippy with warnings denied;
- all workspace unit, integration, CLI, and documentation tests;
- independent EXP-0001 validation and adversarial corpus;
- independent Phase 3 model corpus;
- independent EXP-0002 codec self-tests and valid-vector verification;
- the thirteen-file invalid/interrupted corpus;
- 21 layer-targeted adversarial mutations;
- all framing, footer, scale, directory, page-size, COW, checkpoint, locator-width, page-reuse, HTTP-range, and external-sort experiments;
- Rust 1.85 MSRV compilation;
- 32-bit `i686-unknown-linux-gnu` library compilation;
- 64-bit big-endian `powerpc64-unknown-linux-gnu` library compilation;
- twenty-one cargo-fuzz target builds and bounded pull-request smoke campaigns.

The twenty-one fuzz targets consist of six inherited Phase 2 byte targets, seven Phase 3 algorithm-model targets, and eight concrete EXP-0002 targets covering strict parsing, recovery, writer round trips, slice lookup, rewrite, source lookup, source strict/recovery, and source history.

All permanent workflows use read-only repository permissions.

## Current limitations and blockers

- Candidate 1 page-sequence semantics prohibit exact historical page reuse.
- The append writer rebuilds all directory pages.
- The final page identity, leaf layout, and object-identifier width are unresolved.
- Full source readers are synchronous.
- `Exp0002StableSource` requires transport-provided strong version evidence; concrete conditional HTTP/cloud adapters are not implemented.
- Rewrite commands currently materialize source and output in memory.
- The bounded external sort is a model, not yet integrated into the byte writer.
- Automatic semantic dependency discovery is unavailable without profiles or schemas.
- Default resource limits remain implementation-local; normative minima are unresolved.
- Capability identifiers are structurally encoded, but Candidate 1 defines no non-zero allocation.
- No authenticity, signature, provenance, encryption, metadata confidentiality, or external trusted freshness exists.
- Replacing the entire file with an older valid copy remains undetectable without trusted external state.
- Both current byte implementations live in one repository and may share a specification misunderstanding.

## Next frontier tasks

1. Design and test revised immutable-page or page-birth-generation semantics that permit safe reuse.
2. Implement a deterministic batched byte-level page-reuse writer for the revised semantics.
3. Integrate bounded external sorting with object emission, page packing, failure cleanup, and final publication.
4. Prototype a concrete conditional HTTP or cloud-object adapter using strong version evidence.
5. Benchmark cold-cache inventory workloads for 88-, 72-, 56-, and 64-byte locator alternatives.
6. Decide object-identifier width and leaf layout.
7. Define normative minimum limits versus caller policy.
8. Define future-field and capability-preservation rules.
9. Define profile-supplied dependency and history-retention inputs for semantic compaction.
10. Obtain an independently maintained implementation or external independent review.
11. Resolve FCP-0002 objections before moving the proposal to Review.

## Exit rule

Phase 3 is not complete until a selected experimental layout demonstrates bounded source-based lookup and validation, append publication, safe page reuse or an explicitly accepted alternative, previous-root recovery, linked-history enumeration, unambiguous active-root rules, repair, and compaction with cross-language valid and invalid vectors, hostile-input evidence, continuous fuzzing, realistic range-I/O measurements, deterministic large-writer strategy, documented rejected alternatives, independent review, and maintainer disposition of FCP-0002 blockers.
