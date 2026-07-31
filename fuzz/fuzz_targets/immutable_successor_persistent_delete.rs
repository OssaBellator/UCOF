#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_delete, build_genesis,
    validate_canonical_occupancy, ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput,
    PersistentBatchMode, LEAF_CAPACITY,
};

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(2_usize, |byte| 2 + usize::from(*byte) % (2 * LEAF_CAPACITY));
    let limits = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 2 * LEAF_CAPACITY + 8,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 4 * 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
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
    let genesis = build_genesis(&objects, limits).expect("bounded canonical genesis");
    let index = data
        .last()
        .map_or(0_usize, |byte| usize::from(*byte) % count);
    let object_id = u64::try_from(index + 1).expect("small object id");

    let direct =
        append_persistent_delete(&genesis, object_id, limits).expect("bounded persistent deletion");
    assert_eq!(direct.mode, PersistentBatchMode::CopyOnWriteDeletion);
    assert_eq!(direct.report.object_count, count - 1);
    assert_eq!(
        validate_canonical_occupancy(&direct.bytes, limits).expect("deleted bytes validate"),
        direct.report
    );

    let general = append_persistent_batch(
        &genesis,
        &[ImmutableBatchOperation::Delete(object_id)],
        limits,
    )
    .expect("general deletion path");
    assert_eq!(general, direct);
    assert_eq!(
        append_persistent_delete(&genesis, object_id, limits)
            .expect("deterministic replay")
            .bytes,
        direct.bytes
    );
});
