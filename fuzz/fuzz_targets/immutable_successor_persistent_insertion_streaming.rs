#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_insert, append_persistent_insert_to, build_genesis,
    validate_canonical_occupancy, ImmutableLimits, ImmutableObjectInput, PersistentBatchMode,
    PersistentMixedStreamingOptions, LEAF_CAPACITY,
};

fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, u16::from(1 + seed % 31), vec![seed; payload_len])
}

fuzz_target!(|data: &[u8]| {
    let count = data
        .first()
        .map_or(1_usize, |byte| 1 + usize::from(*byte) % (2 * LEAF_CAPACITY));
    let limits = ImmutableLimits {
        max_file_bytes: 32 * 1024 * 1024,
        max_objects: 2 * LEAF_CAPACITY + 8,
        max_pages: 128,
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

    let insertion_position = data
        .get(2)
        .map_or(count, |byte| usize::from(*byte) % (count + 1));
    let absent_id = u64::try_from(insertion_position)
        .expect("small position")
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .expect("bounded identifier");
    let duplicate = data.get(3).is_some_and(|byte| byte & 1 != 0);
    let object_id = if duplicate {
        let index = data
            .get(4)
            .map_or(0_usize, |byte| usize::from(*byte) % count);
        u64::try_from(index + 1).expect("small index") * 2
    } else {
        absent_id
    };
    let seed = data.get(5).copied().unwrap_or(211);
    let input = object(object_id, seed, 1 + usize::from(seed % 96));
    let chunk = 1 + data.get(6).map_or(63_usize, |byte| usize::from(*byte));

    let mut streamed = Vec::new();
    let result = append_persistent_insert_to(
        &mut streamed,
        &base,
        &input,
        limits,
        PersistentMixedStreamingOptions {
            max_write_request_bytes: chunk,
        },
    );
    if duplicate {
        assert!(result.is_err());
        assert!(streamed.is_empty());
        return;
    }

    let report = result.expect("streamed insertion");
    let owned = append_persistent_insert(&base, &input, limits).expect("owned insertion");
    assert_eq!(streamed, owned.bytes);
    assert_eq!(report.report, owned.report);
    assert_eq!(report.mode, PersistentBatchMode::CopyOnWriteInsertion);
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
    let validated = validate_canonical_occupancy(&streamed, limits).expect("canonical successor");
    assert_eq!(validated, report.report);
});
