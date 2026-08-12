# ADR-0005: Require a stable source view for strict random-access validation

- **Status:** Accepted
- **Date:** 2026-07-30
- **Owners:** UCOF maintainers
- **Related issues:** None yet
- **Related FCPs:** FCP-0001
- **Supersedes:** None
- **Superseded by:** None

## Context

Metadata-only inspection deliberately skips opaque payload bodies and therefore cannot establish the committed-prefix digest. Phase 2 also needs a strict random-access operation that can verify payload integrity without reading the entire file into memory.

The `ReadAt` abstraction permits repeated reads at explicit offsets. A validator can first establish the structural inventory and then hash the committed prefix in bounded blocks. This is a multi-pass operation. If a source changes between reads, the structural report and hashed bytes may refer to different versions.

Object stores, files, memory buffers, and remote range services provide different consistency guarantees. The reference library cannot infer snapshot stability from the `ReadAt` method signatures alone.

## Decision

The Rust reference library introduces `SourceValidator`, a strict bounded validator over `ReadAt`.

A caller using `SourceValidator` must provide a source that presents a stable length and stable bytes for the duration of one `validate` call. An adapter may satisfy this through:

- immutable memory;
- a file handle protected from concurrent mutation;
- a snapshot or version identifier;
- conditional range requests bound to one object version;
- another equivalent consistency mechanism.

Validation proceeds in two bounded stages:

1. `MetadataInspector` validates bootstrap, footer, directory, physical record headers, and the active manifest without reading opaque payloads.
2. The validator reads every committed-prefix byte in bounded blocks, hashes it, revalidates footer fields against the structural report, and compares the digest.

The operation uses one cumulative `max_total_bytes_read` budget across inspection and hashing. It reports:

- total read operations;
- total bytes read;
- bytes hashed;
- largest allocation;
- verified integrity status;
- unsupported required capabilities.

Hash block size is bounded by both `max_stream_chunk_bytes` and `max_allocation_bytes`. No complete opaque payload is allocated.

## Decision drivers

- verify all committed bytes without full-file buffering;
- make duplicate reads and hash work visible to callers;
- preserve the metadata-only fast path;
- avoid pretending that a mutable range source is a snapshot;
- reuse one hostile-input limit configuration across reader modes;
- retain a simple synchronous core.

## Alternatives considered

### Load the whole file and reuse the Phase 1 slice validator

Rejected for the bounded untrusted-input API because memory scales with file size.

### Trust the directory and hash only referenced payloads

Rejected. Integrity covers the complete committed prefix, including framing and metadata bytes. Unreferenced committed bytes must not be silently skipped.

### Hash first, inspect second

This still requires source stability and risks reporting structure from bytes different from those hashed. The selected order gives early structural and limit failures before bulk I/O.

### Add version tokens to the core `ReadAt` trait immediately

Deferred. Version-token models differ across local files and remote object stores. Adapters may enforce stability externally until a portable contract is demonstrated.

### Ignore metadata inspection reads in the total budget

Rejected. Repeated reads are real attacker-influenced work and must be included in caller accounting.

## Consequences

### Positive

- payload integrity is verified with bounded memory;
- bytes read and bytes hashed are reported separately;
- metadata inspection remains available without payload I/O;
- validation work cannot hide behind multiple internal passes;
- footer fields are checked again against the structural result;
- range-backed adapters can implement snapshot guarantees appropriate to their storage system.

### Negative

- strict validation may read some metadata more than once;
- a file near the maximum file-size limit may require a larger total-read budget than its stored size;
- stability is currently a documented caller/source contract rather than a type-system guarantee;
- network adapters may need version-aware requests or retries;
- the Phase 1 flat directory remains fully materialized.

## Security implications

- a source that mutates during validation is outside the strict operation's contract;
- combined read accounting limits multi-pass amplification;
- checked ranges prevent offset and length overflow;
- digest comparison occurs only after the full committed prefix is hashed;
- footer manifest, count, directory offset, and directory length are rechecked against the inspection result;
- unknown required capabilities prevent a fully-interpretable status but do not erase verified byte integrity.

## Follow-up work

- add version-aware remote adapters and conditional range examples;
- evaluate a `StableReadAt` wrapper once multiple adapters demonstrate common semantics;
- add tests with a deliberately mutating source to ensure changes fail rather than pass accidentally;
- reduce duplicated metadata reads where this does not weaken accounting or consistency;
- add fuzz targets for footer revalidation and bounded hash loops;
- document recommended total-read budgets for full validation.
