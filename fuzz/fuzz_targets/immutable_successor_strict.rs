#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{validate, ImmutableLimits};

fuzz_target!(|data: &[u8]| {
    let limits = ImmutableLimits {
        max_file_bytes: 1 << 20,
        max_objects: 4_096,
        max_pages: 1_024,
        max_depth: 8,
        max_allocation_bytes: 2 << 20,
        max_output_bytes: 2 << 20,
        ..ImmutableLimits::default()
    };
    let _ = validate(data, limits);
});
