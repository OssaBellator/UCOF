#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_insert, build_genesis, validate,
    ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput, PersistentBatchMode,
    LEAF_CAPACITY,
};

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(1_usize, |byte| 1 + usize::from(*byte) % LEAF_CAPACITY);
    let limits = ImmutableLimits {
        max_file_bytes: 2 * 1024 * 1024,
        max_objects: LEAF_CAPACITY + 8,
        max_pages: 32,
        max_depth: 4,
        max_allocation_bytes: 2 * 1024 * 1024,
        max_output_bytes: 2 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let objects: Vec<_> = (1..=u64::try_from(count).expect("small count"))
        .map(|index| {
            let seed = data
                .get(usize::try_from(index).expect("small index"))
                .copied()
                .unwrap_or(index as u8);
            ImmutableObjectInput::new(
                index * 2,
                u16::from(1 + seed % 31),
                vec![seed, seed.rotate_left(1)],
            )
        })
        .collect();
    let genesis = build_genesis(&objects, limits).expect("bounded genesis");
    let slot = data
        .last()
        .map_or(0_usize, |byte| usize::from(*byte) % (count + 1));
    let object_id = 1 + 2 * u64::try_from(slot).expect("small slot");
    let inserted = ImmutableObjectInput::new(object_id, 7, b"inserted".to_vec());

    let direct = append_persistent_insert(&genesis, &inserted, limits)
        .expect("bounded persistent insertion");
    assert_eq!(direct.mode, PersistentBatchMode::CopyOnWriteInsertion);
    assert_eq!(direct.report.object_count, count + 1);
    assert_eq!(
        validate(&direct.bytes, limits).expect("inserted bytes validate"),
        direct.report
    );

    let general = append_persistent_batch(
        &genesis,
        &[ImmutableBatchOperation::Put(inserted.clone())],
        limits,
    )
    .expect("general insertion path");
    assert_eq!(general, direct);
    assert_eq!(
        append_persistent_insert(&genesis, &inserted, limits)
            .expect("deterministic replay")
            .bytes,
        direct.bytes
    );
});
