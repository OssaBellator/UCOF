#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_core::{Limits, MetadataInspector, SliceSource};

fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_file_bytes: 4 * 1024 * 1024,
        max_total_bytes_read: 2 * 1024 * 1024,
        max_payload_bytes: 2 * 1024 * 1024,
        max_metadata_bytes: 512 * 1024,
        max_allocation_bytes: 512 * 1024,
        max_records: 4096,
        ..Limits::default()
    };
    let mut source = SliceSource::new(data);
    let _ = MetadataInspector::new(limits).inspect(&mut source);
});
