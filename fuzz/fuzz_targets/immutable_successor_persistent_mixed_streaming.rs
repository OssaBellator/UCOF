#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_mixed_batch, append_persistent_mixed_batch_to, build_genesis,
    validate_canonical_occupancy, ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput,
    PersistentMixedStreamingOptions, LEAF_CAPACITY,
};

fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, u16::from(1 + seed % 31), vec![seed; payload_len])
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(4_usize, |byte| 4 + usize::from(*byte) % (2 * LEAF_CAPACITY));
    let write_chunk = data
        .get(1)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 96));
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
                .get(2 + usize::try_from(index).expect("small index") % data.len().max(1))
                .copied()
                .unwrap_or(index as u8);
            object(index * 2, seed, 1 + usize::from(seed % 64))
        })
        .collect();
    let genesis = build_genesis(&objects, limits).expect("bounded canonical genesis");

    let delete_index = data
        .get(2)
        .map_or(0_usize, |byte| usize::from(*byte) % count);
    let delete_id = u64::try_from(delete_index + 1).expect("small index") * 2;
    let replace_index = data.get(3).map_or((delete_index + 1) % count, |byte| {
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
    let replace_seed = data.get(4).copied().unwrap_or(17);
    let insert_seed = data.get(5).copied().unwrap_or(29);

    let mut operations = vec![
        ImmutableBatchOperation::Delete(delete_id),
        ImmutableBatchOperation::Put(object(
            replace_id,
            replace_seed,
            1 + usize::from(replace_seed % 96),
        )),
    ];
    if data.get(6).is_none_or(|byte| byte & 1 == 0) {
        operations.push(ImmutableBatchOperation::Put(object(
            inserted_id,
            insert_seed,
            1 + usize::from(insert_seed % 96),
        )));
    }

    let expected = append_persistent_mixed_batch(&genesis, &operations, limits)
        .expect("bounded owned mixed batch");
    let mut actual = Vec::new();
    let report = append_persistent_mixed_batch_to(
        &mut actual,
        &genesis,
        &operations,
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: write_chunk,
        },
    )
    .expect("bounded streamed mixed batch");
    assert_eq!(actual, expected.bytes);
    assert_eq!(report.report, expected.report);
    assert_eq!(report.pages_written, expected.pages_written);
    assert_eq!(report.pages_reused, expected.pages_reused);
    assert_eq!(report.mode, expected.mode);
    assert_eq!(
        report.tail_allocation_bytes,
        actual
            .len()
            .checked_sub(genesis.len())
            .expect("append tail")
    );
    assert!(report.tail_allocation_bytes < actual.len());
    assert!(report.largest_write_request <= write_chunk);
    assert_eq!(
        validate_canonical_occupancy(&actual, limits).expect("streamed validation"),
        report.report
    );

    operations.reverse();
    let mut reversed = Vec::new();
    append_persistent_mixed_batch_to(
        &mut reversed,
        &genesis,
        &operations,
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: write_chunk,
        },
    )
    .expect("reversed streamed mixed batch");
    assert_eq!(reversed, actual);
});
