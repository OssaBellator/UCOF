#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::exp0002::{
    build_genesis, validate_strict, FileHeader, ObjectInput, ValidationLimits,
};
use ucof_experiments::exp0002_rewrite::{
    repair_all_to_new_file, rewrite_selected_to_new_file, RewriteLimits,
};

fuzz_target!(|data: &[u8]| {
    let count = usize::from(data.first().copied().unwrap_or(0) % 16) + 1;
    let mut cursor = 1_usize.min(data.len());
    let mut objects = Vec::with_capacity(count);
    for index in 0..count {
        let requested = usize::from(data.get(cursor).copied().unwrap_or(0) % 64);
        cursor = cursor.saturating_add(1).min(data.len());
        let end = cursor.saturating_add(requested).min(data.len());
        let payload = data[cursor..end].to_vec();
        cursor = end;
        objects.push(ObjectInput {
            object_id: u64::try_from(index + 1).expect("bounded object index"),
            kind: 1,
            payload,
            is_root: index == 0,
        });
    }
    let validation = ValidationLimits {
        max_file_bytes: 8 * 1024 * 1024,
        max_commit_bytes: 8 * 1024 * 1024,
        max_snapshot_bytes: 1024 * 1024,
        max_pages: 1024,
        max_page_depth: 16,
        max_objects: 1024,
        max_payload_bytes: 8 * 1024 * 1024,
        max_hashed_bytes: 32 * 1024 * 1024,
        max_roots: 1024,
        max_capabilities: 1024,
    };
    let rewrite = RewriteLimits {
        validation: validation.clone(),
        max_objects_copied: 1024,
        max_payload_bytes_copied: 8 * 1024 * 1024,
        max_output_bytes: 8 * 1024 * 1024,
    };
    let source = build_genesis(
        FileHeader {
            file_id: [1; 16],
            creation_nonce: [2; 16],
        },
        objects,
    )
    .expect("generated source");
    let repair = repair_all_to_new_file(
        &source,
        FileHeader {
            file_id: [3; 16],
            creation_nonce: [4; 16],
        },
        &rewrite,
    )
    .expect("generated repair");
    validate_strict(&repair.output, &validation).expect("repair output");

    let retained_count = usize::from(data.get(cursor).copied().unwrap_or(0)) % count + 1;
    let retained: Vec<u64> = (1..=retained_count)
        .map(|value| u64::try_from(value).expect("bounded retained id"))
        .collect();
    let compacted = rewrite_selected_to_new_file(
        &source,
        FileHeader {
            file_id: [5; 16],
            creation_nonce: [6; 16],
        },
        &retained,
        &[1],
        &rewrite,
    )
    .expect("generated compaction");
    let report = validate_strict(&compacted.output, &validation).expect("compaction output");
    assert_eq!(report.objects.len(), retained_count);
});
