#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    lookup_at, ImmutableLimits, ImmutableSliceSource, ImmutableSourceLimits,
};

fuzz_target!(|data: &[u8]| {
    let object_id = data
        .get(..8)
        .and_then(|bytes| bytes.try_into().ok())
        .map_or(1_u64, u64::from_le_bytes)
        .max(1);
    let bytes = data.get(8..).unwrap_or_default();
    let limits = ImmutableSourceLimits {
        format: ImmutableLimits {
            max_file_bytes: 1 << 20,
            max_objects: 4_096,
            max_pages: 1_024,
            max_depth: 8,
            max_allocation_bytes: 2 << 20,
            max_output_bytes: 2 << 20,
            max_history_entries: 32,
            max_recovery_scan_bytes: 1 << 20,
            max_recovery_attempts: 256,
            max_recovery_candidates: 16,
        },
        max_total_bytes_read: 4 << 20,
        max_read_operations: 4_096,
        max_read_request_bytes: 4 * 1024,
        hash_block_bytes: 4 * 1024,
    };
    let mut source = ImmutableSliceSource::new(bytes);
    let _ = lookup_at(&mut source, object_id, limits);
});
