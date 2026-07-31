#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_batch, build_genesis, rewrite_source_selected_history, validate_history,
    ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput, ImmutableReadAt,
    ImmutableSourceError, ImmutableSourceLimits,
};

#[derive(Debug)]
struct TraceSource {
    bytes: Vec<u8>,
    largest_request: usize,
    reads: usize,
    mutate_after_first_read: bool,
}

impl TraceSource {
    fn stable(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            largest_request: 0,
            reads: 0,
            mutate_after_first_read: false,
        }
    }

    fn mutating(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            largest_request: 0,
            reads: 0,
            mutate_after_first_read: true,
        }
    }
}

impl ImmutableReadAt for TraceSource {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        u64::try_from(self.bytes.len()).map_err(|_| ImmutableSourceError::Limit("length"))
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        if self.mutate_after_first_read && self.reads == 1 && !self.bytes.is_empty() {
            self.bytes[0] ^= 1;
        }
        let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
        let end = start
            .checked_add(buffer.len())
            .ok_or(ImmutableSourceError::Io("range"))?;
        buffer.copy_from_slice(
            self.bytes
                .get(start..end)
                .ok_or(ImmutableSourceError::Io("range"))?,
        );
        self.reads = self
            .reads
            .checked_add(1)
            .ok_or(ImmutableSourceError::Limit("read operations"))?;
        self.largest_request = self.largest_request.max(buffer.len());
        Ok(())
    }
}

fn source_limits() -> ImmutableSourceLimits {
    ImmutableSourceLimits {
        format: ImmutableLimits {
            max_file_bytes: 2 * 1024 * 1024,
            max_objects: 32,
            max_history_entries: 8,
            max_pages: 64,
            max_depth: 4,
            max_allocation_bytes: 2 * 1024 * 1024,
            max_output_bytes: 2 * 1024 * 1024,
            ..ImmutableLimits::default()
        },
        max_total_bytes_read: 8 * 1024 * 1024,
        max_read_operations: 200_000,
        max_read_request_bytes: 256,
        hash_block_bytes: 256,
        ..ImmutableSourceLimits::default()
    }
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(2_usize, |byte| 2 + usize::from(*byte % 6));
    let limits = source_limits();
    let objects: Vec<_> = (1..=u64::try_from(count).expect("small count"))
        .map(|object_id| {
            let seed = data
                .get(usize::try_from(object_id).expect("small object id"))
                .copied()
                .unwrap_or(object_id as u8);
            ImmutableObjectInput::new(
                object_id,
                u16::from(1 + seed % 31),
                vec![seed, seed.rotate_left(1)],
            )
        })
        .collect();
    let genesis = build_genesis(&objects, limits.format).expect("bounded genesis");
    let first_seed = data.get(count + 1).copied().unwrap_or(17);
    let first = append_batch(
        &genesis,
        &[
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                1,
                7,
                vec![first_seed, first_seed.rotate_left(1)],
            )),
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                u64::try_from(count + 1).expect("small insertion"),
                9,
                b"first-insert".to_vec(),
            )),
        ],
        limits.format,
    )
    .expect("first append");
    let second_seed = data.get(count + 2).copied().unwrap_or(29);
    let second = append_batch(
        &first,
        &[
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                u64::try_from(count + 1).expect("small replacement"),
                11,
                vec![second_seed, second_seed.rotate_left(2)],
            )),
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                u64::try_from(count + 2).expect("small insertion"),
                13,
                b"second-insert".to_vec(),
            )),
        ],
        limits.format,
    )
    .expect("second append");

    let selection = if data.last().copied().unwrap_or(0) & 1 == 0 {
        vec![0, 2]
    } else {
        vec![0, 1, 2]
    };
    let mut stable = TraceSource::stable(second.clone());
    let rewritten = rewrite_source_selected_history(&mut stable, &selection, limits)
        .expect("bounded selected history rewrite");
    assert_eq!(rewritten.retained.len(), selection.len());
    assert_eq!(
        validate_history(&rewritten.bytes, limits.format)
            .expect("rewritten history validates")
            .entries
            .len(),
        selection.len()
    );
    assert!(stable.largest_request <= limits.max_read_request_bytes);

    let mut replay = TraceSource::stable(second.clone());
    assert_eq!(
        rewrite_source_selected_history(&mut replay, &selection, limits)
            .expect("deterministic replay")
            .bytes,
        rewritten.bytes
    );

    let mutation_seed = data.last().copied().unwrap_or(0);
    let mut corrupted = second.clone();
    let mutation_offset = usize::from(mutation_seed) % corrupted.len();
    corrupted[mutation_offset] ^= 1;
    let mut corrupted_source = TraceSource::stable(corrupted);
    assert!(rewrite_source_selected_history(&mut corrupted_source, &selection, limits).is_err());

    let mut mutating_source = TraceSource::mutating(second);
    assert!(rewrite_source_selected_history(&mut mutating_source, &selection, limits).is_err());
});
