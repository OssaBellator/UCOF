# Phase 2 Status — Safety-First Core Codec

**Status:** In progress; bounded-source increment passing, sequential increment under validation  
**Started:** 2026-07-29  
**Working branch:** `phase-2/safety-first-core`  
**Stacked pull request:** #2  
**Depends on:** Phase 1 pull request #1

## Objective

Convert the Phase 1 experiment into a maintainable, hostile-input-resistant core library without freezing `UCOF-EXP-0001` bytes.

## Implemented increments

| Deliverable | Status | Evidence |
|---|---|---|
| Synchronous random-access source contract | Implemented and tested | `ReadAt` in `ucof-core::source` |
| Borrowed slice adapter | Implemented and tested | `SliceSource` |
| Seekable source adapter | Implemented and tested | `SeekSource<R>` |
| Cumulative read accounting | Implemented and tested | `ReadStats` and `max_total_bytes_read` |
| Single-allocation bound | Implemented | `max_allocation_bytes` |
| Future-work limit fields | Added | logical bytes, dependencies, diagnostics, transform expansion |
| Metadata-only inspection | Implemented and tested | `MetadataInspector` |
| Explicit metadata integrity status | Implemented | `IntegrityStatus::NotChecked` |
| Directory-to-header cross-check | Implemented and tested | source-backed inventory validation |
| Unsupported required capability reporting | Implemented without false conformance claim | `InspectionReport` |
| Payload-skip test | Passing | `tests/bounded_source.rs` |
| Slice/seek equivalence test | Passing | `tests/bounded_source.rs` |
| Read-budget failure test | Passing | `tests/bounded_source.rs` |
| Record-header corruption test | Passing | `tests/bounded_source.rs` |
| Bounded sequential event reader | Implemented; CI validation in progress | `SequentialReader<R>` |
| Owned payload chunk ceiling | Implemented | `max_stream_chunk_bytes` |
| Incremental committed-prefix hashing | Implemented | `StreamStats::bytes_hashed` |
| Verified sequential commit event | Implemented | `StreamEvent::Commit`, `IntegrityStatus::Verified` |
| Exact-end stream validation | Implemented | trailing-byte probe after footer |
| Sequential reader tests | Added; CI validation in progress | `tests/sequential_reader.rs` |
| Random-access I/O decision | Accepted | `docs/decisions/0003-bounded-synchronous-read-at.md` |
| Sequential event decision | Accepted | `docs/decisions/0004-bounded-sequential-events.md` |
| Full inherited CI suite | Passing before sequential increment | formatting, clippy, Rust, Python, adversarial corpus, experiments |

## Safety properties demonstrated

- source length is checked before fixed-range reads;
- offsets and lengths use checked arithmetic;
- cumulative reads fail before exceeding caller policy;
- allocations fail before using an untrusted declared length;
- opaque payload bodies are not read during metadata-only inspection;
- physical record headers are cross-checked against directory claims;
- metadata-only reports explicitly state that payload integrity was not checked;
- unknown required capabilities are visible and prevent a fully-interpretable status without blocking structural inventory;
- slice-backed and seek-backed sources produce equivalent reports;
- a malformed physical record identity cannot be hidden by unchanged directory metadata;
- sequential opaque payloads are emitted in caller-bounded chunks rather than accumulated;
- manifest and directory payloads are retained only after metadata and allocation checks;
- the committed prefix is hashed as bytes are consumed;
- no sequential commit event is emitted before directory, manifest, digest, and exact-end checks pass;
- stream truncation, digest mismatch, trailing bytes, and logical-byte exhaustion have direct tests;
- iterator errors are intended to be terminal rather than repeating indefinitely.

## Important limitations

- both source and sequential APIs are synchronous;
- random-access record headers are currently read individually rather than batched;
- full random-access source integrity validation still needs to hash payload bodies;
- the sequential reader currently owns one allocation per payload event;
- diagnostic and salvage APIs have not yet been implemented;
- streaming and seek-optimized writer APIs remain pending;
- property testing, fuzz targets, and sparse-file fixtures remain pending;
- `UCOF-EXP-0001` still has the UC-02 flat-directory limitation recorded in Phase 1.

## Next increments

1. Complete CI validation and correction of the sequential event reader.
2. Add strict source-backed validation with separate bytes-read and bytes-hashed accounting.
3. Add bounded diagnostic reports that never upgrade damaged state to valid state.
4. Add streaming and seekable writer finalization APIs.
5. Add property tests, fuzz target compilation, corpus expansion, and documentation examples.
6. Add a sparse-source test proving metadata inventory does not scale with payload length.

## Exit rule

Phase 2 is complete only when the documented hostile-input APIs, fuzzing, property tests, sparse metadata inspection, and error-model distinctions are implemented and continuously tested. No Phase 2 API makes the experimental wire format stable.
