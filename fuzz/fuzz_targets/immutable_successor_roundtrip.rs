#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, validate, ImmutableLimits, ImmutableObjectInput,
};

fuzz_target!(|data: &[u8]| {
    let limits = ImmutableLimits {
        max_file_bytes: 2 << 20,
        max_objects: 32,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 2 << 20,
        max_output_bytes: 2 << 20,
        ..ImmutableLimits::default()
    };

    let desired = data.first().map_or(1_usize, |byte| 1 + usize::from(*byte % 16));
    let source = data.get(1..).unwrap_or_default();
    let mut objects = Vec::with_capacity(desired);
    for index in 0..desired {
        let seed = source.get(index).copied().unwrap_or(index as u8);
        let start = source
            .len()
            .checked_mul(index)
            .and_then(|value| value.checked_div(desired))
            .unwrap_or(0);
        let end = source
            .len()
            .checked_mul(index + 1)
            .and_then(|value| value.checked_div(desired))
            .unwrap_or(source.len());
        let payload = source.get(start..end).unwrap_or_default().to_vec();
        objects.push(ImmutableObjectInput::new(
            u64::try_from(index + 1).expect("small object identifier"),
            u16::from(1 + seed % 31),
            payload,
        ));
    }

    let genesis = build_genesis(&objects, limits).expect("bounded generated genesis");
    let genesis_report = validate(&genesis, limits).expect("generated genesis validates");
    assert_eq!(genesis_report.sequence, 0);
    assert_eq!(genesis_report.object_count, objects.len());

    let selected = data
        .first()
        .map_or(0_usize, |byte| usize::from(*byte) % objects.len());
    let mut replacement_payload = objects[selected].payload.clone();
    replacement_payload.reverse();
    replacement_payload.extend_from_slice(b":replacement");
    let replacement = ImmutableObjectInput::new(
        objects[selected].object_id,
        objects[selected].kind,
        replacement_payload,
    );
    let appended = append_replacement(&genesis, &replacement, limits)
        .expect("bounded replacement append");
    let append_report = validate(&appended, limits).expect("replacement append validates");
    assert_eq!(append_report.sequence, 1);
    assert_eq!(append_report.object_count, objects.len());
});
