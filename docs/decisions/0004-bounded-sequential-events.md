# ADR-0004: Use bounded owned chunks for sequential event reading

- **Status:** Accepted
- **Date:** 2026-07-30
- **Owners:** UCOF maintainers
- **Related issues:** None yet
- **Related FCPs:** FCP-0001
- **Supersedes:** None
- **Superseded by:** None

## Context

Phase 2 requires a non-seeking reader for stream-compatible files. The API must expose record structure and payload progress without loading complete opaque payloads, borrowing across repeated reader calls, or claiming a valid commit before the trailing footer and digest have been checked.

The Phase 1 format is written in this order:

1. fixed file header;
2. contiguous records;
3. final directory record;
4. exact-end footer.

This permits one-pass reading, but full commit validation is available only after the directory and footer arrive. Manifest and directory metadata must be retained for validation, while opaque payloads may be arbitrarily larger than a caller's preferred working buffer.

This ADR is implementation-local. It does not change `UCOF-EXP-0001` bytes and does not require independent implementations to use Rust-style events.

## Decision

The Rust reference library introduces `SequentialReader<R: Read>` with a pull-based event API:

- `FileHeader` after bootstrap validation;
- `RecordStart` with physical record metadata;
- one or more owned `PayloadChunk` events;
- `RecordEnd` after the declared payload is consumed and any core metadata is parsed;
- `Commit` only after directory, manifest, footer, digest, and exact-end checks succeed.

Payload chunks are owned `Vec<u8>` values bounded by both:

- `Limits::max_stream_chunk_bytes`;
- `Limits::max_allocation_bytes`.

Opaque payloads are never accumulated internally. Manifest and directory payloads are accumulated only after their declared lengths pass metadata and allocation limits.

The reader hashes every byte before the footer as it is consumed. Footer bytes are excluded from the committed-prefix digest. A `Commit` event carries `IntegrityStatus::Verified`, roots, unsupported required capabilities, and final read/hash statistics.

Unknown required capabilities do not prevent structural streaming. They make the final commit report not fully interpretable. Callers that require complete semantic support must check `StreamCommit::is_fully_interpretable()`.

Any parse, limit, integrity, or I/O error makes iteration terminal. The reader must not emit the same error indefinitely.

## Decision drivers

- bounded memory for large payloads;
- no seeking or full-file buffering;
- exact accounting of bytes read, hashed, and logically exposed;
- explicit distinction between record progress and committed validity;
- deterministic failure on truncation and trailing data;
- a simple synchronous API that is easy to reproduce independently.

## Alternatives considered

### Borrowed payload slices

Borrowing directly from an internal buffer can avoid one allocation per event, but it complicates repeated calls, trait-object use, and adapters whose underlying `Read` implementation fills temporary buffers. A future callback or reusable-buffer API may optimize copies without changing event semantics.

### Caller-provided payload buffer

This minimizes allocations but makes the first API more stateful and error-prone. The owned-chunk API establishes semantics first. A lower-level `read_payload_into` interface may be added later.

### Buffer each complete record

This is simple for consumers but violates bounded-memory goals for large opaque payloads and duplicates the Phase 1 in-memory approach.

### Emit a commit before digest verification

Rejected. A footer-shaped event without verified digest and exact-end checks would invite callers to confuse discovery with validity.

### Reject unknown required capabilities immediately

Rejected for structural enumeration. The stream remains parseable, but the final report explicitly states that it is not fully interpretable.

## Consequences

### Positive

- opaque payload memory is independent of payload length;
- stream consumers receive explicit progress and remaining-byte counts;
- truncated payloads are categorized at the point of consumption;
- the final commit has verified committed-prefix integrity;
- metadata validation uses the same caller-controlled limits as other readers;
- event consumers never need seek support.

### Negative

- each payload event currently owns an allocation;
- the reader retains all directory entries for `UCOF-EXP-0001`, preserving the Phase 1 flat-directory scaling limitation;
- metadata records are both emitted as chunks and retained internally;
- synchronous `Read` does not directly model asynchronous network streams;
- payload application semantics remain outside the core reader.

## Security implications

- chunk size, metadata size, allocations, records, total bytes, logical bytes, and file bytes are bounded before work;
- all offsets and counters use checked arithmetic;
- no commit event is emitted before digest and exact-end verification;
- a malicious directory cannot rewrite the streamed physical inventory;
- trailing bytes are rejected rather than searched for another root;
- iterator errors are terminal to prevent unbounded repeated diagnostics.

## Follow-up work

- add source-backed full integrity validation with separate bytes-read and bytes-hashed accounting;
- add a reusable-buffer or callback API if profiling shows owned chunks are costly;
- add property tests for chunk-boundary independence;
- add fuzz targets for event-state transitions, truncation, and valid-vector mutation;
- design bounded diagnostic and salvage APIs that never emit `Commit` for damaged input;
- evaluate async adapters that preserve the same state and limit semantics.
