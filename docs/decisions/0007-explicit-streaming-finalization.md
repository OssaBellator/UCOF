# ADR-0007: Publish the active root only through explicit writer finalization

- **Status:** Accepted
- **Date:** 2026-07-30
- **Owners:** UCOF maintainers
- **Related FCPs:** FCP-0001
- **Supersedes:** None
- **Superseded by:** None

## Context

The Phase 1 writer accumulates an entire file in memory. Phase 2 requires deterministic output to ordinary `Write` sinks, bounded copying from payload readers, duplicate-identifier prevention, and a clear point at which a file becomes committed.

A writer that emits a footer before all records, directory metadata, and manifest selection are validated could publish a root for incomplete content. A source read or sink write may also fail after some prefix bytes have already been written.

## Decision

The reference implementation adds `StreamingWriter<W: Write>`.

- the fixed file header is written at construction;
- every record declares its payload length before its header is emitted;
- reader-backed opaque payloads are copied in chunks bounded by `max_stream_chunk_bytes` and `max_allocation_bytes`;
- object identifiers are checked before a second record is emitted;
- manifest payloads are canonicalized before writing;
- the primary directory is generated from the writer's checked record ledger;
- the committed-prefix digest is maintained incrementally;
- the footer is written only by `finish(manifest_id)` after the selected object is confirmed to be a manifest;
- any source or sink failure marks the writer terminal and prevents later finalization.

For identical inputs, `StreamingWriter` must produce exactly the same bytes as the in-memory deterministic `Writer`.

`SeekableWriter<W: Write + Seek>` is an adapter over the same deterministic stream path. It requires output to begin at offset zero and can rewind a finalized sink for immediate readback. It does not introduce alternate bytes or validity rules.

## Consequences

### Positive

- payload bodies need not be accumulated in memory;
- the active root is not published before successful finalization;
- deterministic output remains testable against the Phase 1 writer;
- ordinary files, pipes, sockets, and object-upload streams can share the same byte path;
- a seekable caller can immediately validate or inspect finalized output.

### Negative

- payload length must be known before each record is written in EXP-0001;
- a failed non-seekable write may leave an unusable prefix in the destination;
- the flat EXP-0001 directory is still retained in memory until finalization;
- the initial seekable adapter does not yet backpatch lengths or reclaim a failed suffix.

## Security implications

- the footer is the publication boundary and must remain the last successful write;
- failed writers are terminal so callers cannot accidentally continue after a partial object;
- declared payload and total file sizes are checked before writes;
- input truncation is distinguished from sink I/O failure;
- seekable output beginning at a nonzero offset is rejected to avoid ambiguous embedded-file semantics.

## Follow-up work

- evaluate a paged directory builder after the Phase 3 directory redesign;
- add optional transactional local-file output using temporary files and atomic rename;
- investigate safe seekable rollback where the sink supports truncation;
- add async adapters without changing deterministic byte generation;
- benchmark allocation reuse for payload copying.
