#[path = "../src/bounded_source_descriptor.rs"]
mod bounded_source_descriptor;
#[path = "../src/bounded_spill_fallible.rs"]
mod bounded_spill_fallible;
#[path = "../src/bounded_spill_sort.rs"]
mod bounded_spill_sort;

#[test]
fn bounded_source_descriptor_has_fixed_private_layout() {
    let descriptor = bounded_source_descriptor::BoundedSourceDescriptor {
        object_id: 7,
        source_index: 11,
        kind: 3,
        logical_len: 19,
        strong_version: [23; 32],
    };
    let bytes = descriptor.encode().expect("encode descriptor");
    assert_eq!(
        bytes.len(),
        bounded_source_descriptor::BOUNDED_SOURCE_DESCRIPTOR_BYTES
    );
    assert_eq!(&bytes[..8], &7u64.to_le_bytes());
    assert_eq!(&bytes[8..16], &11u64.to_le_bytes());
    assert_eq!(&bytes[16..18], &3u16.to_le_bytes());
    assert_eq!(&bytes[18..24], &[0; 6]);
    assert_eq!(&bytes[24..32], &19u64.to_le_bytes());
    assert_eq!(&bytes[32..], &[23; 32]);
}
