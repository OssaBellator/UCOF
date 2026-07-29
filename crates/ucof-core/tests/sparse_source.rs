use std::io;
use ucof_core::{encode_canonical, CborValue, Limits, MetadataInspector, ReadAt, RecordKind};

const HEADER_LEN: u64 = 32;
const RECORD_HEADER_LEN: u64 = 40;
const FOOTER_LEN: u64 = 80;

#[derive(Debug)]
struct VirtualSparseSource {
    len: u64,
    payload_start: u64,
    payload_end: u64,
    segments: Vec<(u64, Vec<u8>)>,
    reads: Vec<(u64, usize)>,
}

impl ReadAt for VirtualSparseSource {
    fn len(&mut self) -> io::Result<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        self.reads.push((offset, buffer.len()));
        let length = u64::try_from(buffer.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "length"))?;
        let end = offset
            .checked_add(length)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "range"))?;

        for (segment_offset, segment) in &self.segments {
            let segment_len = u64::try_from(segment.len())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "segment length"))?;
            let segment_end = segment_offset
                .checked_add(segment_len)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "segment range"))?;
            if offset >= *segment_offset && end <= segment_end {
                let start = usize::try_from(offset - segment_offset)
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "segment offset"))?;
                let finish = start
                    .checked_add(buffer.len())
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "segment range"))?;
                buffer.copy_from_slice(&segment[start..finish]);
                return Ok(());
            }
        }

        if offset >= self.payload_start && end <= self.payload_end {
            buffer.fill(0);
            return Ok(());
        }

        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "unmapped virtual range",
        ))
    }
}

#[test]
fn metadata_inventory_cost_does_not_scale_with_sparse_payload_length() {
    let payload_len = 8_u64 * 1024 * 1024 * 1024;
    let mut source = virtual_file(payload_len);
    let file_len = source.len;
    let limits = Limits {
        max_file_bytes: file_len,
        max_total_bytes_read: 1024 * 1024,
        max_logical_decoded_bytes: payload_len,
        max_payload_bytes: payload_len,
        max_allocation_bytes: 1024 * 1024,
        ..Limits::default()
    };

    let report = MetadataInspector::new(limits)
        .inspect(&mut source)
        .expect("sparse metadata inspection");

    assert_eq!(report.entries.len(), 2);
    assert_eq!(report.entries[0].stored_len, payload_len);
    assert!(report.stats.bytes_read < 64 * 1024);
    assert!(source.reads.iter().all(|(offset, length)| {
        let end = offset + u64::try_from(*length).expect("read length");
        end <= source.payload_start || *offset >= source.payload_end
    }));
}

fn virtual_file(payload_len: u64) -> VirtualSparseSource {
    let header = file_header();
    let opaque_offset = HEADER_LEN;
    let payload_start = opaque_offset + RECORD_HEADER_LEN;
    let payload_end = payload_start + payload_len;

    let manifest_payload = encode_canonical(&map(vec![
        ("roots", CborValue::Array(vec![CborValue::Unsigned(1)])),
        ("required", CborValue::Array(Vec::new())),
        ("optional", CborValue::Array(Vec::new())),
    ]))
    .expect("manifest encoding");
    let manifest_offset = payload_end;
    let manifest_payload_offset = manifest_offset + RECORD_HEADER_LEN;
    let manifest_len = u64::try_from(manifest_payload.len()).expect("manifest length");

    let directory_offset = manifest_payload_offset + manifest_len;
    let directory_payload = encode_canonical(&map(vec![(
        "entries",
        CborValue::Array(vec![
            directory_entry(1, RecordKind::Opaque, opaque_offset, payload_len),
            directory_entry(2, RecordKind::Manifest, manifest_offset, manifest_len),
        ]),
    )]))
    .expect("directory encoding");
    let directory_len = u64::try_from(directory_payload.len()).expect("directory length");
    let footer_offset = directory_offset + RECORD_HEADER_LEN + directory_len;
    let file_len = footer_offset + FOOTER_LEN;

    let segments = vec![
        (0, header),
        (
            opaque_offset,
            record_header(RecordKind::Opaque, 1, payload_len),
        ),
        (
            manifest_offset,
            record_header(RecordKind::Manifest, 2, manifest_len),
        ),
        (manifest_payload_offset, manifest_payload),
        (
            directory_offset,
            record_header(RecordKind::Directory, 0, directory_len),
        ),
        (directory_offset + RECORD_HEADER_LEN, directory_payload),
        (
            footer_offset,
            footer(directory_offset, RECORD_HEADER_LEN + directory_len, 2, 3),
        ),
    ];

    VirtualSparseSource {
        len: file_len,
        payload_start,
        payload_end,
        segments,
        reads: Vec::new(),
    }
}

fn file_header() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"UCOF\r\n\x1a\n");
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 32);
    bytes.extend_from_slice(&[0_u8; 12]);
    bytes
}

fn record_header(kind: RecordKind, object_id: u64, payload_len: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"UCRD");
    push_u16(&mut bytes, u16::from(kind));
    push_u16(&mut bytes, 0);
    push_u32(&mut bytes, 40);
    push_u64(&mut bytes, payload_len);
    push_u64(&mut bytes, payload_len);
    push_u64(&mut bytes, object_id);
    push_u32(&mut bytes, 0);
    bytes
}

fn footer(directory_offset: u64, directory_len: u64, manifest_id: u64, count: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"UCFTR001");
    push_u32(&mut bytes, 80);
    push_u32(&mut bytes, 0);
    push_u64(&mut bytes, directory_offset);
    push_u64(&mut bytes, directory_len);
    push_u64(&mut bytes, manifest_id);
    push_u64(&mut bytes, count);
    bytes.extend_from_slice(&[0_u8; 32]);
    bytes
}

fn directory_entry(id: u64, kind: RecordKind, offset: u64, length: u64) -> CborValue {
    map(vec![
        ("id", CborValue::Unsigned(id)),
        ("kind", CborValue::Unsigned(u64::from(u16::from(kind)))),
        ("offset", CborValue::Unsigned(offset)),
        ("stored_len", CborValue::Unsigned(length)),
        ("logical_len", CborValue::Unsigned(length)),
    ])
}

fn map(entries: Vec<(&str, CborValue)>) -> CborValue {
    CborValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (CborValue::Text(key.to_owned()), value))
            .collect(),
    )
}

fn push_u16(target: &mut Vec<u8>, value: u16) {
    target.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(target: &mut Vec<u8>, value: u32) {
    target.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(target: &mut Vec<u8>, value: u64) {
    target.extend_from_slice(&value.to_le_bytes());
}
