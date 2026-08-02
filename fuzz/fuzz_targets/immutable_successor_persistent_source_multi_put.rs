#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_put_batch, build_genesis, plan_persistent_put_batch_tail_at, ImmutableLimits,
    ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError, ImmutableSourceLimits,
    PersistentSourceMultiPutError, PersistentSourceVersion, PersistentVersionedReadAt,
    LEAF_CAPACITY,
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
        2 + data
            .first()
            .map_or(31_usize, |byte| usize::from(*byte) % 220)
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
    let last_existing = u64::try_from(count * 2).expect("last id");
    let first_seed = data.get(1).copied().unwrap_or(221);
    let second_seed = data.get(2).copied().unwrap_or(222);
    let third_seed = data.get(3).copied().unwrap_or(223);
    let mut inputs = vec![
        object(2, first_seed, 1 + usize::from(first_seed) % 41),
        object(3, second_seed, 1 + usize::from(second_seed) % 43),
        object(
            last_existing + 1,
            third_seed,
            1 + usize::from(third_seed) % 47,
        ),
    ];
    if count > 2 && data.get(4).is_some_and(|byte| byte & 1 != 0) {
        inputs.push(object(
            last_existing,
            first_seed ^ third_seed,
            1 + usize::from(first_seed ^ third_seed) % 37,
        ));
    }
    if data.get(5).is_some_and(|byte| byte & 1 != 0) {
        inputs.reverse();
    }

    let owned = append_persistent_put_batch(&base, &inputs, format).expect("owned multi-Put");
    let limits = ImmutableSourceLimits {
        format,
        max_total_bytes_read: u64::try_from(base.len() * 14).expect("read budget"),
        max_read_operations: 2_000_000,
        max_read_request_bytes: 1 + data.get(6).map_or(127_usize, |byte| usize::from(*byte)),
        hash_block_bytes: 1 + data.get(7).map_or(131_usize, |byte| usize::from(*byte)),
    };
    let version = PersistentSourceVersion([91; 32]);
    let mut source = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: None,
    };
    let plan = plan_persistent_put_batch_tail_at(&mut source, &inputs, limits)
        .expect("source multi-Put plan");
    assert_eq!(plan.tail, owned.bytes[base.len()..]);
    assert_eq!(plan.report, owned.report);
    assert_eq!(plan.pages_written, owned.pages_written);
    assert_eq!(plan.pages_reused, owned.pages_reused);
    assert_eq!(plan.version, version);
    assert!(plan.inserted_objects >= 2);
    assert!(plan.version_checks > 0);
    assert!(plan.source_stats.bytes_read <= limits.max_total_bytes_read);
    assert!(plan.source_stats.read_operations <= limits.max_read_operations);

    let mut reversed = inputs.clone();
    reversed.reverse();
    let mut second_source = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: None,
    };
    let second = plan_persistent_put_batch_tail_at(&mut second_source, &reversed, limits)
        .expect("order-independent plan");
    assert_eq!(plan.tail, second.tail);
    assert_eq!(plan.report, second.report);

    let mut changed = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: Some(1),
    };
    assert_eq!(
        plan_persistent_put_batch_tail_at(&mut changed, &inputs, limits)
            .expect_err("version change"),
        PersistentSourceMultiPutError::VersionChanged
    );

    let duplicate = vec![inputs[0].clone(), inputs[0].clone()];
    let mut duplicate_source = VersionedSource {
        bytes: base,
        version,
        reads: 0,
        mutate_after_read: None,
    };
    assert!(matches!(
        plan_persistent_put_batch_tail_at(&mut duplicate_source, &duplicate, limits),
        Err(PersistentSourceMultiPutError::Writer(_))
    ));
});
