#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, rewrite_all, rewrite_versioned_source_sequence_to,
    validate_history, ImmutableHistoryToSinkError, ImmutableLimits, ImmutableObjectInput,
    ImmutableReadAt, ImmutableSourceError, ImmutableSourceLimits,
    ImmutableSourceStreamingWriteOptions, ImmutableStreamingWriteOptions, ImmutableVersionedReadAt,
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

fn object(object_id: u64, seed: u8) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::from(1 + seed % 31),
        vec![seed; 1 + usize::from(seed % 64)],
    )
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 8));
    let commit_count = data
        .get(1)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 3));
    let request = data
        .get(2)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
    let hash_block = data
        .get(3)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
    let payload_chunk = data
        .get(4)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let sink_chunk = data
        .get(5)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let format = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 16,
        max_pages: 64,
        max_depth: 4,
        max_history_entries: 8,
        max_allocation_bytes: 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };

    let objects: Vec<_> = (0..count)
        .map(|index| {
            let seed = data.get(index + 6).copied().unwrap_or(index as u8);
            object(u64::try_from(index + 1).expect("small object id"), seed)
        })
        .collect();
    let mut source_bytes = build_genesis(&objects, format).expect("bounded genesis");
    for commit in 1..commit_count {
        let index = data
            .get(6 + count + commit)
            .map_or(commit % count, |byte| usize::from(*byte) % count);
        let seed = data
            .get(6 + count + commit_count + commit)
            .copied()
            .unwrap_or(71 + commit as u8);
        source_bytes = append_replacement(
            &source_bytes,
            &object(u64::try_from(index + 1).expect("small object id"), seed),
            format,
        )
        .expect("bounded replacement");
    }

    let history = validate_history(&source_bytes, format).expect("slice history");
    let sequence = data.last().map_or(0_u64, |byte| {
        u64::from(*byte) % u64::try_from(commit_count).expect("commits")
    });
    let entry = history
        .entries
        .iter()
        .find(|entry| entry.report.sequence == sequence)
        .expect("selected sequence");
    let prefix_len = entry.footer_offset + 192;
    let expected = rewrite_all(
        &source_bytes[..usize::try_from(prefix_len).expect("prefix")],
        format,
    )
    .expect("owned prefix rewrite");
    let limits = ImmutableSourceLimits {
        format,
        max_total_bytes_read: 32 * 1024 * 1024,
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

    let mut source = VersionedSource {
        data: source_bytes.clone(),
        version: [31; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let mut actual = Vec::new();
    let report =
        rewrite_versioned_source_sequence_to(&mut actual, &mut source, sequence, limits, options)
            .expect("selected historical source rewrite");
    assert_eq!(actual, expected.bytes);
    assert_eq!(report.output.source, expected.source);
    assert_eq!(report.output.output.report, expected.output);
    assert_eq!(report.selected_prefix_len, prefix_len);
    assert!(source.largest_request <= request);
    assert!(report.output.largest_payload_read_request <= payload_chunk.min(request));
    assert!(report.output.output.largest_write_request <= sink_chunk);

    let mut missing_source = VersionedSource {
        data: source_bytes.clone(),
        version: [37; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let mut untouched = Vec::new();
    assert_eq!(
        rewrite_versioned_source_sequence_to(
            &mut untouched,
            &mut missing_source,
            u64::try_from(commit_count + 10).expect("missing sequence"),
            limits,
            options,
        ),
        Err(ImmutableHistoryToSinkError::SequenceNotFound(
            u64::try_from(commit_count + 10).expect("missing sequence")
        ))
    );
    assert!(untouched.is_empty());

    let mut unstable = VersionedSource {
        data: source_bytes,
        version: [41; 32],
        reads: 0,
        mutate_after: Some(2),
        largest_request: 0,
    };
    assert_eq!(
        rewrite_versioned_source_sequence_to(
            &mut untouched,
            &mut unstable,
            sequence,
            limits,
            options,
        ),
        Err(ImmutableHistoryToSinkError::VersionChanged)
    );
    assert!(untouched.is_empty());
});
