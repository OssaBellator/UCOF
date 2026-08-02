#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_delete, build_genesis, plan_persistent_deletion_tail_at, ImmutableLimits,
    ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError, ImmutableSourceLimits,
    PersistentSourceDeletionError, PersistentSourceVersion, PersistentVersionedReadAt,
    LEAF_CAPACITY, LEAF_MIN_OCCUPANCY,
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

fn objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(|object_id| {
            ImmutableObjectInput::new(
                object_id,
                u16::try_from(object_id % 31 + 1).expect("kind"),
                vec![object_id as u8; 1 + usize::try_from(object_id % 29).expect("payload")],
            )
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let scenario = data.first().copied().unwrap_or(0) % 5;
    let count = match scenario {
        0 => 2 + data.get(1).map_or(8_usize, |byte| usize::from(*byte) % 120),
        1 => 400,
        2 => LEAF_CAPACITY + 2,
        3 => 2 * LEAF_MIN_OCCUPANCY,
        _ => {
            2 + data
                .get(1)
                .map_or(31_usize, |byte| usize::from(*byte) % 220)
        }
    };
    let format = ImmutableLimits {
        max_file_bytes: 32 * 1024 * 1024,
        max_output_bytes: 32 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let base = build_genesis(&objects(count), format).expect("canonical base");
    let object_id = 1 + u64::try_from(
        data.get(2)
            .map_or(0_usize, |byte| usize::from(*byte) % count),
    )
    .expect("object id");
    let owned = append_persistent_delete(&base, object_id, format).expect("owned deletion");
    let limits = ImmutableSourceLimits {
        format,
        max_total_bytes_read: u64::try_from(base.len() * 14).expect("read budget"),
        max_read_operations: 2_000_000,
        max_read_request_bytes: 1 + data.get(3).map_or(127_usize, |byte| usize::from(*byte)),
        hash_block_bytes: 1 + data.get(4).map_or(131_usize, |byte| usize::from(*byte)),
    };
    let version = PersistentSourceVersion([71; 32]);
    let mut source = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: None,
    };
    let plan = plan_persistent_deletion_tail_at(&mut source, object_id, limits)
        .expect("source deletion plan");
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
        plan_persistent_deletion_tail_at(&mut changed, object_id, limits)
            .expect_err("version change"),
        PersistentSourceDeletionError::VersionChanged
    );

    let mut missing = VersionedSource {
        bytes: base,
        version,
        reads: 0,
        mutate_after_read: None,
    };
    assert!(matches!(
        plan_persistent_deletion_tail_at(
            &mut missing,
            u64::try_from(count + 1).expect("missing id"),
            limits,
        ),
        Err(PersistentSourceDeletionError::Writer(_))
    ));
});
