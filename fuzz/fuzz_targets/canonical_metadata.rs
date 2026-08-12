#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_core::{decode_canonical, Limits};

fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_metadata_bytes: 1024 * 1024,
        max_allocation_bytes: 1024 * 1024,
        max_metadata_depth: 32,
        max_container_items: 4096,
        max_text_bytes: 256 * 1024,
        max_byte_string_bytes: 256 * 1024,
        ..Limits::default()
    };
    let _ = decode_canonical(data, &limits);
});
