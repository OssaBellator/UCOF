#![no_main]

use std::io::Write;

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    build_genesis, validate_canonical_occupancy, write_genesis_sources_to, ImmutableLimits,
    ImmutableObjectInput, ImmutableSourceStreamingWriteError, ImmutableSourceStreamingWriteOptions,
    ImmutableStreamingPayloadSource, ImmutableStreamingWriteOptions,
};

#[derive(Clone, Debug)]
struct PayloadSource {
    object_id: u64,
    kind: u16,
    bytes: Vec<u8>,
    version: [u8; 32],
    mutate: bool,
    largest_request: usize,
}

impl ImmutableStreamingPayloadSource for PayloadSource {
    fn object_id(&self) -> u64 {
        self.object_id
    }

    fn kind(&self) -> u16 {
        self.kind
    }

    fn logical_len(&self) -> u64 {
        u64::try_from(self.bytes.len()).expect("bounded payload")
    }

    fn strong_version(&mut self) -> Result<[u8; 32], &'static str> {
        Ok(self.version)
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), &'static str> {
        let start = usize::try_from(offset).map_err(|_| "offset")?;
        let end = start.checked_add(buffer.len()).ok_or("range")?;
        buffer.copy_from_slice(self.bytes.get(start..end).ok_or("range")?);
        self.largest_request = self.largest_request.max(buffer.len());
        if self.mutate {
            self.version[0] ^= 1;
            self.mutate = false;
        }
        Ok(())
    }
}

#[derive(Default)]
struct BoundedSink {
    bytes: Vec<u8>,
    largest_request: usize,
}

impl Write for BoundedSink {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.largest_request = self.largest_request.max(buffer.len());
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 16));
    let source_chunk = data
        .get(1)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let sink_chunk = data
        .get(2)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let limits = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 32,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };

    let mut sources = Vec::with_capacity(count);
    for index in 0..count {
        let object_id = u64::try_from(index + 1).expect("small object id");
        let seed = data.get(index + 3).copied().unwrap_or(index as u8);
        let payload_len = 1 + usize::from(seed % 96);
        sources.push(PayloadSource {
            object_id,
            kind: u16::from(1 + seed % 31),
            bytes: vec![seed; payload_len],
            version: [seed.wrapping_add(1); 32],
            mutate: false,
            largest_request: 0,
        });
    }
    if data.last().is_some_and(|byte| byte & 1 != 0) {
        sources.reverse();
    }
    let owned: Vec<_> = sources
        .iter()
        .map(|source| {
            ImmutableObjectInput::new(source.object_id, source.kind, source.bytes.clone())
        })
        .collect();
    let expected = build_genesis(&owned, limits).expect("bounded owned genesis");
    let mut sink = BoundedSink::default();
    let report = write_genesis_sources_to(
        &mut sink,
        &mut sources,
        ImmutableSourceStreamingWriteOptions {
            output: ImmutableStreamingWriteOptions {
                max_write_request_bytes: sink_chunk,
            },
            max_source_read_bytes: source_chunk,
        },
        limits,
    )
    .expect("bounded source streaming");
    assert_eq!(sink.bytes, expected);
    assert_eq!(
        validate_canonical_occupancy(&sink.bytes, limits).expect("canonical output"),
        report.output.report
    );
    assert!(sink.largest_request <= sink_chunk);
    assert!(sources
        .iter()
        .all(|source| source.largest_request <= source_chunk));

    let mut unstable = vec![PayloadSource {
        object_id: 1,
        kind: 1,
        bytes: vec![7; 8],
        version: [9; 32],
        mutate: true,
        largest_request: 0,
    }];
    let mut partial = BoundedSink::default();
    assert_eq!(
        write_genesis_sources_to(
            &mut partial,
            &mut unstable,
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions {
                    max_write_request_bytes: sink_chunk,
                },
                max_source_read_bytes: source_chunk,
            },
            limits,
        ),
        Err(ImmutableSourceStreamingWriteError::VersionChanged(1))
    );
    assert!(!partial.bytes.is_empty());
});
