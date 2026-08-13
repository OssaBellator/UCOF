//! Fixed-size source metadata used only by bounded writer staging.

pub const BOUNDED_SOURCE_DESCRIPTOR_BYTES: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedSourceDescriptor {
    pub object_id: u64,
    pub source_index: u64,
    pub kind: u16,
    pub logical_len: u64,
    pub strong_version: [u8; 32],
}

impl BoundedSourceDescriptor {
    pub fn encode(&self) -> Result<[u8; BOUNDED_SOURCE_DESCRIPTOR_BYTES], &'static str> {
        if self.object_id == 0 || self.kind == 0 {
            return Err("source descriptor identity");
        }
        let mut bytes = [0u8; BOUNDED_SOURCE_DESCRIPTOR_BYTES];
        bytes[..8].copy_from_slice(&self.object_id.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.source_index.to_le_bytes());
        bytes[16..18].copy_from_slice(&self.kind.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.logical_len.to_le_bytes());
        bytes[32..64].copy_from_slice(&self.strong_version);
        Ok(bytes)
    }
}
