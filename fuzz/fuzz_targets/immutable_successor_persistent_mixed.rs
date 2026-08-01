#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_mixed_batch, build_genesis,
    validate_canonical_occupancy, ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput,
    PersistentBatchMode, LEAF_CAPACITY,
};

fn object(object_id: u64, seed: u8) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::from(1 + seed % 31),
        vec![seed, seed.rotate_left(1), seed.rotate_left(2)],
    )
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(4_usize, |byte| 4 + usize::from(*byte) % (2 * LEAF_CAPACITY));
    let limits = ImmutableLimits {
        max_file_bytes: 8 * 1024 * 1024,
        max_objects: 2 * LEAF_CAPACITY + 16,
        max_pages: 128,
        max_depth: 4,
        max_allocation_bytes: 8 * 1024 * 1024,
        max_output_bytes: 8 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let objects: Vec<_> = (1..=u64::try_from(count).expect("small count"))
        .map(|index| object(index * 2, index as u8))
        .collect();
    let genesis = build_genesis(&objects, limits).expect("bounded canonical genesis");

    let delete_index = data
        .get(1)
        .map_or(0_usize, |byte| usize::from(*byte) % count);
    let delete_id = u64::try_from(delete_index + 1).expect("small index") * 2;
    let replace_index = data.get(2).map_or((delete_index + 1) % count, |byte| {
        usize::from(*byte) % count
    });
    let mut replace_id = u64::try_from(replace_index + 1).expect("small index") * 2;
    if replace_id == delete_id {
        replace_id = u64::try_from((replace_index + 1) % count + 1).expect("small index") * 2;
    }
    let inserted_id = u64::try_from(count)
        .expect("small count")
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .expect("bounded identifier");
    let replace_seed = data.get(3).copied().unwrap_or(17);
    let insert_seed = data.get(4).copied().unwrap_or(29);

    let mut operations = vec![
        ImmutableBatchOperation::Delete(delete_id),
        ImmutableBatchOperation::Put(object(replace_id, replace_seed)),
    ];
    if data.get(5).is_none_or(|byte| byte & 1 == 0) {
        operations.push(ImmutableBatchOperation::Put(object(
            inserted_id,
            insert_seed,
        )));
    }

    let direct = append_persistent_mixed_batch(&genesis, &operations, limits)
        .expect("bounded persistent mixed batch");
    assert_eq!(direct.mode, PersistentBatchMode::CopyOnWriteCanonicalMixed);
    assert_eq!(
        validate_canonical_occupancy(&direct.bytes, limits).expect("canonical mixed output"),
        direct.report
    );
    let expected_count = count - 1 + usize::from(operations.len() == 3);
    assert_eq!(direct.report.object_count, expected_count);
    assert_eq!(
        append_persistent_batch(&genesis, &operations, limits).expect("general route"),
        direct
    );

    operations.reverse();
    assert_eq!(
        append_persistent_mixed_batch(&genesis, &operations, limits)
            .expect("reversed mixed batch")
            .bytes,
        direct.bytes
    );
});
