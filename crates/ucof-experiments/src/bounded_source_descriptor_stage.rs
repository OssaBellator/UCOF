use crate::bounded_source_descriptor::{BoundedSourceDescriptor, BOUNDED_SOURCE_DESCRIPTOR_BYTES};
use crate::bounded_source_descriptor_parse::parse_bounded_source_descriptor;
use crate::bounded_spill_fallible::{bounded_spill_sort_fallible_to, BoundedSpillInputError};
use crate::bounded_spill_sort::{
    BoundedSpillRecord, BoundedSpillSortError, BoundedSpillSortLimits, BoundedSpillSortReport,
};
use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DESCRIPTOR_STAGE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub enum BoundedSourceStageError<InputError> {
    Input(InputError),
    Sort(BoundedSpillSortError),
    Invalid(&'static str),
    Io(std::io::ErrorKind),
}

impl<InputError: std::fmt::Display> std::fmt::Display for BoundedSourceStageError<InputError> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(error) => write!(formatter, "source descriptor input failed: {error}"),
            Self::Sort(error) => write!(formatter, "source descriptor sort failed: {error}"),
            Self::Invalid(label) => write!(formatter, "invalid source descriptor stage: {label}"),
            Self::Io(kind) => write!(formatter, "source descriptor stage I/O failed: {kind:?}"),
        }
    }
}

#[derive(Debug)]
pub enum BoundedSourceStageVisitError<VisitError> {
    Invalid(&'static str),
    Io(std::io::ErrorKind),
    Visit(VisitError),
}

impl<VisitError: std::fmt::Display> std::fmt::Display for BoundedSourceStageVisitError<VisitError> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(label) => write!(formatter, "invalid prepared source descriptor: {label}"),
            Self::Io(kind) => write!(formatter, "prepared source descriptor I/O failed: {kind:?}"),
            Self::Visit(error) => write!(formatter, "source descriptor visitor failed: {error}"),
        }
    }
}

#[derive(Debug)]
pub struct PreparedBoundedSourceDescriptors {
    path: PathBuf,
    file: Option<File>,
    records: u64,
    bytes: u64,
    report: BoundedSpillSortReport,
}

impl PreparedBoundedSourceDescriptors {
    pub fn records(&self) -> u64 {
        self.records
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn report(&self) -> &BoundedSpillSortReport {
        &self.report
    }

    pub fn visit<VisitError, F>(
        &self,
        mut visit: F,
    ) -> Result<(), BoundedSourceStageVisitError<VisitError>>
    where
        F: FnMut(BoundedSourceDescriptor) -> Result<(), VisitError>,
    {
        let mut file = self
            .file
            .as_ref()
            .ok_or(BoundedSourceStageVisitError::Invalid("closed stage"))?
            .try_clone()
            .map_err(|error| BoundedSourceStageVisitError::Io(error.kind()))?;
        file.seek(SeekFrom::Start(0))
            .map_err(|error| BoundedSourceStageVisitError::Io(error.kind()))?;
        let mut reader = BufReader::new(file);
        let mut bytes = [0u8; BOUNDED_SOURCE_DESCRIPTOR_BYTES];
        for _ in 0..self.records {
            reader
                .read_exact(&mut bytes)
                .map_err(|error| BoundedSourceStageVisitError::Io(error.kind()))?;
            let descriptor = parse_bounded_source_descriptor(&bytes)
                .map_err(BoundedSourceStageVisitError::Invalid)?;
            visit(descriptor).map_err(BoundedSourceStageVisitError::Visit)?;
        }
        let mut trailing = [0u8; 1];
        match reader.read(&mut trailing) {
            Ok(0) => Ok(()),
            Ok(_) => Err(BoundedSourceStageVisitError::Invalid(
                "source descriptor trailing bytes",
            )),
            Err(error) => Err(BoundedSourceStageVisitError::Io(error.kind())),
        }
    }
}

impl Drop for PreparedBoundedSourceDescriptors {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

fn create_stage(directory: &Path) -> Result<(PathBuf, File), std::io::Error> {
    let session = NEXT_DESCRIPTOR_STAGE.fetch_add(1, Ordering::Relaxed);
    let path = directory.join(format!(
        ".ucof-source-descriptors-{}-{session}.bin",
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(&path)?;
    Ok((path, file))
}

fn retire_stage(path: &Path) {
    let _ = fs::remove_file(path);
}

pub fn prepare_bounded_source_descriptors<I, InputError>(
    directory: &Path,
    descriptors: I,
    limits: BoundedSpillSortLimits,
) -> Result<PreparedBoundedSourceDescriptors, BoundedSourceStageError<InputError>>
where
    I: IntoIterator<Item = Result<BoundedSourceDescriptor, InputError>>,
{
    if limits.record_bytes != BOUNDED_SOURCE_DESCRIPTOR_BYTES {
        return Err(BoundedSourceStageError::Invalid(
            "spill record byte configuration",
        ));
    }
    let (path, file) = create_stage(directory)
        .map_err(|error| BoundedSourceStageError::Io(error.kind()))?;
    let invalid = Cell::new(None);
    let records = descriptors.into_iter().map(|result| {
        result.map(|descriptor| match descriptor.encode() {
            Ok(bytes) => BoundedSpillRecord::new(descriptor.object_id, bytes.to_vec()),
            Err(label) => {
                invalid.set(Some(label));
                BoundedSpillRecord::new(0, Vec::new())
            }
        })
    });
    let mut writer = BufWriter::new(file);
    let sorted = bounded_spill_sort_fallible_to(directory, records, &mut writer, limits);
    if let Some(label) = invalid.get() {
        drop(writer);
        retire_stage(&path);
        return Err(BoundedSourceStageError::Invalid(label));
    }
    let report = match sorted {
        Ok(report) => report,
        Err(BoundedSpillInputError::Input(error)) => {
            drop(writer);
            retire_stage(&path);
            return Err(BoundedSourceStageError::Input(error));
        }
        Err(BoundedSpillInputError::Sort(error)) => {
            drop(writer);
            retire_stage(&path);
            return Err(BoundedSourceStageError::Sort(error));
        }
    };
    if let Err(error) = writer.flush() {
        let kind = error.kind();
        drop(writer);
        retire_stage(&path);
        return Err(BoundedSourceStageError::Io(kind));
    }
    let retained = match writer.get_ref().try_clone() {
        Ok(file) => file,
        Err(error) => {
            let kind = error.kind();
            drop(writer);
            retire_stage(&path);
            return Err(BoundedSourceStageError::Io(kind));
        }
    };
    drop(writer);

    let descriptor_bytes =
        u64::try_from(BOUNDED_SOURCE_DESCRIPTOR_BYTES).expect("descriptor size fits u64");
    let expected = match report.output_records.checked_mul(descriptor_bytes) {
        Some(expected) => expected,
        None => {
            drop(retained);
            retire_stage(&path);
            return Err(BoundedSourceStageError::Invalid(
                "source descriptor stage bytes",
            ));
        }
    };
    let on_disk = match retained.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            let kind = error.kind();
            drop(retained);
            retire_stage(&path);
            return Err(BoundedSourceStageError::Io(kind));
        }
    };
    if report.output_payload_bytes != expected || on_disk != expected {
        drop(retained);
        retire_stage(&path);
        return Err(BoundedSourceStageError::Invalid(
            "source descriptor stage bytes",
        ));
    }
    Ok(PreparedBoundedSourceDescriptors {
        path,
        file: Some(retained),
        records: report.output_records,
        bytes: expected,
        report,
    })
}
