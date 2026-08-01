#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, inventory_source_at, rewrite_all,
    rewrite_versioned_source_to, ImmutableLimits, ImmutableObjectInput, ImmutableReadAt,
    ImmutableSourceError, ImmutableSourceLimits, ImmutableSourceStreamingWriteOptions,
    ImmutableSourceToSinkError, ImmutableStreamingWriteOptions, ImmutableVersionedReadAt,
};

#[derive(Clone, Debug)]
struct VersionedSource {
    data: Vec<u8>,
    version: [u8; 32],
    reads: u64,
    mutate_after: Option<u64>,
    largest_request: usize,
}

impl ImmutableReadAt for VersionedSource {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        u64::try_from(self.data.len()).map_err(|_| ImmutableSourceError::Limit("length"))
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
        let end = start
            .checked_add(buffer.len())
            .ok_or(ImmutableSourceError::Io("range"))?;
        buffer.copy_from_slice(
            self.data
                .get(start..end)
                .ok_or(ImmutableSourceError::Io("range"))?,
        );
        self.reads += 1;
        self.largest_request = self.largest_request.max(buffer.len());
        if self.mutate_after == Some(self.reads) {
            self.version[0] ^= 1;
        }
        Ok(())
    }
}

impl ImmutableVersionedReadAt for VersionedSource {
    fn strong_version(&mut self) -> Result<[u8; 32], ImmutableSourceError> {
        Ok(self.version)
    }
}

fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, u16::from(1 + seed % 31), vec![seed; payload_len])
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 16));
    let request = data
        .get(1)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
    let hash_block = data
        .get(2)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
    let payload_chunk = data
        .get(3)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let sink_chunk = data
        .get(4)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let format = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 32,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };

    let mut active_lengths = Vec::with_capacity(count);
    let objects: Vec<_> = (0..count)
        .map(|index| {
            let object_id = u64::try_from(index + 1).expect("small object id");
            let seed = data.get(index + 5).copied().unwrap_or(index as u8);
            let payload_len = 1 + usize::from(seed % 96);
            active_lengths.push(payload_len);
            object(object_id, seed, payload_len)
        })
        .collect();
    let genesis = build_genesis(&objects, format).expect("bounded genesis");
    let source_bytes = if data.get(5 + count).is_some_and(|byte| byte & 1 != 0) {
        let index = data
            .get(6 + count)
            .map_or(0_usize, |byte| usize::from(*byte) % count);
        let object_id = u64::try_from(index + 1).expect("small object id");
        let seed = data.get(7 + count).copied().unwrap_or(83);
        let payload_len = 1 + usize::from(seed % 96);
        active_lengths[index] = payload_len;
        append_replacement(&genesis, &object(object_id, seed, payload_len), format)
            .expect("bounded replacement")
    } else {
        genesis
    };
    let limits = ImmutableSourceLimits {
        format,
        max_total_bytes_read: 16 * 1024 * 1024,
        max_read_operations: 1_000_000,
        max_read_request_bytes: request,
        hash_block_bytes: hash_block,
    };
    let options = ImmutableSourceStreamingWriteOptions {
        output: ImmutableStreamingWriteOptions {
            max_write_request_bytes: sink_chunk,
        },
        max_source_read_bytes: payload_chunk,
    };

    let expected = rewrite_all(&source_bytes, format).expect("owned active rewrite");
    let expected_payload_bytes: u64 = active_lengths
        .iter()
        .map(|length| u64::try_from(*length).expect("bounded payload"))
        .sum();
    let mut source = VersionedSource {
        data: source_bytes.clone(),
        version: [19; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let mut actual = Vec::new();
    let report = rewrite_versioned_source_to(&mut actual, &mut source, limits, options)
        .expect("versioned source rewrite");
    assert_eq!(actual, expected.bytes);
    assert_eq!(report.source, expected.source);
    assert_eq!(report.output.report, expected.output);
    assert_eq!(
        report.cumulative_source_stats.bytes_read - report.inventory_stats.bytes_read,
        expected_payload_bytes
    );
    assert!(report.largest_payload_read_request <= payload_chunk.min(request));
    assert!(report.output.largest_write_request <= sink_chunk);
    assert!(source.largest_request <= request);

    let mut probe = VersionedSource {
        data: source_bytes.clone(),
        version: [23; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let inventory = inventory_source_at(&mut probe, limits).expect("inventory probe");
    let mut unstable = VersionedSource {
        data: source_bytes,
        version: [23; 32],
        reads: 0,
        mutate_after: Some(inventory.stats.read_operations + 1),
        largest_request: 0,
    };
    let mut partial = Vec::new();
    assert_eq!(
        rewrite_versioned_source_to(&mut partial, &mut unstable, limits, options),
        Err(ImmutableSourceToSinkError::VersionChanged)
    );
    assert!(!partial.is_empty());
});
