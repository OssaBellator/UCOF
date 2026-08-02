#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_batch, build_genesis, plan_persistent_insertion_tail_at,
    ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput, ImmutableReadAt,
    ImmutableSourceError, ImmutableSourceLimits, PersistentSourceInsertionError,
    PersistentSourceVersion, PersistentVersionedReadAt, LEAF_CAPACITY,
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

fuzz_target!(|data: &[u8]| {
    let boundary = data.first().is_some_and(|byte| byte & 0x80 != 0);
    let count = if boundary {
        LEAF_CAPACITY
    } else {
        1 + data
            .first()
            .map_or(31_usize, |byte| usize::from(*byte) % 180)
    };
    let format = ImmutableLimits {
        max_file_bytes: 32 * 1024 * 1024,
        max_output_bytes: 32 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let objects: Vec<_> = (1..=count)
        .map(|index| {
            let seed = data
                .get(index % data.len().max(1))
                .copied()
                .unwrap_or(index as u8);
            object(
                u64::try_from(index * 2).expect("object id"),
                seed,
                1 + usize::from(seed) % 31,
            )
        })
        .collect();
    let base = build_genesis(&objects, format).expect("canonical base");
    let selector = data.get(1).copied().unwrap_or(0) % 3;
    let inserted_id = match selector {
        0 => 1,
        1 => {
            let position = 1 + data
                .get(2)
                .map_or(0_usize, |byte| usize::from(*byte) % count);
            u64::try_from(position * 2 - 1).expect("middle id")
        }
        _ => u64::try_from(count * 2 + 1).expect("last id"),
    };
    let seed = data.get(3).copied().unwrap_or(211);
    let input = object(inserted_id, seed, 1 + usize::from(seed) % 43);
    let operations = [ImmutableBatchOperation::Put(input.clone())];
    let owned = append_persistent_batch(&base, &operations, format).expect("owned insertion");
    let limits = ImmutableSourceLimits {
        format,
        max_total_bytes_read: u64::try_from(base.len() * 12).expect("read budget"),
        max_read_operations: 2_000_000,
        max_read_request_bytes: 1 + data.get(4).map_or(127_usize, |byte| usize::from(*byte)),
        hash_block_bytes: 1 + data.get(5).map_or(131_usize, |byte| usize::from(*byte)),
    };
    let version = PersistentSourceVersion([51; 32]);
    let mut source = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: None,
    };
    let plan = plan_persistent_insertion_tail_at(&mut source, &input, limits)
        .expect("source insertion plan");
    assert_eq!(plan.tail, owned.bytes[base.len()..]);
    assert_eq!(plan.report, owned.report);
    assert_eq!(plan.pages_written, owned.pages_written);
    assert_eq!(plan.pages_reused, owned.pages_reused);
    assert_eq!(plan.version, version);
    assert!(plan.version_checks > 0);
    assert!(plan.source_stats.bytes_read <= limits.max_total_bytes_read);
    assert!(plan.source_stats.read_operations <= limits.max_read_operations);

    let mut changed = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: Some(1),
    };
    assert_eq!(
        plan_persistent_insertion_tail_at(&mut changed, &input, limits)
            .expect_err("version change"),
        PersistentSourceInsertionError::VersionChanged
    );

    let duplicate = objects
        .get(data.get(6).map_or(0_usize, |byte| usize::from(*byte) % count))
        .expect("duplicate object")
        .clone();
    let mut duplicate_source = VersionedSource {
        bytes: base,
        version,
        reads: 0,
        mutate_after_read: None,
    };
    assert!(matches!(
        plan_persistent_insertion_tail_at(&mut duplicate_source, &duplicate, limits),
        Err(PersistentSourceInsertionError::Writer(_))
    ));
});
