#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_mixed_batch, build_genesis, compare_persistent_mixed_leaf_rewrites,
    validate_canonical_occupancy, ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput,
};

fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::from(1 + seed % 31),
        vec![seed; payload_len],
    )
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(2_usize, |byte| 2 + usize::from(*byte) % 480);
    let payload_len = data
        .get(1)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
    let limits = ImmutableLimits {
        max_file_bytes: 32 * 1024 * 1024,
        max_objects: 1_024,
        max_pages: 128,
        max_depth: 4,
        max_allocation_bytes: 16 * 1024 * 1024,
        max_output_bytes: 32 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let objects: Vec<_> = (0..count)
        .map(|index| {
            let object_id = u64::try_from((index + 1) * 2).expect("bounded object id");
            let seed = data.get(index + 2).copied().unwrap_or(index as u8);
            object(object_id, seed, payload_len)
        })
        .collect();
    let base = build_genesis(&objects, limits).expect("bounded canonical base");

    let delete_index = data
        .get(count + 2)
        .map_or(0_usize, |byte| usize::from(*byte) % count);
    let replacement_offset = data
        .get(count + 3)
        .map_or(1_usize, |byte| 1 + usize::from(*byte) % (count - 1));
    let replacement_index = (delete_index + replacement_offset) % count;
    let delete_id = u64::try_from((delete_index + 1) * 2).expect("delete id");
    let replacement_id =
        u64::try_from((replacement_index + 1) * 2).expect("replacement id");
    let replacement_seed = data.get(count + 4).copied().unwrap_or(199);
    let mut operations = vec![
        ImmutableBatchOperation::Delete(delete_id),
        ImmutableBatchOperation::Put(object(
            replacement_id,
            replacement_seed,
            payload_len,
        )),
    ];
    if data
        .get(count + 5)
        .is_some_and(|byte| byte & 1 != 0)
    {
        let insertion_index = data
            .get(count + 6)
            .map_or(count / 2, |byte| usize::from(*byte) % (count + 1));
        let insertion_id =
            u64::try_from(insertion_index * 2 + 1).expect("bounded insertion id");
        operations.push(ImmutableBatchOperation::Put(object(
            insertion_id,
            data.get(count + 7).copied().unwrap_or(211),
            payload_len,
        )));
    }

    let report = compare_persistent_mixed_leaf_rewrites(&base, &operations, limits)
        .expect("mixed rewrite comparison");
    let mut reversed = operations.clone();
    reversed.reverse();
    assert_eq!(
        report,
        compare_persistent_mixed_leaf_rewrites(&base, &reversed, limits)
            .expect("order-independent comparison")
    );

    let expected_objects = count + report.insertions - report.deletions;
    assert_eq!(report.path_local_final_leaf_counts.iter().sum::<usize>(), expected_objects);
    assert_eq!(report.canonical_final_leaf_counts.iter().sum::<usize>(), expected_objects);
    assert_eq!(
        report.path_local_leaf_pages_written + report.path_local_leaf_pages_reused,
        report.final_leaf_pages
    );
    assert_eq!(
        report.canonical_leaf_pages_written + report.canonical_leaf_pages_reused,
        report.final_leaf_pages
    );
    assert!(report.path_local_touched_original_leaf_pages <= report.original_leaf_pages);
    assert_eq!(
        report.extra_canonical_leaf_writes,
        report
            .canonical_leaf_pages_written
            .saturating_sub(report.path_local_leaf_pages_written)
    );
    assert_eq!(report.exact_leaf_layout_match, report.first_differing_leaf.is_none());
    if report.exact_leaf_layout_match {
        assert_eq!(report.path_local_final_leaf_counts, report.canonical_final_leaf_counts);
    }

    let result = append_persistent_mixed_batch(&base, &operations, limits)
        .expect("authenticated canonical mixed writer");
    let validated = validate_canonical_occupancy(&result.bytes, limits)
        .expect("canonical mixed output");
    assert_eq!(validated, result.report);
    assert_eq!(validated.object_count, expected_objects);
});
