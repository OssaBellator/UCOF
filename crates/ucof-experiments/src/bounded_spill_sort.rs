//! Non-normative bounded external-sort foundation for Phase 3 writer experiments.
//!
//! This module deliberately sorts opaque fixed-size payloads by an external `u64` key. It does
//! not define UCOF bytes, publication policy, encryption, or restart semantics. Callers are
//! responsible for supplying a private spill directory. Spill files use exclusive creation and are
//! removed on success or best-effort on failure.

use sha2::{Digest, Sha256};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SPILL_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedSpillRecord {
    pub key: u64,
    pub payload: Vec<u8>,
}

impl BoundedSpillRecord {
    pub fn new(key: u64, payload: Vec<u8>) -> Self {
        Self { key, payload }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundedSpillSortLimits {
    /// Exact payload bytes in every logical record. The spill frame adds an internal eight-byte key.
    pub record_bytes: usize,
    /// Maximum records buffered before one sorted initial run is emitted.
    pub run_records: usize,
    /// Maximum total logical records accepted from the caller.
    pub max_records: u64,
    /// Maximum number of initial sorted runs.
    pub max_initial_runs: usize,
    /// Maximum input runs opened for any one merge group. Must be at least two.
    pub max_open_inputs: usize,
    /// Maximum complete intermediate merge passes.
    pub max_merge_passes: usize,
    /// Maximum encoded spill bytes simultaneously present in the directory.
    pub max_live_spill_bytes: u64,
    /// Maximum encoded spill bytes written across initial and intermediate runs.
    pub max_spill_bytes_written: u64,
    /// Maximum encoded bytes read by intermediate merge passes.
    pub max_merge_bytes_read: u64,
    /// Maximum encoded bytes written by intermediate merge passes.
    pub max_merge_bytes_written: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedSpillSortReport {
    pub input_records: u64,
    pub initial_runs: usize,
    pub merge_passes: usize,
    pub peak_open_files: usize,
    /// Maximum encoded bytes represented by the configured in-memory initial-run buffer.
    pub peak_buffer_encoded_bytes: u64,
    pub initial_spill_bytes: u64,
    pub total_spill_bytes_written: u64,
    pub peak_live_spill_bytes: u64,
    pub merge_bytes_read: u64,
    pub merge_bytes_written: u64,
    /// Encoded bytes read from the final run while streaming payloads to the caller.
    pub final_run_bytes_read: u64,
    pub output_records: u64,
    pub output_payload_bytes: u64,
    pub output_sha256: [u8; 32],
}

#[derive(Debug)]
pub enum BoundedSpillSortError {
    InvalidLimits(&'static str),
    RecordSize { expected: usize, actual: usize },
    DuplicateKey(u64),
    TruncatedRun,
    Limit(&'static str),
    Io(std::io::Error),
}

impl std::fmt::Display for BoundedSpillSortError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLimits(label) => write!(formatter, "invalid spill-sort limit: {label}"),
            Self::RecordSize { expected, actual } => {
                write!(
                    formatter,
                    "spill record has {actual} bytes; expected {expected}"
                )
            }
            Self::DuplicateKey(key) => write!(formatter, "duplicate spill key {key}"),
            Self::TruncatedRun => write!(formatter, "truncated spill run"),
            Self::Limit(label) => write!(formatter, "spill-sort limit exceeded: {label}"),
            Self::Io(error) => write!(formatter, "spill-sort I/O failed: {error}"),
        }
    }
}

impl std::error::Error for BoundedSpillSortError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for BoundedSpillSortError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
struct SpillRun {
    path: PathBuf,
    records: u64,
    bytes: u64,
}

struct SpillWorkspace {
    directory: PathBuf,
    prefix: String,
    next_file: u64,
    tracked: Vec<PathBuf>,
}

impl SpillWorkspace {
    fn new(directory: &Path) -> Result<Self, BoundedSpillSortError> {
        let metadata = fs::symlink_metadata(directory)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(BoundedSpillSortError::InvalidLimits("spill directory"));
        }
        let session = NEXT_SPILL_SESSION.fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            directory: directory.to_path_buf(),
            prefix: format!(".ucof-spill-{}-{session}", std::process::id()),
            next_file: 0,
            tracked: Vec::new(),
        })
    }

    fn create(&mut self, label: &str) -> Result<(PathBuf, File), BoundedSpillSortError> {
        let sequence = self.next_file;
        self.next_file = self
            .next_file
            .checked_add(1)
            .ok_or(BoundedSpillSortError::Limit("spill file count"))?;
        let path = self
            .directory
            .join(format!("{}-{sequence:08}-{label}.bin", self.prefix));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let file = options.open(&path)?;
        self.tracked.push(path.clone());
        Ok((path, file))
    }

    fn remove(&mut self, path: &Path) -> Result<(), BoundedSpillSortError> {
        fs::remove_file(path)?;
        if let Some(index) = self.tracked.iter().position(|candidate| candidate == path) {
            self.tracked.swap_remove(index);
        }
        Ok(())
    }
}

impl Drop for SpillWorkspace {
    fn drop(&mut self) {
        for path in self.tracked.drain(..) {
            let _ = fs::remove_file(path);
        }
    }
}

#[derive(Default)]
struct SpillAccounting {
    initial_spill_bytes: u64,
    total_spill_bytes_written: u64,
    live_spill_bytes: u64,
    peak_live_spill_bytes: u64,
    merge_bytes_read: u64,
    merge_bytes_written: u64,
    peak_open_files: usize,
}

fn checked_u64(value: usize, label: &'static str) -> Result<u64, BoundedSpillSortError> {
    u64::try_from(value).map_err(|_| BoundedSpillSortError::Limit(label))
}

fn checked_add(left: u64, right: u64, label: &'static str) -> Result<u64, BoundedSpillSortError> {
    left.checked_add(right)
        .ok_or(BoundedSpillSortError::Limit(label))
}

fn encoded_record_bytes(limits: BoundedSpillSortLimits) -> Result<u64, BoundedSpillSortError> {
    if limits.record_bytes == 0 {
        return Err(BoundedSpillSortError::InvalidLimits("record bytes"));
    }
    if limits.run_records == 0 {
        return Err(BoundedSpillSortError::InvalidLimits("run records"));
    }
    if limits.max_records == 0 {
        return Err(BoundedSpillSortError::InvalidLimits("record count"));
    }
    if limits.max_initial_runs == 0 {
        return Err(BoundedSpillSortError::InvalidLimits("initial runs"));
    }
    if limits.max_open_inputs < 2 {
        return Err(BoundedSpillSortError::InvalidLimits("open inputs"));
    }
    checked_add(
        8,
        checked_u64(limits.record_bytes, "record bytes")?,
        "record bytes",
    )
}

fn run_encoded_bytes(records: u64, frame_bytes: u64) -> Result<u64, BoundedSpillSortError> {
    records
        .checked_mul(frame_bytes)
        .ok_or(BoundedSpillSortError::Limit("spill bytes"))
}

fn reserve_spill_write(
    accounting: &mut SpillAccounting,
    limits: BoundedSpillSortLimits,
    bytes: u64,
    initial: bool,
) -> Result<(), BoundedSpillSortError> {
    let total = checked_add(
        accounting.total_spill_bytes_written,
        bytes,
        "spill bytes written",
    )?;
    if total > limits.max_spill_bytes_written {
        return Err(BoundedSpillSortError::Limit("spill bytes written"));
    }
    let live = checked_add(accounting.live_spill_bytes, bytes, "live spill bytes")?;
    if live > limits.max_live_spill_bytes {
        return Err(BoundedSpillSortError::Limit("live spill bytes"));
    }
    accounting.total_spill_bytes_written = total;
    accounting.live_spill_bytes = live;
    accounting.peak_live_spill_bytes = accounting.peak_live_spill_bytes.max(live);
    if initial {
        accounting.initial_spill_bytes =
            checked_add(accounting.initial_spill_bytes, bytes, "initial spill bytes")?;
    }
    Ok(())
}

fn write_frame<W: Write>(
    writer: &mut W,
    record: &BoundedSpillRecord,
) -> Result<(), BoundedSpillSortError> {
    writer.write_all(&record.key.to_le_bytes())?;
    writer.write_all(&record.payload)?;
    Ok(())
}

fn read_frame<R: Read>(
    reader: &mut R,
    record_bytes: usize,
) -> Result<Option<BoundedSpillRecord>, BoundedSpillSortError> {
    let mut key = [0u8; 8];
    match reader.read(&mut key[..1])? {
        0 => return Ok(None),
        1 => {}
        _ => unreachable!("one-byte read returned more than one byte"),
    }
    reader
        .read_exact(&mut key[1..])
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => BoundedSpillSortError::TruncatedRun,
            _ => BoundedSpillSortError::Io(error),
        })?;
    let mut payload = vec![0u8; record_bytes];
    reader
        .read_exact(&mut payload)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => BoundedSpillSortError::TruncatedRun,
            _ => BoundedSpillSortError::Io(error),
        })?;
    Ok(Some(BoundedSpillRecord {
        key: u64::from_le_bytes(key),
        payload,
    }))
}

fn write_initial_run(
    workspace: &mut SpillWorkspace,
    records: &mut Vec<BoundedSpillRecord>,
    frame_bytes: u64,
    limits: BoundedSpillSortLimits,
    accounting: &mut SpillAccounting,
) -> Result<SpillRun, BoundedSpillSortError> {
    records.sort_unstable_by_key(|record| record.key);
    for pair in records.windows(2) {
        if pair[0].key == pair[1].key {
            return Err(BoundedSpillSortError::DuplicateKey(pair[0].key));
        }
    }
    let count = checked_u64(records.len(), "run record count")?;
    let bytes = run_encoded_bytes(count, frame_bytes)?;
    reserve_spill_write(accounting, limits, bytes, true)?;
    let (path, file) = workspace.create("run")?;
    let mut writer = BufWriter::new(file);
    for record in records.iter() {
        write_frame(&mut writer, record)?;
    }
    writer.flush()?;
    accounting.peak_open_files = accounting.peak_open_files.max(1);
    Ok(SpillRun {
        path,
        records: count,
        bytes,
    })
}

fn reserve_merge_io(
    accounting: &mut SpillAccounting,
    limits: BoundedSpillSortLimits,
    bytes: u64,
) -> Result<(), BoundedSpillSortError> {
    let read = checked_add(accounting.merge_bytes_read, bytes, "merge bytes read")?;
    if read > limits.max_merge_bytes_read {
        return Err(BoundedSpillSortError::Limit("merge bytes read"));
    }
    let written = checked_add(accounting.merge_bytes_written, bytes, "merge bytes written")?;
    if written > limits.max_merge_bytes_written {
        return Err(BoundedSpillSortError::Limit("merge bytes written"));
    }
    accounting.merge_bytes_read = read;
    accounting.merge_bytes_written = written;
    reserve_spill_write(accounting, limits, bytes, false)
}

fn merge_group(
    workspace: &mut SpillWorkspace,
    group: &[SpillRun],
    frame_bytes: u64,
    limits: BoundedSpillSortLimits,
    accounting: &mut SpillAccounting,
) -> Result<SpillRun, BoundedSpillSortError> {
    let records = group.iter().try_fold(0u64, |total, run| {
        checked_add(total, run.records, "merge records")
    })?;
    let bytes = run_encoded_bytes(records, frame_bytes)?;
    let input_bytes = group.iter().try_fold(0u64, |total, run| {
        checked_add(total, run.bytes, "merge input bytes")
    })?;
    if input_bytes != bytes {
        return Err(BoundedSpillSortError::Limit("merge byte accounting"));
    }
    reserve_merge_io(accounting, limits, bytes)?;
    accounting.peak_open_files = accounting.peak_open_files.max(group.len() + 1);

    let mut readers = Vec::with_capacity(group.len());
    for run in group {
        readers.push(BufReader::new(File::open(&run.path)?));
    }
    let (path, file) = workspace.create("merge")?;
    let mut writer = BufWriter::new(file);
    let mut heap: BinaryHeap<Reverse<(u64, usize, Vec<u8>)>> = BinaryHeap::new();
    for (index, reader) in readers.iter_mut().enumerate() {
        if let Some(record) = read_frame(reader, limits.record_bytes)? {
            heap.push(Reverse((record.key, index, record.payload)));
        }
    }

    let mut previous = None;
    let mut emitted = 0u64;
    while let Some(Reverse((key, index, payload))) = heap.pop() {
        if previous == Some(key) {
            return Err(BoundedSpillSortError::DuplicateKey(key));
        }
        if previous.is_some_and(|value| key < value) {
            return Err(BoundedSpillSortError::Limit("merge ordering"));
        }
        let record = BoundedSpillRecord { key, payload };
        write_frame(&mut writer, &record)?;
        emitted = checked_add(emitted, 1, "merge records")?;
        previous = Some(key);
        if let Some(next) = read_frame(&mut readers[index], limits.record_bytes)? {
            heap.push(Reverse((next.key, index, next.payload)));
        }
    }
    writer.flush()?;
    if emitted != records {
        return Err(BoundedSpillSortError::TruncatedRun);
    }

    for run in group {
        workspace.remove(&run.path)?;
        accounting.live_spill_bytes = accounting
            .live_spill_bytes
            .checked_sub(run.bytes)
            .ok_or(BoundedSpillSortError::Limit("live spill bytes"))?;
    }
    Ok(SpillRun {
        path,
        records,
        bytes,
    })
}

fn stream_final_run<W: Write>(
    workspace: &mut SpillWorkspace,
    run: SpillRun,
    limits: BoundedSpillSortLimits,
    output: &mut W,
) -> Result<(u64, u64, [u8; 32]), BoundedSpillSortError> {
    let mut reader = BufReader::new(File::open(&run.path)?);
    let mut previous = None;
    let mut records = 0u64;
    let mut payload_bytes = 0u64;
    let mut hasher = Sha256::new();
    while let Some(record) = read_frame(&mut reader, limits.record_bytes)? {
        if previous == Some(record.key) {
            return Err(BoundedSpillSortError::DuplicateKey(record.key));
        }
        if previous.is_some_and(|value| record.key < value) {
            return Err(BoundedSpillSortError::Limit("final ordering"));
        }
        output.write_all(&record.payload)?;
        hasher.update(&record.payload);
        records = checked_add(records, 1, "output records")?;
        payload_bytes = checked_add(
            payload_bytes,
            checked_u64(record.payload.len(), "output bytes")?,
            "output bytes",
        )?;
        previous = Some(record.key);
    }
    if records != run.records {
        return Err(BoundedSpillSortError::TruncatedRun);
    }
    workspace.remove(&run.path)?;
    Ok((records, payload_bytes, hasher.finalize().into()))
}

/// Sorts fixed-size opaque payload records by a separate `u64` key using bounded initial runs and
/// bounded-fan-in staged merges, then writes only the payload bytes in strict key order.
///
/// Duplicate keys are rejected both inside initial runs and across merge passes. The result is
/// independent of run size and merge fan-in for a given unique record set. Spill filenames and
/// framing are private implementation details and have no UCOF compatibility meaning. The output
/// writer can receive a valid prefix before a later error; callers requiring atomic visibility must
/// use private staging around the output writer.
pub fn bounded_spill_sort_to<I, W>(
    directory: &Path,
    records: I,
    output: &mut W,
    limits: BoundedSpillSortLimits,
) -> Result<BoundedSpillSortReport, BoundedSpillSortError>
where
    I: IntoIterator<Item = BoundedSpillRecord>,
    W: Write,
{
    let frame_bytes = encoded_record_bytes(limits)?;
    let run_records_u64 = checked_u64(limits.run_records, "run records")?;
    let peak_buffer_encoded_bytes = run_encoded_bytes(run_records_u64, frame_bytes)?;
    let mut workspace = SpillWorkspace::new(directory)?;
    let mut accounting = SpillAccounting::default();
    let mut current = Vec::new();
    let mut buffer = Vec::with_capacity(limits.run_records);
    let mut input_records = 0u64;

    for record in records {
        if record.payload.len() != limits.record_bytes {
            return Err(BoundedSpillSortError::RecordSize {
                expected: limits.record_bytes,
                actual: record.payload.len(),
            });
        }
        input_records = checked_add(input_records, 1, "record count")?;
        if input_records > limits.max_records {
            return Err(BoundedSpillSortError::Limit("record count"));
        }
        buffer.push(record);
        if buffer.len() == limits.run_records {
            if current.len() == limits.max_initial_runs {
                return Err(BoundedSpillSortError::Limit("initial runs"));
            }
            current.push(write_initial_run(
                &mut workspace,
                &mut buffer,
                frame_bytes,
                limits,
                &mut accounting,
            )?);
            buffer.clear();
        }
    }
    if !buffer.is_empty() {
        if current.len() == limits.max_initial_runs {
            return Err(BoundedSpillSortError::Limit("initial runs"));
        }
        current.push(write_initial_run(
            &mut workspace,
            &mut buffer,
            frame_bytes,
            limits,
            &mut accounting,
        )?);
    }
    let initial_runs = current.len();

    if current.is_empty() {
        return Ok(BoundedSpillSortReport {
            input_records: 0,
            initial_runs: 0,
            merge_passes: 0,
            peak_open_files: 0,
            peak_buffer_encoded_bytes,
            initial_spill_bytes: 0,
            total_spill_bytes_written: 0,
            peak_live_spill_bytes: 0,
            merge_bytes_read: 0,
            merge_bytes_written: 0,
            final_run_bytes_read: 0,
            output_records: 0,
            output_payload_bytes: 0,
            output_sha256: Sha256::digest([]).into(),
        });
    }

    let mut merge_passes = 0usize;
    while current.len() > 1 {
        if merge_passes == limits.max_merge_passes {
            return Err(BoundedSpillSortError::Limit("merge passes"));
        }
        let mut next = Vec::new();
        while !current.is_empty() {
            let take = current.len().min(limits.max_open_inputs);
            let group: Vec<_> = current.drain(..take).collect();
            if group.len() == 1 {
                next.push(group.into_iter().next().expect("single run"));
            } else {
                next.push(merge_group(
                    &mut workspace,
                    &group,
                    frame_bytes,
                    limits,
                    &mut accounting,
                )?);
            }
        }
        current = next;
        merge_passes += 1;
    }

    let final_run = current.pop().expect("non-empty final run");
    let final_run_bytes_read = final_run.bytes;
    let (output_records, output_payload_bytes, output_sha256) =
        stream_final_run(&mut workspace, final_run, limits, output)?;
    accounting.live_spill_bytes = 0;
    Ok(BoundedSpillSortReport {
        input_records,
        initial_runs,
        merge_passes,
        peak_open_files: accounting.peak_open_files,
        peak_buffer_encoded_bytes,
        initial_spill_bytes: accounting.initial_spill_bytes,
        total_spill_bytes_written: accounting.total_spill_bytes_written,
        peak_live_spill_bytes: accounting.peak_live_spill_bytes,
        merge_bytes_read: accounting.merge_bytes_read,
        merge_bytes_written: accounting.merge_bytes_written,
        final_run_bytes_read,
        output_records,
        output_payload_bytes,
        output_sha256,
    })
}

#[cfg(test)]
mod bounded_spill_sort_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ucof-bounded-spill-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn payload(key: u64, bytes: usize) -> Vec<u8> {
        let mut payload = vec![0u8; bytes];
        payload[..8].copy_from_slice(&key.to_le_bytes());
        let digest = Sha256::digest(format!("record:{key}").as_bytes());
        let copy = (bytes - 8).min(digest.len());
        payload[8..8 + copy].copy_from_slice(&digest[..copy]);
        payload
    }

    fn permutation(count: u64) -> Vec<u64> {
        (0..count)
            .map(|index| ((65_537 * index + 17_171) % count) + 1)
            .collect()
    }

    fn limits(
        record_bytes: usize,
        run_records: usize,
        max_open_inputs: usize,
    ) -> BoundedSpillSortLimits {
        BoundedSpillSortLimits {
            record_bytes,
            run_records,
            max_records: 30_000,
            max_initial_runs: 10_000,
            max_open_inputs,
            max_merge_passes: 32,
            max_live_spill_bytes: 64 * 1024 * 1024,
            max_spill_bytes_written: 256 * 1024 * 1024,
            max_merge_bytes_read: 256 * 1024 * 1024,
            max_merge_bytes_written: 256 * 1024 * 1024,
        }
    }

    #[test]
    fn run_size_and_fan_in_do_not_change_output() {
        let count = 20_003u64;
        let record_bytes = 88usize;
        let keys = permutation(count);
        let direct: Vec<u8> = (1..=count)
            .flat_map(|key| payload(key, record_bytes))
            .collect();

        let first_directory = TestDirectory::new("first");
        let mut first = Vec::new();
        let first_report = bounded_spill_sort_to(
            &first_directory.0,
            keys.iter()
                .copied()
                .map(|key| BoundedSpillRecord::new(key, payload(key, record_bytes))),
            &mut first,
            limits(record_bytes, 127, 4),
        )
        .expect("first spill sort");

        let second_directory = TestDirectory::new("second");
        let mut second = Vec::new();
        let second_report = bounded_spill_sort_to(
            &second_directory.0,
            keys.into_iter()
                .map(|key| BoundedSpillRecord::new(key, payload(key, record_bytes))),
            &mut second,
            limits(record_bytes, 509, 16),
        )
        .expect("second spill sort");

        assert_eq!(first, direct);
        assert_eq!(second, direct);
        assert_eq!(first_report.output_sha256, second_report.output_sha256);
        assert_ne!(first_report.initial_runs, second_report.initial_runs);
        assert_ne!(first_report.merge_passes, second_report.merge_passes);
        assert!(first_report.peak_open_files <= 5);
        assert!(second_report.peak_open_files <= 17);
        assert!(fs::read_dir(&first_directory.0).unwrap().next().is_none());
        assert!(fs::read_dir(&second_directory.0).unwrap().next().is_none());
    }

    #[test]
    fn duplicate_across_runs_is_rejected_and_cleaned_up() {
        let directory = TestDirectory::new("duplicate");
        let record_bytes = 16;
        let records = [1, 3, 2, 4, 2]
            .into_iter()
            .map(|key| BoundedSpillRecord::new(key, payload(key, record_bytes)));
        let error = bounded_spill_sort_to(
            &directory.0,
            records,
            &mut Vec::new(),
            limits(record_bytes, 2, 2),
        )
        .expect_err("duplicate must fail");
        assert!(matches!(error, BoundedSpillSortError::DuplicateKey(2)));
        assert!(fs::read_dir(&directory.0).unwrap().next().is_none());
    }

    #[test]
    fn live_spill_budget_blocks_merge_amplification_and_cleans_up() {
        let directory = TestDirectory::new("live-budget");
        let record_bytes = 8;
        let frame_bytes = 16u64;
        let mut constrained = limits(record_bytes, 2, 2);
        constrained.max_live_spill_bytes = 6 * frame_bytes;
        let records = [4, 1, 3, 2]
            .into_iter()
            .map(|key| BoundedSpillRecord::new(key, payload(key, record_bytes)));
        let error = bounded_spill_sort_to(&directory.0, records, &mut Vec::new(), constrained)
            .expect_err("merge amplification must exceed live budget");
        assert!(matches!(
            error,
            BoundedSpillSortError::Limit("live spill bytes")
        ));
        assert!(fs::read_dir(&directory.0).unwrap().next().is_none());
    }

    #[test]
    fn empty_input_writes_nothing() {
        let directory = TestDirectory::new("empty");
        let mut output = Vec::new();
        let report = bounded_spill_sort_to(
            &directory.0,
            std::iter::empty(),
            &mut output,
            limits(16, 4, 2),
        )
        .expect("empty spill sort");
        let empty_digest: [u8; 32] = Sha256::digest([]).into();
        assert!(output.is_empty());
        assert_eq!(report.input_records, 0);
        assert_eq!(report.output_sha256, empty_digest);
    }
}
