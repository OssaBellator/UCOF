#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_core::{DiagnosticValidator, Limits, SliceSource, SourceValidator, ValidatedFile};

fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_file_bytes: 4 * 1024 * 1024,
        max_total_bytes_read: 8 * 1024 * 1024,
        max_payload_bytes: 2 * 1024 * 1024,
        max_metadata_bytes: 1024 * 1024,
        max_allocation_bytes: 1024 * 1024,
        ..Limits::default()
    };

    let _ = ValidatedFile::parse(data, &limits);

    let mut source = SliceSource::new(data);
    let _ = SourceValidator::new(limits).validate(&mut source);

    let mut source = SliceSource::new(data);
    let _ = DiagnosticValidator::new(limits).diagnose(&mut source);
});
