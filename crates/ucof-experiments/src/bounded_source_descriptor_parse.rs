use crate::bounded_source_descriptor::{
    BoundedSourceDescriptor, BOUNDED_SOURCE_DESCRIPTOR_BYTES,
};

pub fn parse_bounded_source_descriptor(
    bytes: &[u8],
) -> Result<BoundedSourceDescriptor, &'static str> {
    if bytes.len() != BOUNDED_SOURCE_DESCRIPTOR_BYTES {
        return Err("source descriptor length");
    }
    if bytes[18..24].iter().any(|byte| *byte != 0) {
        return Err("source descriptor reserved bytes");
    }
    let object_id = u64::from_le_bytes(bytes[..8].try_into().expect("fixed field"));
    let source_index = u64::from_le_bytes(bytes[8..16].try_into().expect("fixed field"));
    let kind = u16::from_le_bytes(bytes[16..18].try_into().expect("fixed field"));
    let logical_len = u64::from_le_bytes(bytes[24..32].try_into().expect("fixed field"));
    let strong_version = bytes[32..64].try_into().expect("fixed version field");
    if object_id == 0 || kind == 0 {
        return Err("source descriptor identity");
    }
    Ok(BoundedSourceDescriptor {
        object_id,
        source_index,
        kind,
        logical_len,
        strong_version,
    })
}
