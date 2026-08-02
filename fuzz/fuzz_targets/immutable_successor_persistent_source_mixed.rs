#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_mixed_batch, build_genesis, plan_persistent_mixed_tail_at,
    ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput, ImmutableReadAt,
    ImmutableSourceError, ImmutableSourceLimits, PersistentSourceMixedError,
    PersistentSourceVersion, PersistentVersionedReadAt, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY,
};

struct VersionedSource {
    bytes: Vec<u8>,
    version: PersistentSourceVersion,
    reads: usize,
    mutate_after_read: Option<usize>,
}

impl ImmutableReadAt for VersionedSource {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        u64::try_from(self.bytes.len()).map_err(|_| ImmutableSourceError::Limit("length"))
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
            self.bytes
                .get(start..end)
                .ok_or(ImmutableSourceError::Io("range"))?,
        );
        self.reads += 1;
        if self.mutate_after_read == Some(self.reads) {
            self.version.0[0] ^= 1;
        }
        Ok(())
    }
}

impl PersistentVersionedReadAt for VersionedSource {
    fn version_token(&mut self) -> Result<PersistentSourceVersion, ImmutableSourceError> {
        Ok(self.version)
    }
}

fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, u16::from(seed % 31 + 1), vec![seed; payload_len])
}

fn even_objects(count: usize, data: &[u8]) -> Vec<ImmutableObjectInput> {
    (0..count)
        .map(|index| {
            let seed = data
                .get(index % data.len().max(1))
                .copied()
                .unwrap_or(index as u8);
            object(
                u64::try_from((index + 1) * 2).expect("object id"),
                seed,
                1 + usize::from(seed) % 31,
            )
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let scenario = data.first().copied().unwrap_or(0) % 4;
    let count = match scenario {
        0 => 2 * LEAF_MIN_OCCUPANCY,
        1 => LEAF_CAPACITY,
        2 => 400,
        _ => 2 + data.get(1).map_or(31_usize, |byte| usize::from(*byte) % 220),
    };
    let format = ImmutableLimits {
        max_file_bytes: 64 * 1024 * 1024,
        max_output_bytes: 64 * 1024 * 1024,
        max_allocation_bytes: 64 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let base = build_genesis(&even_objects(count, data), format).expect("canonical base");
    let first_seed = data.get(2).copied().unwrap_or(231);
    let second_seed = data.get(3).copied().unwrap_or(232);
    let last_existing = u64::try_from(count * 2).expect("last id");
    let mut operations = match scenario {
        0 => vec![
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(object(4, first_seed, 17)),
        ],
        1 => vec![
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(object(1, first_seed, 19)),
            ImmutableBatchOperation::Put(object(3, second_seed, 23)),
        ],
        _ => vec![
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(object(3, first_seed, 21)),
            ImmutableBatchOperation::Put(object(
                last_existing,
                second_seed,
                1 + usize::from(second_seed) % 37,
            )),
        ],
    };
    if data.get(4).is_some_and(|byte| byte & 1 != 0) {
        operations.reverse();
    }

    let owned = append_persistent_mixed_batch(&base, &operations, format).expect("owned mixed");
    let limits = ImmutableSourceLimits {
        format,
        max_total_bytes_read: u64::try_from(base.len() * 16).expect("read budget"),
        max_read_operations: 2_000_000,
        max_read_request_bytes: 1 + data.get(5).map_or(127_usize, |byte| usize::from(*byte)),
        hash_block_bytes: 1 + data.get(6).map_or(131_usize, |byte| usize::from(*byte)),
    };
    let version = PersistentSourceVersion([111; 32]);
    let mut source = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: None,
    };
    let plan = plan_persistent_mixed_tail_at(&mut source, &operations, limits)
        .expect("source mixed plan");
    assert_eq!(plan.tail, owned.bytes[base.len()..]);
    assert_eq!(plan.report, owned.report);
    assert_eq!(plan.pages_written, owned.pages_written);
    assert_eq!(plan.pages_reused, owned.pages_reused);
    assert_eq!(plan.version, version);
    assert!(plan.version_checks > 0);
    assert!(plan.retained_metadata_allocation_bytes > 0);
    assert!(plan.source_stats.bytes_read <= limits.max_total_bytes_read);
    assert!(plan.source_stats.read_operations <= limits.max_read_operations);

    let mut reversed = operations.clone();
    reversed.reverse();
    let mut second_source = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: None,
    };
    let second = plan_persistent_mixed_tail_at(&mut second_source, &reversed, limits)
        .expect("order-independent mixed plan");
    assert_eq!(plan.tail, second.tail);
    assert_eq!(plan.report, second.report);

    let mut changed = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: Some(1),
    };
    assert_eq!(
        plan_persistent_mixed_tail_at(&mut changed, &operations, limits)
            .expect_err("version change"),
        PersistentSourceMixedError::VersionChanged
    );

    let mut missing = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: None,
    };
    assert!(matches!(
        plan_persistent_mixed_tail_at(
            &mut missing,
            &[
                ImmutableBatchOperation::Delete(last_existing + 2),
                ImmutableBatchOperation::Put(object(1, first_seed, 7)),
            ],
            limits,
        ),
        Err(PersistentSourceMixedError::Writer(_))
    ));

    let mut duplicate = VersionedSource {
        bytes: base,
        version,
        reads: 0,
        mutate_after_read: None,
    };
    assert!(matches!(
        plan_persistent_mixed_tail_at(
            &mut duplicate,
            &[
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(2, second_seed, 9)),
            ],
            limits,
        ),
        Err(PersistentSourceMixedError::Writer(_))
    ));
});
