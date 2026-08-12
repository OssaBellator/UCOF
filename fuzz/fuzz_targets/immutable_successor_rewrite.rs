#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, rewrite_all, rewrite_selected, validate, ImmutableLimits,
    ImmutableObjectInput,
};

fuzz_target!(|data: &[u8]| {
    let limits = ImmutableLimits {
        max_file_bytes: 2 << 20,
        max_objects: 16,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 2 << 20,
        max_output_bytes: 2 << 20,
        max_history_entries: 8,
        max_recovery_scan_bytes: 2 << 20,
        max_recovery_attempts: 128,
        max_recovery_candidates: 8,
    };

    let desired = data
        .first()
        .map_or(1_usize, |byte| 1 + usize::from(*byte % 8));
    let source = data.get(1..).unwrap_or_default();
    let mut objects = Vec::with_capacity(desired);
    for index in 0..desired {
        let start = source.len().saturating_mul(index) / desired;
        let end = source.len().saturating_mul(index + 1) / desired;
        objects.push(ImmutableObjectInput::new(
            u64::try_from(index + 1).expect("small identifier"),
            u16::try_from(1 + index % 31).expect("small kind"),
            source.get(start..end).unwrap_or_default().to_vec(),
        ));
    }

    let genesis = build_genesis(&objects, limits).expect("bounded genesis");
    let selected = data
        .get(1)
        .map_or(0_usize, |byte| usize::from(*byte) % objects.len());
    let mut replacement_payload = objects[selected].payload.clone();
    replacement_payload.extend_from_slice(b":rewrite-source");
    let appended = append_replacement(
        &genesis,
        &ImmutableObjectInput::new(
            objects[selected].object_id,
            objects[selected].kind,
            replacement_payload,
        ),
        limits,
    )
    .expect("bounded append");

    let rewrite = if data.get(2).is_some_and(|byte| byte & 1 == 1) {
        let mut ids: Vec<u64> = objects
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                data.get(3 + index / 8)
                    .map_or(index % 2 == 0, |byte| byte & (1 << (index % 8)) != 0)
            })
            .map(|(_, object)| object.object_id)
            .collect();
        if ids.is_empty() {
            ids.push(objects[0].object_id);
        }
        rewrite_selected(&appended, &ids, limits).expect("selected rewrite")
    } else {
        rewrite_all(&appended, limits).expect("rewrite all")
    };

    let report = validate(&rewrite.bytes, limits).expect("rewritten output validates");
    assert_eq!(report, rewrite.output);
    assert_eq!(report.sequence, 0);
    assert_eq!(report.object_count, rewrite.retained_object_ids.len());
    assert!(!rewrite.byte_scoped_signatures_preserved);
    assert!(rewrite
        .retained_object_ids
        .windows(2)
        .all(|pair| pair[0] < pair[1]));
});
