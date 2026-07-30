#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::exp0002::ValidationLimits;
use ucof_experiments::exp0002_lookup::{lookup_authenticated, AuthenticatedLookupLimits};

fuzz_target!(|data: &[u8]| {
    let mut id_bytes = [0_u8; 8];
    let prefix = data.len().min(8);
    id_bytes[..prefix].copy_from_slice(&data[..prefix]);
    let object_id = u64::from_le_bytes(id_bytes).max(1);
    let file = data.get(prefix..).unwrap_or_default();
    let file_len = u64::try_from(file.len()).unwrap_or(u64::MAX);
    let limits = AuthenticatedLookupLimits {
        validation: ValidationLimits {
            max_file_bytes: file_len,
            max_commit_bytes: file_len,
            max_snapshot_bytes: file_len.min(1024 * 1024),
            max_pages: 64,
            max_page_depth: 16,
            max_objects: 4096,
            max_payload_bytes: file_len,
            max_hashed_bytes: file_len.saturating_mul(4),
            max_roots: 4096,
            max_capabilities: 1024,
        },
        max_page_reads: 16,
    };
    let _ = lookup_authenticated(file, object_id, &limits);
});
