#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::{scan_backwards, RecoveryScanLimits};

fuzz_target!(|data: &[u8]| {
    let _ = scan_backwards(
        data,
        b"ROOTCAND",
        32,
        RecoveryScanLimits {
            max_scan_bytes: data.len().min(64 * 1024),
            max_magic_candidates: 64,
            max_validations: 32,
            max_results: 16,
        },
        |offset, candidate| {
            let marker = candidate.get(8).copied().unwrap_or_default();
            (marker & 1 == 0).then_some((offset, marker))
        },
    );
});
