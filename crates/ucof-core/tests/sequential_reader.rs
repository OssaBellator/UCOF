use std::io::Cursor;
use ucof_core::{Error, IntegrityStatus, Limits, Manifest, SequentialReader, StreamEvent, Writer};

#[test]
fn sequential_reader_emits_bounded_chunks_and_verified_commit() {
    let bytes = demo_file(Vec::new());
    let limits = Limits {
        max_stream_chunk_bytes: 3,
        ..Limits::default()
    };
    let mut reader = SequentialReader::new(Cursor::new(bytes.clone()), limits);
    let mut object_one = Vec::new();
    let mut saw_zero_length_record = false;
    let mut commit = None;

    while let Some(event) = reader.next_event().expect("valid sequential event") {
        match event {
            StreamEvent::RecordStart(record) if record.object_id == 2 => {
                assert_eq!(record.stored_len, 0);
                saw_zero_length_record = true;
            }
            StreamEvent::PayloadChunk {
                object_id: 1,
                bytes,
                ..
            } => {
                assert!(bytes.len() <= 3);
                object_one.extend_from_slice(&bytes);
            }
            StreamEvent::PayloadChunk { bytes, .. } => {
                assert!(bytes.len() <= 3);
            }
            StreamEvent::Commit(value) => commit = Some(value),
            _ => {}
        }
    }

    assert_eq!(object_one, b"abcdefgh");
    assert!(saw_zero_length_record);
    let commit = commit.expect("commit event");
    assert_eq!(commit.manifest_id, 3);
    assert_eq!(commit.record_count, 4);
    assert_eq!(commit.roots, vec![1, 2]);
    assert!(commit.is_fully_interpretable());
    assert_eq!(commit.integrity, IntegrityStatus::Verified);
    assert_eq!(
        commit.stats.bytes_read,
        u64::try_from(bytes.len()).expect("file length")
    );
    assert_eq!(commit.stats.bytes_hashed + 80, commit.stats.bytes_read);
    assert!(commit.stats.payload_chunks > 2);
}

#[test]
fn required_capabilities_are_reported_without_false_interpretation() {
    let bytes = demo_file(vec![42]);
    let mut reader = SequentialReader::with_default_limits(Cursor::new(bytes));
    let commit = loop {
        match reader.next_event().expect("valid structural stream") {
            Some(StreamEvent::Commit(commit)) => break commit,
            Some(_) => {}
            None => panic!("missing commit"),
        }
    };

    assert_eq!(commit.unsupported_required_capabilities, vec![42]);
    assert!(!commit.is_fully_interpretable());
    assert_eq!(commit.integrity, IntegrityStatus::Verified);
}

#[test]
fn payload_tampering_fails_before_commit() {
    let mut bytes = demo_file(Vec::new());
    bytes[32 + 40] ^= 1;
    let mut reader = SequentialReader::with_default_limits(Cursor::new(bytes));

    let error = drain_error(&mut reader);
    assert_eq!(error, Error::DigestMismatch);
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut bytes = demo_file(Vec::new());
    bytes.extend_from_slice(b"tail");
    let mut reader = SequentialReader::with_default_limits(Cursor::new(bytes));

    let error = drain_error(&mut reader);
    assert_eq!(
        error,
        Error::InvalidRecordOrder("trailing bytes after footer")
    );
}

#[test]
fn truncated_payload_is_categorized() {
    let bytes = demo_file(Vec::new());
    let truncated = bytes[..75].to_vec();
    let mut reader = SequentialReader::with_default_limits(Cursor::new(truncated));

    let error = drain_error(&mut reader);
    assert_eq!(error, Error::Truncated("record payload"));
}

#[test]
fn logical_byte_budget_stops_before_excess_read() {
    let bytes = demo_file(Vec::new());
    let limits = Limits {
        max_logical_decoded_bytes: 7,
        ..Limits::default()
    };
    let mut reader = SequentialReader::new(Cursor::new(bytes), limits);

    let error = drain_error(&mut reader);
    assert_eq!(error, Error::LimitExceeded("logical decoded bytes"));
    let cursor = reader.into_inner();
    assert_eq!(cursor.position(), 32 + 40 + 7);
}

#[test]
fn zero_chunk_limit_fails_before_payload_allocation() {
    let bytes = demo_file(Vec::new());
    let limits = Limits {
        max_stream_chunk_bytes: 0,
        ..Limits::default()
    };
    let mut reader = SequentialReader::new(Cursor::new(bytes), limits);

    let error = drain_error(&mut reader);
    assert_eq!(error, Error::LimitExceeded("stream chunk bytes"));
}

fn drain_error(reader: &mut SequentialReader<Cursor<Vec<u8>>>) -> Error {
    loop {
        match reader.next_event() {
            Ok(Some(_)) => {}
            Ok(None) => panic!("stream unexpectedly completed"),
            Err(error) => return error,
        }
    }
}

fn demo_file(required: Vec<u64>) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.add_opaque(1, b"abcdefgh").expect("object one");
    writer.add_opaque(2, b"").expect("object two");
    let mut manifest = Manifest::new(vec![1, 2]);
    manifest.required_capabilities = required;
    writer.add_manifest(3, &manifest).expect("manifest");
    writer.finish(3).expect("file")
}
