#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_batch_to, build_genesis,
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
    let limits = ImmutableLimits {
        max_file_bytes: 32 * 1024 * 1024,
        max_objects: 2 * LEAF_CAPACITY + 16,
        max_pages: 256,
        max_depth: 4,
        max_allocation_bytes: 32 * 1024 * 1024,
        max_output_bytes: 32 * 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let objects: Vec<_> = (1..=count)
        .map(|index| {
            let seed = data.get(index + 1).copied().unwrap_or(index as u8);
            object(
                u64::try_from(index * 2).expect("small identifier"),
                seed,
                1 + usize::from(seed % 64),
            )
        })
        .collect();
    let base = build_genesis(&objects, limits).expect("bounded canonical genesis");

    let index = data
        .get(1)
        .map_or(0_usize, |byte| usize::from(*byte) % count);
    let existing_id = u64::try_from(index + 1).expect("small index") * 2;
    let inserted_id = u64::try_from(count)
        .expect("small count")
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .expect("bounded identifier");
    let first_seed = data.get(2).copied().unwrap_or(41);
    let second_seed = data.get(3).copied().unwrap_or(53);
    let mode = data.get(4).copied().unwrap_or(0) % 5;
    let mut operations = match mode {
        0 => vec![ImmutableBatchOperation::Put(object(
            existing_id,
            first_seed,
            1 + usize::from(first_seed % 96),
        ))],
        1 => vec![ImmutableBatchOperation::Put(object(
            inserted_id,
            first_seed,
            1 + usize::from(first_seed % 96),
        ))],
        2 => vec![
            ImmutableBatchOperation::Put(object(
                existing_id,
                first_seed,
                1 + usize::from(first_seed % 96),
            )),
            ImmutableBatchOperation::Put(object(
                inserted_id,
                second_seed,
                1 + usize::from(second_seed % 96),
            )),
        ],
        3 => vec![ImmutableBatchOperation::Delete(existing_id)],
        _ => vec![
            ImmutableBatchOperation::Delete(existing_id),
            ImmutableBatchOperation::Put(object(
                inserted_id,
                second_seed,
                1 + usize::from(second_seed % 96),
            )),
        ],
    };
    let chunk = 1 + data.get(5).map_or(63_usize, |byte| usize::from(*byte));

    let owned = append_persistent_batch(&base, &operations, limits).expect("owned batch");
    let mut streamed = Vec::new();
    let report = append_persistent_batch_to(
        &mut streamed,
        &base,
        &operations,
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: chunk,
        },
    )
    .expect("streamed batch");
    assert_eq!(streamed, owned.bytes);
    assert_eq!(report.report, owned.report);
    assert_eq!(report.mode, owned.mode);
    assert_eq!(report.pages_written, owned.pages_written);
    assert_eq!(report.pages_reused, owned.pages_reused);
    assert!(report.largest_write_request <= chunk);
    assert!(report.tail_allocation_bytes < streamed.len());
    assert_eq!(
        validate_canonical_occupancy(&streamed, limits).expect("canonical successor"),
        report.report
    );

    operations.reverse();
    let mut reverse = Vec::new();
    let reverse_report = append_persistent_batch_to(
        &mut reverse,
        &base,
        &operations,
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: chunk,
        },
    )
    .expect("reverse batch");
    assert_eq!(reverse, streamed);
    assert_eq!(reverse_report, report);

    let duplicate = vec![
        ImmutableBatchOperation::Put(object(inserted_id, 1, 1)),
        ImmutableBatchOperation::Delete(inserted_id),
    ];
    let mut rejected = Vec::new();
    assert!(append_persistent_batch_to(
        &mut rejected,
        &base,
        &duplicate,
        limits,
        PersistentMixedStreamingOptions::default(),
    )
    .is_err());
    assert!(rejected.is_empty());
});
