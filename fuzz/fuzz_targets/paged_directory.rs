#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::{ObjectLocator, PagedDirectory};

fuzz_target!(|data: &[u8]| {
    let leaf_capacity = usize::from(data.first().copied().unwrap_or(0) % 31) + 1;
    let fanout = usize::from(data.get(1).copied().unwrap_or(0) % 31) + 2;
    let mut entries = Vec::new();
    for (index, chunk) in data.get(2..).unwrap_or_default().chunks(16).take(128).enumerate() {
        let mut id_bytes = [0_u8; 8];
        id_bytes[..chunk.len().min(8)].copy_from_slice(&chunk[..chunk.len().min(8)]);
        let mut length_bytes = [0_u8; 8];
        if chunk.len() > 8 {
            length_bytes[..(chunk.len() - 8).min(8)]
                .copy_from_slice(&chunk[8..chunk.len().min(16)]);
        }
        let object_id = u64::from_le_bytes(id_bytes);
        let stored_len = u64::from_le_bytes(length_bytes) % (1024 * 1024);
        entries.push(ObjectLocator {
            object_id,
            kind: 1,
            offset: u64::try_from(index).expect("bounded index") * 4096,
            stored_len,
            logical_len: stored_len,
        });
    }

    if let Ok(directory) = PagedDirectory::build(entries, leaf_capacity, fanout) {
        let _ = directory.validate(4096);
        let query = u64::from_le_bytes(data.get(..8).unwrap_or_default().try_into().unwrap_or([0; 8]));
        let _ = directory.lookup(query, 64);
    }
});
