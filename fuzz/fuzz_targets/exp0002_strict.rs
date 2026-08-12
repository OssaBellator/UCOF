#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::exp0002::{validate_strict, ValidationLimits};

fuzz_target!(|data: &[u8]| {
    let length = u64::try_from(data.len()).unwrap_or(u64::MAX);
    let limits = ValidationLimits {
        max_file_bytes: length,
        max_commit_bytes: length,
        max_snapshot_bytes: length.min(1024 * 1024),
        max_pages: 256,
        max_page_depth: 16,
        max_objects: 4096,
        max_payload_bytes: length,
        max_hashed_bytes: length.saturating_mul(4),
        max_roots: 4096,
        max_capabilities: 1024,
    };
    let _ = validate_strict(data, &limits);
});
