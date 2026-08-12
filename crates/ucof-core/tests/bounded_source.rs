use std::io::{self, Cursor};
use ucof_core::{
    Error, IntegrityStatus, Limits, Manifest, MetadataInspector, ReadAt, SeekSource, SliceSource,
    Writer,
};

#[derive(Debug)]
struct TrackingSource<'a> {
    bytes: &'a [u8],
    reads: Vec<(u64, usize)>,
}

impl<'a> TrackingSource<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            reads: Vec::new(),
        }
    }
}

impl ReadAt for TrackingSource<'_> {
    fn len(&mut self) -> io::Result<u64> {
        Ok(u64::try_from(self.bytes.len()).expect("test length"))
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        self.reads.push((offset, buffer.len()));
        let start = usize::try_from(offset).expect("test offset");
        let end = start.checked_add(buffer.len()).expect("test range");
        let source = self
            .bytes
            .get(start..end)
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        buffer.copy_from_slice(source);
        Ok(())
    }
}

#[test]
fn metadata_inspection_skips_opaque_payload_body() {
    let payload = vec![0x5a; 1024 * 1024];
    let bytes = file_with_payload(&payload, Vec::new());
    let payload_start = 32_u64 + 40;
    let payload_end = payload_start + u64::try_from(payload.len()).expect("payload length");

    let mut source = TrackingSource::new(&bytes);
    let report = MetadataInspector::default()
        .inspect(&mut source)
        .expect("metadata inspection");

    assert_eq!(report.integrity, IntegrityStatus::NotChecked);
    assert_eq!(report.entries.len(), 2);
    assert!(report.stats.bytes_read < 4096);
    assert!(source.reads.iter().all(|(offset, length)| {
        let end = offset + u64::try_from(*length).expect("read length");
        end <= payload_start || *offset >= payload_end
    }));
}

#[test]
fn metadata_inspection_reports_required_capabilities_without_claiming_support() {
    let bytes = file_with_payload(b"payload", vec![42]);
    let mut source = SliceSource::new(&bytes);
    let report = MetadataInspector::default()
        .inspect(&mut source)
        .expect("structural inventory");

    assert_eq!(report.unsupported_required_capabilities, vec![42]);
    assert!(!report.is_fully_interpretable());
}

#[test]
fn read_budget_stops_inspection_before_unbounded_work() {
    let bytes = file_with_payload(b"payload", Vec::new());
    let mut source = SliceSource::new(&bytes);
    let limits = Limits {
        max_total_bytes_read: 31,
        ..Limits::default()
    };
    let error = MetadataInspector::new(limits)
        .inspect(&mut source)
        .expect_err("read budget must fail");

    assert_eq!(error, Error::LimitExceeded("total bytes read"));
}

#[test]
fn seekable_source_matches_slice_source() {
    let bytes = file_with_payload(b"payload", Vec::new());
    let mut slice = SliceSource::new(&bytes);
    let mut seekable = SeekSource::new(Cursor::new(bytes.clone()));

    let slice_report = MetadataInspector::default()
        .inspect(&mut slice)
        .expect("slice inspection");
    let seek_report = MetadataInspector::default()
        .inspect(&mut seekable)
        .expect("seek inspection");

    assert_eq!(slice_report, seek_report);
}

#[test]
fn metadata_inspection_does_not_hide_record_header_corruption() {
    let mut bytes = file_with_payload(b"payload", Vec::new());
    bytes[32 + 28] = 9;
    let mut source = SliceSource::new(&bytes);
    let error = MetadataInspector::default()
        .inspect(&mut source)
        .expect_err("record identity mismatch must fail");

    assert!(matches!(error, Error::DirectoryMismatch(_)));
}

fn file_with_payload(payload: &[u8], required: Vec<u64>) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.add_opaque(1, payload).expect("opaque object");
    let mut manifest = Manifest::new(vec![1]);
    manifest.required_capabilities = required;
    writer.add_manifest(2, &manifest).expect("manifest");
    writer.finish(2).expect("file")
}
