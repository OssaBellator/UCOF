use std::io::{Cursor, Seek};
use ucof_core::{
    Error, IntegrityStatus, Limits, Manifest, SeekableWriter, SliceSource, SourceValidator,
    StreamingWriter, Writer,
};

#[test]
fn streaming_writer_matches_in_memory_writer_byte_for_byte() {
    let expected = in_memory_file();

    let mut writer = StreamingWriter::with_default_limits(Vec::new()).expect("streaming writer");
    writer.add_opaque(1, b"alpha").expect("object one");
    writer.add_opaque(2, b"").expect("object two");
    writer
        .add_manifest(3, &Manifest::new(vec![1, 2]))
        .expect("manifest");
    let finished = writer.finish(3).expect("finish");

    assert_eq!(finished.inner, expected);
    assert_eq!(
        finished.bytes_written,
        u64::try_from(expected.len()).expect("expected length")
    );
}

#[test]
fn reader_backed_payload_respects_small_chunks_and_validates() {
    let payload = vec![0x7a; 1025];
    let limits = Limits {
        max_stream_chunk_bytes: 7,
        ..Limits::default()
    };
    let mut writer = StreamingWriter::new(Vec::new(), limits).expect("streaming writer");
    let mut source = Cursor::new(payload.clone());
    writer
        .add_opaque_from_reader(
            1,
            u64::try_from(payload.len()).expect("payload length"),
            &mut source,
        )
        .expect("stream payload");
    writer
        .add_manifest(2, &Manifest::new(vec![1]))
        .expect("manifest");
    let bytes = writer.finish(2).expect("finish").inner;

    let mut source = SliceSource::new(&bytes);
    let report = SourceValidator::default()
        .validate(&mut source)
        .expect("valid output");
    assert_eq!(report.integrity, IntegrityStatus::Verified);
}

#[test]
fn seekable_writer_rewinds_finalized_output() {
    let cursor = Cursor::new(Vec::new());
    let mut writer = SeekableWriter::with_default_limits(cursor).expect("seekable writer");
    writer.add_opaque(1, b"payload").expect("object");
    writer
        .add_manifest(2, &Manifest::new(vec![1]))
        .expect("manifest");
    let mut finished = writer.finish_and_rewind(2).expect("finish and rewind");

    assert_eq!(finished.inner.stream_position().expect("position"), 0);
    assert_eq!(
        finished.bytes_written,
        u64::try_from(finished.inner.get_ref().len()).expect("file length")
    );
}

#[test]
fn truncated_payload_source_prevents_footer_publication_and_is_terminal() {
    let mut writer = StreamingWriter::with_default_limits(Vec::new()).expect("streaming writer");
    let mut source = Cursor::new(vec![1_u8; 4]);
    let error = writer
        .add_opaque_from_reader(1, 8, &mut source)
        .expect_err("short source");
    assert_eq!(error, Error::Truncated("writer payload source"));

    let error = writer
        .add_opaque(2, b"later")
        .expect_err("failed writer is terminal");
    assert_eq!(
        error,
        Error::InvalidRecordOrder("writer used after failure")
    );
}

#[test]
fn duplicate_identifier_is_rejected_before_second_record() {
    let mut writer = StreamingWriter::with_default_limits(Vec::new()).expect("streaming writer");
    writer.add_opaque(1, b"one").expect("first object");
    let error = writer
        .add_opaque(1, b"two")
        .expect_err("duplicate identifier");

    assert_eq!(error, Error::DuplicateObjectId(1));
}

#[test]
fn missing_manifest_prevents_finalization() {
    let mut writer = StreamingWriter::with_default_limits(Vec::new()).expect("streaming writer");
    writer.add_opaque(1, b"payload").expect("object");
    let error = writer.finish(9).expect_err("missing manifest");

    assert_eq!(error, Error::MissingManifest(9));
}

fn in_memory_file() -> Vec<u8> {
    let mut writer = Writer::new();
    writer.add_opaque(1, b"alpha").expect("object one");
    writer.add_opaque(2, b"").expect("object two");
    writer
        .add_manifest(3, &Manifest::new(vec![1, 2]))
        .expect("manifest");
    writer.finish(3).expect("file")
}
