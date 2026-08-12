#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_core::{Limits, PrefixSalvager, SliceSource};

fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_file_bytes: 4 * 1024 * 1024,
        max_total_bytes_read: 512 * 1024,
        max_payload_bytes: 2 * 1024 * 1024,
        max_records: 4096,
        max_diagnostics: 8,
        ..Limits::default()
    };
    let mut source = SliceSource::new(data);
    let _ = PrefixSalvager::new(limits).scan(&mut source);
});
