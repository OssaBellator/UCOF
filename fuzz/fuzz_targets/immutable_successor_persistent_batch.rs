#![no_main]

use std::collections::BTreeSet;

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_persistent_batch, build_genesis, validate, ImmutableBatchOperation, ImmutableLimits,
    ImmutableObjectInput, PersistentBatchMode,
};

fuzz_target!(|data: &[u8]| {
    let limits = ImmutableLimits {
        max_file_bytes: 2 << 20,
        max_objects: 32,
        max_pages: 64,
        max_depth: 4,
        max_allocation_bytes: 2 << 20,
        max_output_bytes: 2 << 20,
        ..ImmutableLimits::default()
    };
    let desired = data
        .first()
        .map_or(2_usize, |byte| 2 + usize::from(*byte % 15));
    let mut objects = Vec::with_capacity(desired);
    for index in 0..desired {
        let seed = data.get(index + 1).copied().unwrap_or(index as u8);
        objects.push(ImmutableObjectInput::new(
            u64::try_from(index + 1).expect("small object identifier"),
            u16::from(1 + seed % 31),
            vec![seed, seed.rotate_left(1), seed.rotate_left(2)],
        ));
    }
    let genesis = build_genesis(&objects, limits).expect("bounded genesis");

    let shape_change = data.get(1).is_some_and(|byte| byte & 1 != 0);
    let mut selected = BTreeSet::new();
    for byte in data.iter().skip(2).take(4) {
        selected.insert(usize::from(*byte) % desired);
    }
    if selected.is_empty() {
        selected.insert(0);
    }
    let mut operations = Vec::new();
    for index in selected {
        let seed = data.get(index + 2).copied().unwrap_or(index as u8);
        operations.push(ImmutableBatchOperation::Put(ImmutableObjectInput::new(
            u64::try_from(index + 1).expect("small object identifier"),
            u16::from(1 + seed % 31),
            vec![seed, b':', b'v', b'2'],
        )));
    }
    if shape_change {
        operations.push(ImmutableBatchOperation::Put(ImmutableObjectInput::new(
            u64::try_from(desired + 1).expect("small inserted identifier"),
            1,
            b"inserted".to_vec(),
        )));
    }

    let result =
        append_persistent_batch(&genesis, &operations, limits).expect("bounded persistent batch");
    let report = validate(&result.bytes, limits).expect("persistent batch validates");
    assert_eq!(report, result.report);
    assert_eq!(
        result.mode,
        if shape_change {
            PersistentBatchMode::CopyOnWritePutBatch
        } else {
            PersistentBatchMode::CopyOnWriteReplacements
        }
    );

    operations.reverse();
    assert_eq!(
        append_persistent_batch(&genesis, &operations, limits)
            .expect("reordered persistent batch")
            .bytes,
        result.bytes
    );
});
