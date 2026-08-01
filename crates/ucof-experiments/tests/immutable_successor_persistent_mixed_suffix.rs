use std::io::{self, Write};

use ucof_experiments::immutable_successor::{
    append_persistent_mixed_batch, append_persistent_mixed_suffix, build_genesis,
    validate_canonical_occupancy, write_persistent_mixed_batch_to, ImmutableBatchOperation,
    ImmutableLimits, ImmutableObjectInput, PersistentMixedWriteError, PersistentMixedWriteOptions,
};

fn object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::try_from(1 + object_id % 19).expect("kind"),
        format!("payload:{object_id}").into_bytes(),
    )
}

fn even_objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(|index| object(index * 2))
        .collect()
}

fn assert_suffix_matches(
    genesis_count: usize,
    operations: &[ImmutableBatchOperation],
    expected_root_level: u8,
) {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(genesis_count), limits).expect("genesis");
    let full = append_persistent_mixed_batch(&genesis, operations, limits).expect("full writer");
    let suffix =
        append_persistent_mixed_suffix(&genesis, operations, limits).expect("suffix writer");
    assert_eq!(suffix.prefix_len, genesis.len());
    assert_eq!(suffix.suffix.len(), full.bytes.len() - genesis.len());
    let mut combined = genesis.clone();
    combined.extend_from_slice(&suffix.suffix);
    assert_eq!(combined, full.bytes);
    assert_eq!(suffix.report, full.report);
    assert_eq!(suffix.pages_written, full.pages_written);
    assert_eq!(suffix.pages_reused, full.pages_reused);
    assert_eq!(suffix.report.root_level, expected_root_level);
    assert_eq!(
        validate_canonical_occupancy(&combined, limits).expect("canonical combined output"),
        suffix.report
    );
}

#[test]
fn suffix_matches_stable_height_root_collapse_and_root_growth() {
    assert_suffix_matches(
        400,
        &[
            ImmutableBatchOperation::Delete(700),
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                701,
                78,
                b"inserted-701".to_vec(),
            )),
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                702,
                77,
                b"replacement-702".to_vec(),
            )),
        ],
        1,
    );
    assert_suffix_matches(
        186,
        &[
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                4,
                91,
                b"replacement-four".to_vec(),
            )),
        ],
        0,
    );
    assert_suffix_matches(
        185,
        &[
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(object(1)),
            ImmutableBatchOperation::Put(object(371)),
        ],
        1,
    );
}

#[derive(Default)]
struct RecordingWriter {
    bytes: Vec<u8>,
    largest_request: usize,
}

impl Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.largest_request = self.largest_request.max(buffer.len());
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn bounded_sink_writer_matches_full_output() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let operations = [
        ImmutableBatchOperation::Delete(700),
        ImmutableBatchOperation::Put(object(701)),
        ImmutableBatchOperation::Put(ImmutableObjectInput::new(
            702,
            77,
            b"replacement-702".to_vec(),
        )),
    ];
    let expected =
        append_persistent_mixed_batch(&genesis, &operations, limits).expect("full writer");
    let mut writer = RecordingWriter::default();
    let report = write_persistent_mixed_batch_to(
        &mut writer,
        &genesis,
        &operations,
        limits,
        PersistentMixedWriteOptions {
            max_write_request_bytes: 113,
        },
    )
    .expect("bounded writer");
    assert_eq!(writer.bytes, expected.bytes);
    assert_eq!(report.report, expected.report);
    assert_eq!(report.prefix_bytes_written, genesis.len());
    assert_eq!(report.suffix_bytes_written, expected.bytes.len() - genesis.len());
    assert!(report.largest_write_request <= 113);
    assert!(writer.largest_request <= 113);
}

struct FailingWriter {
    remaining: usize,
}

impl Write for FailingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other("injected sink failure"));
        }
        let written = buffer.len().min(self.remaining);
        self.remaining -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn invalid_configuration_and_sink_failure_return_no_report() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(8), limits).expect("genesis");
    let operations = [
        ImmutableBatchOperation::Delete(2),
        ImmutableBatchOperation::Put(object(1)),
    ];
    let mut output = Vec::new();
    assert!(matches!(
        write_persistent_mixed_batch_to(
            &mut output,
            &genesis,
            &operations,
            limits,
            PersistentMixedWriteOptions {
                max_write_request_bytes: 0,
            },
        ),
        Err(PersistentMixedWriteError::Format(_))
    ));
    assert!(output.is_empty());

    let mut failing = FailingWriter { remaining: 17 };
    assert!(matches!(
        write_persistent_mixed_batch_to(
            &mut failing,
            &genesis,
            &operations,
            limits,
            PersistentMixedWriteOptions {
                max_write_request_bytes: 7,
            },
        ),
        Err(PersistentMixedWriteError::Output(_))
    ));
}
