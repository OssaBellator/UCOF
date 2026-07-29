# Phase 2 Bounded Core API Guide

This guide describes the experimental Rust APIs on the Phase 2 branch. It is not a stable API promise and does not make `UCOF-EXP-0001` bytes stable.

## Choose the narrowest operation

| Goal | API | Reads payload bodies? | Integrity claim |
|---|---|---:|---|
| Inventory a seekable or range-backed source | `MetadataInspector` | No, except manifest and directory metadata | `NotChecked` |
| Validate a stable random-access source | `SourceValidator` | Yes, in bounded blocks | `Verified` on success |
| Process a non-seeking stream | `SequentialReader` | Yes, as bounded events | `Verified` only at final commit event |
| Explain why a source is invalid | `DiagnosticValidator` | Depends on the reached stage | Explicit `Invalid` or `Verified` |
| Recover complete prefix records | `PrefixSalvager` | No | Always `UnverifiedPrefix` |
| Write deterministic output to `Write` | `StreamingWriter` | Streams caller payloads | Footer only after `finish` |
| Write and rewind a seekable sink | `SeekableWriter` | Streams caller payloads | Footer only after `finish_and_rewind` |

## Configure hostile-input limits

```rust
use ucof_core::Limits;

let limits = Limits {
    max_file_bytes: 512 * 1024 * 1024,
    max_total_bytes_read: 64 * 1024 * 1024,
    max_payload_bytes: 256 * 1024 * 1024,
    max_metadata_bytes: 8 * 1024 * 1024,
    max_allocation_bytes: 1024 * 1024,
    max_stream_chunk_bytes: 64 * 1024,
    max_diagnostics: 32,
    ..Limits::default()
};
```

Do not raise one limit merely to bypass an architectural failure. In particular, increasing `max_records` does not make the flat EXP-0001 directory suitable for massive object counts.

## Metadata-only inspection

```rust
use std::fs::File;
use ucof_core::{Limits, MetadataInspector, SeekSource};

let file = File::open("example.ucof")?;
let mut source = SeekSource::new(file);
let report = MetadataInspector::new(Limits::default()).inspect(&mut source)?;

assert_eq!(report.integrity, ucof_core::IntegrityStatus::NotChecked);
for entry in report.entries {
    println!("{} {:?} {} bytes", entry.id, entry.kind, entry.stored_len);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

This operation reads the bootstrap, exact-end footer, directory, record headers, and active manifest. It deliberately skips opaque payload bodies.

## Strict source validation

```rust
use std::fs::File;
use ucof_core::{Limits, SeekSource, SourceValidator};

let file = File::open("example.ucof")?;
let mut source = SeekSource::new(file);
let report = SourceValidator::new(Limits::default()).validate(&mut source)?;

println!("hashed {} bytes", report.stats.bytes_hashed);
assert_eq!(report.integrity, ucof_core::IntegrityStatus::Verified);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The source must present a stable length and byte view for the duration of the operation. Metadata inspection and bulk hashing share one cumulative read budget.

## Sequential event reading

```rust
use std::fs::File;
use ucof_core::{Limits, SequentialReader, StreamEvent};

let file = File::open("example.ucof")?;
let mut reader = SequentialReader::new(file, Limits::default());
while let Some(event) = reader.next_event()? {
    match event {
        StreamEvent::PayloadChunk { object_id, bytes, .. } => {
            println!("object {object_id}: {} bytes", bytes.len());
        }
        StreamEvent::Commit(commit) => {
            assert_eq!(commit.integrity, ucof_core::IntegrityStatus::Verified);
        }
        _ => {}
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

No record or payload event is an integrity claim. Only the final commit event establishes the committed-prefix digest and exact-end footer.

## Diagnostics and salvage

```rust
use ucof_core::{DiagnosticStatus, DiagnosticValidator, PrefixSalvager, SliceSource};

let bytes = std::fs::read("damaged.ucof")?;
let mut source = SliceSource::new(&bytes);
let diagnosis = DiagnosticValidator::default().diagnose(&mut source)?;

if diagnosis.status == DiagnosticStatus::Invalid {
    for item in diagnosis.diagnostics {
        eprintln!("{:?}: {}", item.category, item.message);
    }
}

let mut source = SliceSource::new(&bytes);
let salvage = PrefixSalvager::default().scan(&mut source)?;
assert_eq!(salvage.status, DiagnosticStatus::UnverifiedPrefix);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Recovered records are not valid objects merely because their physical ranges are complete. Salvage does not verify the directory, active manifest, digest, schema, or profile.

## Streaming output

```rust
use std::fs::File;
use ucof_core::{Manifest, StreamingWriter};

let output = File::create("example.ucof")?;
let mut writer = StreamingWriter::with_default_limits(output)?;
writer.add_opaque(1, b"payload")?;
writer.add_manifest(2, &Manifest::new(vec![1]))?;
let finished = writer.finish(2)?;
println!("wrote {} bytes", finished.bytes_written);
# Ok::<(), Box<dyn std::error::Error>>(())
```

A footer is never written before successful explicit finalization. After a source or sink failure, the writer is terminal and the partial destination must not be treated as a UCOF file.

## Trusted convenience versus bounded APIs

`Writer` and `ValidatedFile::parse` remain useful for small in-memory tests and trusted convenience paths. Applications accepting untrusted or remotely stored input should prefer the bounded source or sequential APIs and should set limits appropriate to their environment.

## Current limitations

- all Phase 2 I/O APIs are synchronous;
- EXP-0001 requires payload lengths before writing;
- the directory remains flat and materialized;
- no transform, encryption, signature, provenance, append-history, or external-reference semantics exist yet;
- salvage stops at the first fatal framing error and does not resynchronize;
- no API in this guide makes the experimental epoch stable.
