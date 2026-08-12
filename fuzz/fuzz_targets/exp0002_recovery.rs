#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::exp0002::ValidationLimits;
use ucof_experiments::exp0002_recovery::{scan_valid_prefixes, Exp0002RecoveryLimits};

fuzz_target!(|data: &[u8]| {
    let length = u64::try_from(data.len()).unwrap_or(u64::MAX);
    let validation = ValidationLimits {
        max_file_bytes: length,
        max_commit_bytes: length,
        max_snapshot_bytes: length.min(1024 * 1024),
        max_pages: 128,
        max_page_depth: 16,
        max_objects: 2048,
        max_payload_bytes: length,
        max_hashed_bytes: length.saturating_mul(4),
        max_roots: 2048,
        max_capabilities: 1024,
    };
    let recovery = Exp0002RecoveryLimits {
        max_scan_bytes: data.len().min(256 * 1024),
        max_magic_matches: 128,
        max_candidate_validations: 32,
        max_results: 16,
        max_chain_depth: 16,
    };
    let _ = scan_valid_prefixes(data, &validation, &recovery);
});
