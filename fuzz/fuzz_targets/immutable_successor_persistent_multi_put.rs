#![no_main]

use std::collections::BTreeMap;

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_put_batch, build_genesis,
    validate_canonical_occupancy, ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput,
    PersistentBatchMode, LEAF_CAPACITY,
};

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(2_usize, |byte| 2 + usize::from(*byte) % (2 * LEAF_CAPACITY));
    let limits = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 2 * LEAF_CAPACITY + 16,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 4 * 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let objects: Vec<_> = (1..=u64::try_from(count).expect("small count"))
        .map(|index| {
            let object_id = index * 2;
            let seed = data
                .get(usize::try_from(index).expect("small index"))
                .copied()
                .unwrap_or(index as u8);
            ImmutableObjectInput::new(
                object_id,
                u16::from(1 + seed % 31),
                vec![seed, seed.rotate_left(1)],
            )
        })
        .collect();
    let genesis = build_genesis(&objects, limits).expect("bounded canonical genesis");

    let mut updates = BTreeMap::new();
    let guaranteed_insert = u64::try_from(count).expect("small count") * 2 + 1;
    updates.insert(
        guaranteed_insert,
        ImmutableObjectInput::new(guaranteed_insert, 7, b"guaranteed-insert".to_vec()),
    );
    for (index, byte) in data.iter().skip(1).take(4).enumerate() {
        let selector = usize::from(*byte);
        let object_id = if selector & 1 == 0 {
            2 * u64::try_from(1 + selector % count).expect("small replacement")
        } else {
            1 + 2 * u64::try_from(selector % (count + 1)).expect("small insertion")
        };
        updates.insert(
            object_id,
            ImmutableObjectInput::new(
                object_id,
                u16::try_from(1 + index).expect("small kind"),
                vec![*byte, byte.rotate_left(1)],
            ),
        );
    }
    if updates.len() == 1 {
        let second_insert = guaranteed_insert + 2;
        updates.insert(
            second_insert,
            ImmutableObjectInput::new(second_insert, 9, b"second-insert".to_vec()),
        );
    }
    let forward: Vec<_> = updates.into_values().collect();
    let mut reverse = forward.clone();
    reverse.reverse();

    let direct = append_persistent_put_batch(&genesis, &forward, limits)
        .expect("bounded persistent multi put");
    assert_eq!(direct.mode, PersistentBatchMode::CopyOnWritePutBatch);
    assert_eq!(
        validate_canonical_occupancy(&direct.bytes, limits).expect("multi-put bytes validate"),
        direct.report
    );
    assert_eq!(
        append_persistent_put_batch(&genesis, &reverse, limits)
            .expect("caller-order invariant")
            .bytes,
        direct.bytes
    );

    let operations: Vec<_> = forward
        .into_iter()
        .map(ImmutableBatchOperation::Put)
        .collect();
    assert_eq!(
        append_persistent_batch(&genesis, &operations, limits).expect("general multi-put path"),
        direct
    );
});
