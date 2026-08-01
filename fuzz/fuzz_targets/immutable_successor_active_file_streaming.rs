#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, rewrite_active_file_to, rewrite_all,
    validate_canonical_occupancy, ImmutableLimits, ImmutableObjectInput,
    ImmutableSourceStreamingWriteOptions, ImmutableStreamingWriteOptions,
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
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 16));
    let source_chunk = data
        .get(1)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let sink_chunk = data
        .get(2)
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 64));
    let limits = ImmutableLimits {
        max_file_bytes: 4 * 1024 * 1024,
        max_objects: 32,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        ..ImmutableLimits::default()
    };

    let objects: Vec<_> = (0..count)
        .map(|index| {
            let object_id = u64::try_from(index + 1).expect("small object id");
            let seed = data.get(index + 3).copied().unwrap_or(index as u8);
            object(object_id, seed, 1 + usize::from(seed % 96))
        })
        .collect();
    let genesis = build_genesis(&objects, limits).expect("bounded genesis");
    let source = if data.get(3 + count).is_some_and(|byte| byte & 1 != 0) {
        let selected = data
            .get(4 + count)
            .map_or(0_usize, |byte| usize::from(*byte) % count);
        let object_id = u64::try_from(selected + 1).expect("small object id");
        let seed = data.get(5 + count).copied().unwrap_or(73);
        append_replacement(
            &genesis,
            &object(object_id, seed, 1 + usize::from(seed % 96)),
            limits,
        )
        .expect("bounded replacement")
    } else {
        genesis
    };

    let expected = rewrite_all(&source, limits).expect("owned rewrite");
    let mut actual = Vec::new();
    let report = rewrite_active_file_to(
        &mut actual,
        &source,
        ImmutableSourceStreamingWriteOptions {
            output: ImmutableStreamingWriteOptions {
                max_write_request_bytes: sink_chunk,
            },
            max_source_read_bytes: source_chunk,
        },
        limits,
    )
    .expect("active streaming rewrite");
    assert_eq!(actual, expected.bytes);
    assert_eq!(report.source, expected.source);
    assert_eq!(report.output.output.report, expected.output);
    assert!(report.largest_payload_read_request <= source_chunk);
    assert!(report.output.output.largest_write_request <= sink_chunk);
    assert_eq!(
        validate_canonical_occupancy(&actual, limits).expect("canonical output"),
        report.output.output.report
    );

    let mut tampered = source;
    let last = tampered.last_mut().expect("non-empty source");
    *last ^= 1;
    let mut untouched = Vec::new();
    assert!(rewrite_active_file_to(
        &mut untouched,
        &tampered,
        ImmutableSourceStreamingWriteOptions {
            output: ImmutableStreamingWriteOptions {
                max_write_request_bytes: sink_chunk,
            },
            max_source_read_bytes: source_chunk,
        },
        limits,
    )
    .is_err());
    assert!(untouched.is_empty());
});
