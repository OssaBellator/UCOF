# Phase 2 Status — Safety-First Core Codec

**Status:** Technical implementation complete; stacked review in progress  
**Started:** 2026-07-29  
**Working branch:** `phase-2/safety-first-core`  
**Stacked pull request:** #2  
**Depends on:** Phase 1 pull request #1

## Objective

Convert the Phase 1 experiment into a maintainable, hostile-input-resistant core library without freezing `UCOF-EXP-0001` bytes.

## Implemented deliverables

### Bounded reader APIs

| Deliverable | Status | Evidence |
|---|---|---|
| Synchronous random-access source contract | Implemented and tested | `ReadAt` |
| Borrowed slice and seekable adapters | Implemented and tested | `SliceSource`, `SeekSource<R>` |
| Metadata-only inspection | Implemented and tested | `MetadataInspector` |
| Strict random-access validation | Implemented and tested | `SourceValidator` |
| Bounded non-seeking event reader | Implemented and tested | `SequentialReader<R>` |
| Separate metadata and verified integrity states | Implemented | `IntegrityStatus` |
| Cumulative source-read accounting | Implemented and tested | `ReadStats`, `SourceValidationStats` |
| Separate bytes-read and bytes-hashed accounting | Implemented and tested | `SourceValidationStats` |
| Bounded payload event chunks | Implemented and tested | `max_stream_chunk_bytes` |
| Exact-end sequential commit validation | Implemented and tested | `StreamEvent::Commit` |
| Directory-to-physical-header cross-check | Implemented and tested | source inventory validation |
| Unsupported required capability reporting | Implemented without false conformance claim | inspection and commit reports |

### Diagnostics and salvage

| Deliverable | Status | Evidence |
|---|---|---|
| Strict categorized diagnostics | Implemented and tested | `DiagnosticValidator` |
| Explicit verified, invalid, and unverified states | Implemented | `DiagnosticStatus` |
| Structurally useful context after integrity failure | Implemented without validity upgrade | `DiagnosticReport` |
| Bounded complete-prefix salvage | Implemented and tested | `PrefixSalvager` |
| Salvage that never claims conformance | Implemented | `UnverifiedPrefix` |
| Diagnostic-count and read limits | Implemented and tested | `max_diagnostics`, `max_total_bytes_read` |
| Assurance-boundary decision | Accepted | ADR-0006 |

### Writer APIs

| Deliverable | Status | Evidence |
|---|---|---|
| Deterministic non-seeking writer | Implemented and tested | `StreamingWriter<W>` |
| Reader-backed bounded payload copying | Implemented and tested | `add_opaque_from_reader` |
| Seekable writer and rewind | Implemented and tested | `SeekableWriter<W>` |
| Explicit finalization | Implemented and tested | `finish`, `finish_and_rewind` |
| Footer publication only after validation | Implemented and tested | writer finalization path |
| Duplicate-identifier prevention | Implemented and tested | streaming writer ledger |
| Terminal state after source or sink failure | Implemented and tested | failed writer checks |
| Byte identity with in-memory writer | Implemented and property tested | writer equivalence tests |
| Finalization decision | Accepted | ADR-0007 |

### Limits and error model

The public `Limits` object covers:

- file bytes;
- cumulative physical bytes read;
- logical decoded bytes;
- records;
- payload and metadata bytes;
- metadata depth and container items;
- text and byte-string lengths;
- dependency depth;
- single allocations;
- diagnostics;
- stream chunks;
- transform expansion ratio reserved for later phases.

Error categories distinguish malformed framing, unsupported epochs or capabilities, invalid metadata, directory mismatch, integrity failure, resource exhaustion, truncation, and I/O failure. Strict diagnostics preserve conceptual categories without exposing unstable wire internals as the public API contract.

### Tests and tooling

| Deliverable | Status | Evidence |
|---|---|---|
| Deterministic writer property tests | Passing | `tests/properties.rs` |
| Truncation property tests | Passing | `tests/properties.rs` |
| Payload-mutation property tests | Passing | `tests/properties.rs` |
| Eight-gibibyte virtual sparse payload inspection | Passing | `tests/sparse_source.rs` |
| Metadata-only 1 MiB payload-skip test | Passing | `tests/bounded_source.rs` |
| Sequential chunk and commit tests | Passing | `tests/sequential_reader.rs` |
| Strict source hash-budget tests | Passing | `tests/source_validator.rs` |
| Diagnostic and salvage tests | Passing | `tests/diagnostics.rs` |
| Streaming and seekable writer tests | Passing | `tests/streaming_writer.rs` |
| CLI assurance-level tests | Passing | `ucof-cli/tests/cli.rs` |
| Compiled API examples | Passing | `ucof-core/examples/` |
| Independent Python parser and corpus | Passing | `tools/validate_exp_0001.py` |
| Adversarial Python cases | Passing | `tools/test_exp_0001_adversarial.py` |
| Six dedicated fuzz targets | Compiling and smoke tested | `fuzz/fuzz_targets/` |
| Scheduled fuzz workflow | Enabled, read-only | `.github/workflows/fuzz.yml` |
| API usage guide | Published | `docs/PHASE_2_API_GUIDE.md` |

The fuzz targets cover:

1. complete files and strict diagnostics;
2. canonical metadata;
3. metadata-only inspection;
4. prefix salvage;
5. sequential event reading;
6. deterministic writer round trips.

### Compatibility and CI

- stable Rust formatting and clippy run with warnings denied;
- all workspace tests and documentation tests run with the committed lockfile;
- the provisional MSRV is Rust 1.85.0 and is checked continuously;
- the core library compiles for `i686-unknown-linux-gnu` to exercise 32-bit arithmetic assumptions;
- the core library compiles for `powerpc64-unknown-linux-gnu` to exercise a big-endian host while retaining little-endian wire integers;
- the independent Python corpus, adversarial cases, and Phase 1 experiments run in the inherited suite;
- dependency resolution is committed in `Cargo.lock` and CI uses `--locked`.

## Safety properties demonstrated

- source length is checked before fixed-range reads;
- offsets and lengths use checked arithmetic;
- cumulative reads fail before exceeding caller policy;
- allocations fail before using untrusted declared lengths;
- metadata inspection skips opaque payload bodies, including an eight-gibibyte virtual range;
- physical record headers are cross-checked against directory claims;
- metadata-only reports explicitly state that payload integrity was not checked;
- unknown required capabilities remain visible and prevent a fully-interpretable status;
- sequential payloads are emitted in caller-bounded chunks rather than accumulated;
- logical-byte exhaustion stops before the next excess byte is consumed;
- no sequential commit event is emitted before directory, manifest, digest, and exact-end checks pass;
- iterator errors are terminal rather than repeating indefinitely;
- strict source validation hashes every committed-prefix byte without allocating complete payloads;
- metadata inspection and bulk hashing share one cumulative read budget;
- footer manifest, count, directory offset, and directory length are rechecked against structural results;
- strict diagnostics never upgrade damaged data to valid data;
- salvage reports only complete ranges and is always unverified;
- streaming writers never publish a footer before explicit successful finalization;
- failed writers are terminal;
- streaming output matches deterministic in-memory output byte for byte.

## Phase 2 exit assessment

The technical exit criteria are satisfied:

- fuzz targets compile and run in pull requests, with a scheduled workflow for continuing campaigns;
- property tests cover deterministic round trips, truncation, and mutation;
- metadata inspection of a multi-gigabyte sparse source does not read payload bodies;
- configured limits stop oversized reads, metadata, allocations, logical bytes, records, and diagnostics;
- no known parser panic or unbounded allocation remains in the malformed corpus;
- documentation distinguishes trusted in-memory convenience APIs from bounded untrusted-input APIs;
- diagnostic and salvage behavior cannot silently become validity;
- formatting, lint, tests, documentation, MSRV, 32-bit, big-endian, independent-parser, adversarial, and experiment checks are continuous.

Phase 2 remains under review because it is stacked on the unmerged Phase 1 proposal. Technical completion does not accept FCP-0001 or stabilize the experimental bytes.

## Important limitations and deferred work

- all current I/O APIs are synchronous;
- strict random-access validation requires a stable source view for one operation;
- the EXP-0001 directory and writer ledger remain flat and materialized;
- random-access record headers are read individually rather than batched;
- source validation intentionally rereads some metadata and the footer;
- sequential payload events own one allocation per event;
- salvage stops at the first fatal framing error and does not resynchronize;
- payload lengths must be known before streaming record headers are written;
- transactional local-file publication and safe rollback are not yet implemented;
- append-only snapshots, checkpoint recovery, root enumeration, repair, and compaction are Phase 3 work;
- transforms, compression, schemas, signatures, provenance, encryption, external references, and profiles remain outside EXP-0001.

## Review and promotion rule

Review should focus on resource accounting, failure-state semantics, source stability, diagnostics versus salvage, writer publication, and whether any public type leaks an accidental wire-layout commitment.

No Phase 2 API or passing test makes the experimental wire format stable. Promotion still requires accepted format proposals, independent implementation evidence, and a later stable-version decision.
