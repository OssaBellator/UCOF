#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::exp0002::ValidationLimits;
use ucof_experiments::exp0002_source::{Exp0002SliceSource, Exp0002SourceLimits};
use ucof_experiments::{
    scan_valid_prefixes_at, validate_strict_at, Exp0002SourceRecoveryLimits,
};

fuzz_target!(|data: &[u8]| {
    let file_len = u64::try_from(data.len()).unwrap_or(u64::MAX);
    let source_limits = Exp0002SourceLimits {
        validation: ValidationLimits {
            max_file_bytes: file_len,
            max_commit_bytes: file_len,
            max_snapshot_bytes: file_len.min(1024 * 1024),
            max_pages: 128,
            max_page_depth: 16,
            max_objects: 8192,
            max_payload_bytes: file_len,
            max_hashed_bytes: file_len.saturating_mul(8),
            max_roots: 4096,
            max_capabilities: 1024,
        },
        max_source_bytes_read: file_len.saturating_mul(8).saturating_add(64 * 1024),
        max_read_operations: 8192,
        max_read_request_bytes: 64 * 1024,
        hash_block_bytes: 4096,
        max_page_reads: 128,
    };

    let mut strict_source = Exp0002SliceSource::new(data);
    let _ = validate_strict_at(&mut strict_source, &source_limits);

    let mut recovery_source = Exp0002SliceSource::new(data);
    let _ = scan_valid_prefixes_at(
        &mut recovery_source,
        &Exp0002SourceRecoveryLimits {
            candidate: source_limits,
            max_scan_bytes: file_len.min(64 * 1024),
            max_scan_read_operations: 16,
            max_magic_matches: 64,
            max_candidate_validations: 32,
            max_results: 8,
            max_total_candidate_bytes_read: file_len
                .saturating_mul(32)
                .saturating_add(64 * 1024),
        },
    );
});
