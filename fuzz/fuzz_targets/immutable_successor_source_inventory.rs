#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, inventory_source_at, validate_source_at, ImmutableLimits,
    ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError, ImmutableSourceInventoryError,
    ImmutableSourceLimits, ImmutableVersionedReadAt,
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
    let format = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 32,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };

    let objects: Vec<_> = (0..count)
        .map(|index| {
            let object_id = u64::try_from(index + 1).expect("small object id");
            let seed = data.get(index + 3).copied().unwrap_or(index as u8);
            object(object_id, seed, 1 + usize::from(seed % 96))
        })
        .collect();
    let genesis = build_genesis(&objects, format).expect("bounded genesis");
    let source_bytes = if data.get(3 + count).is_some_and(|byte| byte & 1 != 0) {
        let index = data
            .get(4 + count)
            .map_or(0_usize, |byte| usize::from(*byte) % count);
        let object_id = u64::try_from(index + 1).expect("small object id");
        let seed = data.get(5 + count).copied().unwrap_or(79);
        append_replacement(
            &genesis,
            &object(object_id, seed, 1 + usize::from(seed % 96)),
            format,
        )
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

    let mut inventory_source = VersionedSource {
        data: source_bytes.clone(),
        version: [11; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    let inventory = inventory_source_at(&mut inventory_source, limits).expect("inventory");
    assert_eq!(inventory.objects.len(), count);
    assert_eq!(inventory.report.object_count, count);
    assert!(inventory_source.largest_request <= request);
    assert!(inventory
        .objects
        .windows(2)
        .all(|pair| pair[0].object_id < pair[1].object_id));

    let mut strict_source = VersionedSource {
        data: source_bytes.clone(),
        version: [11; 32],
        reads: 0,
        mutate_after: None,
        largest_request: 0,
    };
    assert_eq!(
        validate_source_at(&mut strict_source, limits)
            .expect("strict source")
            .report,
        inventory.report
    );

    let mut unstable = VersionedSource {
        data: source_bytes,
        version: [13; 32],
        reads: 0,
        mutate_after: Some(2),
        largest_request: 0,
    };
    assert_eq!(
        inventory_source_at(&mut unstable, limits),
        Err(ImmutableSourceInventoryError::VersionChanged)
    );
});
