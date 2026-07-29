#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::exp0002::{
    build_append, build_genesis, validate_strict, FileHeader, ObjectInput, ValidationLimits,
};

fuzz_target!(|data: &[u8]| {
    let mut file_id = [0_u8; 16];
    let mut nonce = [0_u8; 16];
    for (index, byte) in data.iter().take(16).enumerate() {
        file_id[index] = *byte;
    }
    for (index, byte) in data.iter().skip(16).take(16).enumerate() {
        nonce[index] = *byte;
    }

    let object_count = usize::from(data.get(32).copied().unwrap_or(0) % 16) + 1;
    let mut cursor = 33_usize;
    let mut objects = Vec::with_capacity(object_count);
    for index in 0..object_count {
        let requested = usize::from(data.get(cursor).copied().unwrap_or(0) % 64);
        cursor = cursor.saturating_add(1);
        let available = data.len().saturating_sub(cursor);
        let payload_len = requested.min(available);
        let payload = data[cursor..cursor + payload_len].to_vec();
        cursor = cursor.saturating_add(payload_len);
        objects.push(ObjectInput {
            object_id: u64::try_from(index + 1).expect("bounded object index"),
            kind: u16::from(data.get(cursor).copied().unwrap_or(0) % 31) + 1,
            payload,
            is_root: index == 0,
        });
        cursor = cursor.saturating_add(1);
    }

    let limits = ValidationLimits {
        max_file_bytes: 8 * 1024 * 1024,
        max_commit_bytes: 8 * 1024 * 1024,
        max_snapshot_bytes: 1024 * 1024,
        max_pages: 1024,
        max_page_depth: 16,
        max_objects: 1024,
        max_payload_bytes: 8 * 1024 * 1024,
        max_hashed_bytes: 32 * 1024 * 1024,
        max_roots: 1024,
        max_capabilities: 1024,
    };
    let genesis = build_genesis(
        FileHeader {
            file_id,
            creation_nonce: nonce,
        },
        objects,
    )
    .expect("writer-generated genesis must succeed");
    validate_strict(&genesis, &limits).expect("writer-generated genesis must validate");

    if cursor < data.len() {
        let next_id = u64::try_from(object_count + 1).expect("bounded object index");
        let append = build_append(
            &genesis,
            vec![ObjectInput {
                object_id: next_id,
                kind: 1,
                payload: data[cursor..].iter().copied().take(256).collect(),
                is_root: false,
            }],
            vec![1, next_id],
            &limits,
        )
        .expect("writer-generated append must succeed");
        validate_strict(&append, &limits).expect("writer-generated append must validate");
    }
});
