#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, rewrite_source_selected_history,
    rewrite_versioned_source_selected_history_to, validate_history,
    ImmutableHistoryChainStreamingError, ImmutableHistoryChainStreamingOptions, ImmutableLimits,
    ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError, ImmutableSourceLimits,
    ImmutableVersionedReadAt,
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
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 6));
    let commit_count = data
        .get(1)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 3));
    let request = data
        .get(2)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
    let write_chunk = data
        .get(3)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
    let format = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 16,
        max_pages: 64,
        max_depth: 4,
        max_history_entries: 8,
        max_allocation_bytes: 4 * 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let objects: Vec<_> = (0..count)
        .map(|index| {
            let seed = data.get(index + 4).copied().unwrap_or(index as u8);
            object(u64::try_from(index + 1).expect("small object id"), seed)
        })
        .collect();
    let mut source_bytes = build_genesis(&objects, format).expect("bounded genesis");
    for commit in 1..commit_count {
        let index = data
            .get(4 + count + commit)
            .map_or(commit % count, |byte| usize::from(*byte) % count);
        let seed = data
            .get(4 + count + commit_count + commit)
            .copied()
            .unwrap_or(91 + commit as u8);
        source_bytes = append_replacement(
            &source_bytes,
            &object(u64::try_from(index + 1).expect("small object id"), seed),
            format,
        )
        .expect("bounded replacement");
    }

    let mut selected = vec![0_u64];
    if commit_count > 1 {
        selected.push(u64::try_from(commit_count - 1).expect("last sequence"));
    }
    if commit_count > 2 && data.last().is_some_and(|byte| byte & 1 != 0) {
        selected.push(1);
        selected.reverse();
    }
    let limits = ImmutableSourceLimits {
        format,
        max_total_bytes_read: 64 * 1024 * 1024,
        max_read_operations: 1_000_000,
        max_read_request_bytes: request,
        hash_block_bytes: request,
    };

    let mut expected_source = VersionedSource {
        data: source_bytes.clone(),
        version: [107; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let expected = rewrite_source_selected_history(&mut expected_source, &selected, limits)
        .expect("owned selected history");
    let mut source = VersionedSource {
        data: source_bytes.clone(),
        version: [107; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let mut actual = Vec::new();
    let report = rewrite_versioned_source_selected_history_to(
        &mut actual,
        &mut source,
        &selected,
        limits,
        ImmutableHistoryChainStreamingOptions {
            max_write_request_bytes: write_chunk,
        },
    )
    .expect("versioned selected history");
    assert_eq!(actual, expected.bytes);
    assert_eq!(report.retained, expected.retained);
    assert_eq!(report.source_stats, expected.stats);
    assert_eq!(report.bytes_written, actual.len() as u64);
    assert_eq!(report.output_allocation_bytes, actual.len());
    assert!(report.largest_write_request <= write_chunk);
    assert!(source.largest_request <= request);
    assert_eq!(
        validate_history(&actual, format)
            .expect("rewritten history")
            .entries
            .len(),
        selected.len()
    );

    let mut duplicate_source = VersionedSource {
        data: source_bytes.clone(),
        version: [109; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let mut untouched = Vec::new();
    assert!(rewrite_versioned_source_selected_history_to(
        &mut untouched,
        &mut duplicate_source,
        &[0, 0],
        limits,
        ImmutableHistoryChainStreamingOptions::default(),
    )
    .is_err());
    assert!(untouched.is_empty());

    let mut unstable = VersionedSource {
        data: source_bytes,
        version: [113; 32],
        reads: 0,
        mutate_after: Some(2),
        largest_request: 0,
    };
    assert_eq!(
        rewrite_versioned_source_selected_history_to(
            &mut untouched,
            &mut unstable,
            &selected,
            limits,
            ImmutableHistoryChainStreamingOptions::default(),
        ),
        Err(ImmutableHistoryChainStreamingError::VersionChanged)
    );
    assert!(untouched.is_empty());
});
