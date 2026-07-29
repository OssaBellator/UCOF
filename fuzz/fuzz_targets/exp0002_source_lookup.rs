#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::exp0002::ValidationLimits;
use ucof_experiments::exp0002_source::{
    lookup_authenticated_at, Exp0002SliceSource, Exp0002SourceLimits,
};

fuzz_target!(|data: &[u8]| {
    let prefix = data.len().min(8);
    let mut id_bytes = [0_u8; 8];
    id_bytes[..prefix].copy_from_slice(&data[..prefix]);
    let object_id = u64::from_le_bytes(id_bytes).max(1);
    let file = data.get(prefix..).unwrap_or_default();
    let file_len = u64::try_from(file.len()).unwrap_or(u64::MAX);
    let mut source = Exp0002SliceSource::new(file);
    let limits = Exp0002SourceLimits {
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
        max_source_bytes_read: file_len.saturating_mul(4),
        max_read_operations: 4096,
        max_read_request_bytes: 64 * 1024,
        hash_block_bytes: 4096,
        max_page_reads: 16,
    };
    let _ = lookup_authenticated_at(&mut source, object_id, &limits);
});
