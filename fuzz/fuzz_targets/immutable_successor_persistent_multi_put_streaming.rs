#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_put_batch, append_persistent_put_batch_to, build_genesis,
    validate_canonical_occupancy, ImmutableLimits, ImmutableObjectInput, PersistentBatchMode,
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

    let first_index = data
        .get(1)
        .map_or(0_usize, |byte| usize::from(*byte) % count);
    let mut second_index = data
        .get(2)
        .map_or((first_index + 1) % count, |byte| usize::from(*byte) % count);
    if second_index == first_index {
        second_index = (second_index + 1) % count;
    }
    let first_id = u64::try_from(first_index + 1).expect("small index") * 2;
    let second_id = u64::try_from(second_index + 1).expect("small index") * 2;
    let first_seed = data.get(3).copied().unwrap_or(41);
    let second_seed = data.get(4).copied().unwrap_or(53);
    let insert_seed = data.get(5).copied().unwrap_or(67);
    let first_insert_id = u64::try_from(count)
        .expect("small count")
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .expect("bounded identifier");
    let second_insert_id = first_insert_id.checked_add(2).expect("bounded identifier");

    let mut inputs = vec![
        object(first_id, first_seed, 1 + usize::from(first_seed % 96)),
        object(
            first_insert_id,
            insert_seed,
            1 + usize::from(insert_seed % 96),
        ),
    ];
    if data.get(6).is_none_or(|byte| byte & 1 == 0) {
        inputs.push(object(
            second_id,
            second_seed,
            1 + usize::from(second_seed % 96),
        ));
    } else {
        inputs.push(object(
            second_insert_id,
            second_seed,
            1 + usize::from(second_seed % 96),
        ));
    }
    let chunk = 1 + data.get(7).map_or(63_usize, |byte| usize::from(*byte));

    let owned = append_persistent_put_batch(&base, &inputs, limits).expect("owned multi put");
    let mut streamed = Vec::new();
    let report = append_persistent_put_batch_to(
        &mut streamed,
        &base,
        &inputs,
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: chunk,
        },
    )
    .expect("streamed multi put");
    assert_eq!(streamed, owned.bytes);
    assert_eq!(report.report, owned.report);
    assert_eq!(report.mode, PersistentBatchMode::CopyOnWritePutBatch);
    assert_eq!(report.pages_written, owned.pages_written);
    assert_eq!(report.pages_reused, owned.pages_reused);
    assert_eq!(
        report.base_bytes_written,
        u64::try_from(base.len()).expect("base bytes")
    );
    assert_eq!(
        report.tail_bytes_written,
        u64::try_from(streamed.len() - base.len()).expect("tail bytes")
    );
    assert!(report.largest_write_request <= chunk);
    assert!(report.tail_allocation_bytes < streamed.len());
    assert_eq!(
        validate_canonical_occupancy(&streamed, limits).expect("canonical successor"),
        report.report
    );

    inputs.reverse();
    let mut reverse = Vec::new();
    let reverse_report = append_persistent_put_batch_to(
        &mut reverse,
        &base,
        &inputs,
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: chunk,
        },
    )
    .expect("reverse multi put");
    assert_eq!(reverse, streamed);
    assert_eq!(reverse_report.report, report.report);

    let duplicate = vec![object(first_insert_id, 1, 1), object(first_insert_id, 2, 2)];
    let mut rejected = Vec::new();
    assert!(append_persistent_put_batch_to(
        &mut rejected,
        &base,
        &duplicate,
        limits,
        PersistentMixedStreamingOptions::default(),
    )
    .is_err());
    assert!(rejected.is_empty());
});
