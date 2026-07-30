#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, scan_recovery_candidates, validate, validate_history,
    ImmutableError, ImmutableLimits, ImmutableObjectInput, FOOTER_LEN,
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
        let payload = source.get(start..end).unwrap_or_default().to_vec();
        objects.push(ImmutableObjectInput::new(
            u64::try_from(index + 1).expect("small object identifier"),
            u16::try_from(1 + index % 31).expect("small object kind"),
            payload,
        ));
    }

    let genesis = build_genesis(&objects, limits).expect("bounded genesis");
    let selected = data
        .get(1)
        .map_or(0_usize, |byte| usize::from(*byte) % objects.len());
    let mut replacement_payload = objects[selected].payload.clone();
    replacement_payload.extend_from_slice(b":history");
    let replacement = ImmutableObjectInput::new(
        objects[selected].object_id,
        objects[selected].kind,
        replacement_payload,
    );
    let appended = append_replacement(&genesis, &replacement, limits).expect("bounded append");

    let history = validate_history(&appended, limits).expect("history validates");
    assert_eq!(history.entries.len(), 2);
    assert_eq!(history.entries[0].report.sequence, 1);
    assert_eq!(history.entries[1].report.sequence, 0);

    let recovery = scan_recovery_candidates(&appended, limits).expect("recovery scan");
    assert_eq!(recovery.candidates.len(), 2);
    assert_eq!(recovery.candidates[0].report.sequence, 1);
    assert_eq!(recovery.candidates[1].report.sequence, 0);
    assert_eq!(recovery.candidates[0].prefix_len as usize, appended.len());
    assert_eq!(recovery.candidates[1].prefix_len as usize, genesis.len());

    let cut = data
        .get(2)
        .map_or(1_usize, |byte| 1 + usize::from(*byte) % FOOTER_LEN);
    let interrupted = &appended[..appended.len() - cut];
    let interrupted_report =
        scan_recovery_candidates(interrupted, limits).expect("interrupted recovery scan");
    assert!(interrupted_report
        .candidates
        .iter()
        .all(|candidate| candidate.report.sequence == 0));
    assert!(interrupted_report
        .candidates
        .iter()
        .any(|candidate| candidate.prefix_len as usize == genesis.len()));

    let mut damaged_history = appended.clone();
    let previous_footer = genesis.len() - FOOTER_LEN;
    damaged_history[previous_footer + 80] ^= 0x01;
    assert_eq!(
        validate(&damaged_history, limits)
            .expect("newest commit remains valid")
            .sequence,
        1
    );
    assert_eq!(
        validate_history(&damaged_history, limits),
        Err(ImmutableError::Invalid("commit digest"))
    );
});
