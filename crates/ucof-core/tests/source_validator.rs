use std::io;
use ucof_core::{
    Error, IntegrityStatus, Limits, Manifest, ReadAt, SourceValidator, Writer,
};

#[derive(Debug)]
struct TrackingSource {
    bytes: Vec<u8>,
    max_request: usize,
}

impl TrackingSource {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            max_request: 0,
        }
    }
}

impl ReadAt for TrackingSource {
    fn len(&mut self) -> io::Result<u64> {
        u64::try_from(self.bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "test source length"))
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        self.max_request = self.max_request.max(buffer.len());
        let start = usize::try_from(offset)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "test offset"))?;
        let end = start
            .checked_add(buffer.len())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "test range"))?;
        let source = self
            .bytes
            .get(start..end)
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        buffer.copy_from_slice(source);
        Ok(())
    }
}

#[test]
fn source_validator_hashes_large_payload_in_bounded_blocks() {
    let bytes = file_with_payload(&vec![0x5a; 1024 * 1024], Vec::new());
    let file_len = u64::try_from(bytes.len()).expect("file length");
    let limits = Limits {
        max_stream_chunk_bytes: 4096,
        max_total_bytes_read: 4 * 1024 * 1024,
        ..Limits::default()
    };
    let mut source = TrackingSource::new(bytes);
    let report = SourceValidator::new(limits)
        .validate(&mut source)
        .expect("strict source validation");

    assert_eq!(report.integrity, IntegrityStatus::Verified);
    assert_eq!(report.stats.bytes_hashed + 80, file_len);
    assert!(report.stats.bytes_read > file_len);
    assert!(report.stats.largest_allocation <= 4096);
    assert!(source.max_request <= 4096);
    assert!(report.is_fully_interpretable());
    assert_eq!(report.manifest.roots, vec![1]);
}

#[test]
fn source_validator_detects_payload_tampering() {
    let mut bytes = file_with_payload(b"payload", Vec::new());
    bytes[32 + 40] ^= 1;
    let mut source = TrackingSource::new(bytes);

    let error = SourceValidator::default()
        .validate(&mut source)
        .expect_err("tampering must fail");
    assert_eq!(error, Error::DigestMismatch);
}

#[test]
fn source_validator_reports_required_capabilities() {
    let bytes = file_with_payload(b"payload", vec![42]);
    let mut source = TrackingSource::new(bytes);
    let report = SourceValidator::default()
        .validate(&mut source)
        .expect("structurally valid source");

    assert_eq!(report.unsupported_required_capabilities, vec![42]);
    assert!(!report.is_fully_interpretable());
    assert_eq!(report.integrity, IntegrityStatus::Verified);
}

#[test]
fn combined_read_budget_includes_inspection_and_hashing() {
    let bytes = file_with_payload(&vec![0x33; 4096], Vec::new());
    let file_len = u64::try_from(bytes.len()).expect("file length");
    let limits = Limits {
        max_total_bytes_read: file_len,
        ..Limits::default()
    };
    let mut source = TrackingSource::new(bytes);

    let error = SourceValidator::new(limits)
        .validate(&mut source)
        .expect_err("two-pass work must share one budget");
    assert_eq!(error, Error::LimitExceeded("total bytes read"));
}

#[test]
fn zero_hash_chunk_limit_fails_before_hash_allocation() {
    let bytes = file_with_payload(b"payload", Vec::new());
    let limits = Limits {
        max_stream_chunk_bytes: 0,
        ..Limits::default()
    };
    let mut source = TrackingSource::new(bytes);

    let error = SourceValidator::new(limits)
        .validate(&mut source)
        .expect_err("zero chunk limit must fail");
    assert_eq!(error, Error::LimitExceeded("hash chunk bytes"));
}

fn file_with_payload(payload: &[u8], required: Vec<u64>) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.add_opaque(1, payload).expect("opaque object");
    let mut manifest = Manifest::new(vec![1]);
    manifest.required_capabilities = required;
    writer.add_manifest(2, &manifest).expect("manifest");
    writer.finish(2).expect("file")
}
