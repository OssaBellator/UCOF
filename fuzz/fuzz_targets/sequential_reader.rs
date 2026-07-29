#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;
use ucof_core::{Limits, SequentialReader};

fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_file_bytes: 4 * 1024 * 1024,
        max_total_bytes_read: 4 * 1024 * 1024,
        max_payload_bytes: 2 * 1024 * 1024,
        max_logical_decoded_bytes: 2 * 1024 * 1024,
        max_metadata_bytes: 1024 * 1024,
        max_allocation_bytes: 64 * 1024,
        max_stream_chunk_bytes: 4096,
        ..Limits::default()
    };
    let mut reader = SequentialReader::new(Cursor::new(data), limits);
    while let Ok(Some(_)) = reader.next_event() {}
});
