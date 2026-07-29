# ADR-0003: Use a bounded synchronous random-access source abstraction

- **Status:** Accepted
- **Date:** 2026-07-29
- **Owners:** UCOF maintainers
- **Related issues:** None yet
- **Related FCPs:** FCP-0001
- **Supersedes:** None
- **Superseded by:** None

## Context

The Phase 1 reader validates a complete borrowed byte slice. That is useful for proving the experimental wire layout, but it does not demonstrate bounded reads from files, sparse objects, or remote range adapters. It also cannot show that metadata-only inspection avoids payload bodies.

Phase 2 needs a small implementation-local I/O contract that supports:

- exact bounded reads at explicit offsets;
- caller-controlled accounting of bytes and operations;
- slice-backed tests;
- seekable files and cursors;
- future range-backed adapters;
- APIs that do not imply payload integrity when payload bytes were not read.

This ADR does not change the wire format and is not normative for independent implementations.

## Decision

The Rust reference library introduces a synchronous `ReadAt` trait:

```rust
pub trait ReadAt {
    fn len(&mut self) -> io::Result<u64>;
    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()>;
}
```

The initial adapters are:

- `SliceSource` for borrowed in-memory bytes;
- `SeekSource<R>` for values implementing `Read + Seek`.

The higher-level reader performs all range, allocation, and cumulative-read checks before invoking the source. Source implementations must fill the complete requested buffer or return an I/O error.

Metadata-only inspection reads:

- the fixed header;
- the exact-end footer;
- the directory record and payload;
- each inventory record header;
- the active manifest payload.

It skips opaque payload bodies. Its report therefore carries `IntegrityStatus::NotChecked`; structural inventory must never be presented as full conformance or verified payload integrity.

The trait uses `&mut self` so adapters may seek and maintain request-local state without interior mutability. Thread-safe concurrent range access is not assumed by the core contract.

## Decision drivers

- make every physical read explicit and measurable;
- support hostile-input budgets before allocation or I/O;
- avoid requiring a full file in memory;
- permit simple independent reproduction;
- keep networking and async runtimes outside the mandatory core;
- distinguish structural metadata inspection from complete integrity validation.

## Alternatives considered

### `Read + Seek` as the public contract

This is familiar and sufficient for local files, but it couples the reader to cursor state and does not map cleanly to object-store range requests. It remains supported through `SeekSource`.

### A byte-slice-only API

This has the smallest implementation surface but cannot prove bounded I/O or sparse metadata access. It remains as the Phase 1 convenience path.

### Async-first source trait

An async trait would force runtime, allocation, and trait-object decisions too early. Async adapters may later expose equivalent semantics without changing the synchronous core contract.

### Memory mapping

Memory mapping can be an adapter optimization but must not define validity or resource accounting. Mapping a file also does not guarantee that an application avoids touching payload pages.

## Consequences

### Positive

- metadata-only reads can be tested precisely;
- local files, cursors, slices, and future range clients share one contract;
- read budgets are enforced independently of source behavior;
- the API does not require loading payload bodies;
- wire-format logic remains independent from OS file handles.

### Negative

- `SeekSource` performs a seek per read;
- the first API is synchronous;
- I/O errors are initially categorized with contextual but implementation-neutral detail;
- repeated small record-header reads may be inefficient until batching is added;
- a mutable source cannot be shared concurrently without an external wrapper.

## Security implications

- offset and length arithmetic remains in the trusted reader layer and uses checked operations;
- cumulative bytes read and single allocations are bounded before source calls;
- source length is treated as untrusted and compared with file limits;
- metadata-only inspection explicitly refuses to claim payload integrity;
- directory entries are cross-checked against physical record headers rather than trusted as authoritative ranges.

## Follow-up work

- add sequential event reading without seeking;
- add a strict source validator that hashes payloads under a separate byte budget;
- define diagnostic and salvage reports with bounded counts;
- evaluate batched header reads and remote range adapters;
- document async adapter semantics without introducing a mandatory runtime;
- fuzz source-backed header, footer, directory, and manifest paths.
