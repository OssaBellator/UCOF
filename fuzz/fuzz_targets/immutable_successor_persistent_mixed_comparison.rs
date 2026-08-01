#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    build_genesis, compare_persistent_mixed_rewrites, ImmutableBatchOperation, ImmutableLimits,
    ImmutableObjectInput, PersistentMixedRewriteRelation, LEAF_CAPACITY,
};

fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, u16::from(1 + seed % 31), vec![seed; payload_len])
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
        .map(|index| {
            let seed = data
                .get(usize::try_from(index).expect("small index") + 1)
                .copied()
                .unwrap_or(index as u8);
            object(index * 2, seed, 1 + usize::from(seed % 64))
        })
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
    let replace_seed = data.get(3).copied().unwrap_or(41);
    let insert_seed = data.get(4).copied().unwrap_or(53);
    let insert_id = u64::try_from(count)
        .expect("small count")
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .expect("bounded identifier");
    let inserted = data.get(5).is_none_or(|byte| byte & 1 == 0);

    let mut operations = vec![
        ImmutableBatchOperation::Delete(delete_id),
        ImmutableBatchOperation::Put(object(
            replace_id,
            replace_seed,
            1 + usize::from(replace_seed % 96),
        )),
    ];
    if inserted {
        operations.push(ImmutableBatchOperation::Put(object(
            insert_id,
            insert_seed,
            1 + usize::from(insert_seed % 96),
        )));
    }

    let forward = compare_persistent_mixed_rewrites(&genesis, &operations, limits)
        .expect("forward comparison");
    operations.reverse();
    let reverse = compare_persistent_mixed_rewrites(&genesis, &operations, limits)
        .expect("reverse comparison");
    assert_eq!(forward, reverse);
    assert_eq!(forward.original_leaf_sizes.iter().sum::<usize>(), count);
    assert_eq!(
        forward.canonical_final_leaf_sizes.iter().sum::<usize>(),
        count - 1 + usize::from(inserted)
    );
    if forward.leaf_partition_equal {
        assert!(forward.comparable_relation.is_some());
        assert!(!matches!(
            forward.comparable_relation,
            Some(PersistentMixedRewriteRelation::CanonicalWritesMore(_))
        ));
    } else {
        assert!(forward.comparable_relation.is_none());
    }
});
