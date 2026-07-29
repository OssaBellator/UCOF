/// Caller-controlled limits for hostile input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_file_bytes: u64,
    pub max_records: u64,
    pub max_payload_bytes: u64,
    pub max_metadata_bytes: u64,
    pub max_metadata_depth: usize,
    pub max_container_items: u64,
    pub max_text_bytes: u64,
    pub max_byte_string_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024,
            max_records: 100_000,
            max_payload_bytes: 32 * 1024 * 1024,
            max_metadata_bytes: 8 * 1024 * 1024,
            max_metadata_depth: 64,
            max_container_items: 100_000,
            max_text_bytes: 1024 * 1024,
            max_byte_string_bytes: 8 * 1024 * 1024,
        }
    }
}
