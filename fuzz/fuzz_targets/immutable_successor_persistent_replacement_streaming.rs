#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_replacement_batch_to, build_genesis,
    validate_canonical_occupancy, ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput,
    PersistentMixedStreamingOptions,
};

fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, u16::from(1 + seed % 31), vec![seed; payload_len])
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(2_usize, |byte| 2 + usize::from(*byte) % 480);
    let chunk = data
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
            let object_id = u64::try_from((index + 1) * 2).expect("object id");
            let seed = data.get(index + 2).copied().unwrap_or(index as u8);
            object(object_id, seed, 1 + usize::from(seed % 96))
        })
        .collect();
    let base = build_genesis(&objects, limits).expect("bounded base");

    let first_index = data
        .get(count + 2)
        .map_or(0_usize, |byte| usize::from(*byte) % count);
    let second_offset = data
        .get(count + 3)
        .map_or(1_usize, |byte| 1 + usize::from(*byte) % (count - 1));
    let second_index = (first_index + second_offset) % count;
    let first_id = u64::try_from((first_index + 1) * 2).expect("first id");
    let second_id = u64::try_from((second_index + 1) * 2).expect("second id");
    let first_seed = data.get(count + 4).copied().unwrap_or(201);
    let second_seed = data.get(count + 5).copied().unwrap_or(203);
    let mut operations = vec![
        ImmutableBatchOperation::Put(object(
            first_id,
            first_seed,
            1 + usize::from(first_seed % 96),
        )),
        ImmutableBatchOperation::Put(object(
            second_id,
            second_seed,
            1 + usize::from(second_seed % 96),
        )),
    ];

    let owned = append_persistent_batch(&base, &operations, limits).expect("owned replacement");
    let mut streamed = Vec::new();
    let forward = append_persistent_replacement_batch_to(
        &mut streamed,
        &base,
        &operations,
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: chunk,
        },
    )
    .expect("streamed replacement");
    assert_eq!(streamed, owned.bytes);
    assert_eq!(forward.report, owned.report);
    assert_eq!(forward.pages_written, owned.pages_written);
    assert_eq!(forward.pages_reused, owned.pages_reused);
    assert_eq!(
        forward.base_bytes_written,
        u64::try_from(base.len()).expect("base bytes")
    );
    assert_eq!(
        forward.tail_bytes_written,
        u64::try_from(streamed.len() - base.len()).expect("tail bytes")
    );
    assert!(forward.largest_write_request <= chunk);
    assert!(forward.tail_allocation_bytes < streamed.len());
    assert_eq!(
        validate_canonical_occupancy(&streamed, limits).expect("canonical streamed output"),
        owned.report
    );

    operations.reverse();
    let mut reversed_bytes = Vec::new();
    let reverse = append_persistent_replacement_batch_to(
        &mut reversed_bytes,
        &base,
        &operations,
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: chunk,
        },
    )
    .expect("reversed replacements");
    assert_eq!(reversed_bytes, streamed);
    assert_eq!(reverse, forward);

    let missing_id = u64::try_from(count * 2 + 1).expect("missing id");
    let mut untouched = Vec::new();
    assert!(append_persistent_replacement_batch_to(
        &mut untouched,
        &base,
        &[ImmutableBatchOperation::Put(object(missing_id, 211, 7))],
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: chunk,
        },
    )
    .is_err());
    assert!(untouched.is_empty());
});
