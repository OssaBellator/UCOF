use crate::bounded_source_descriptor::{BoundedSourceDescriptor, BOUNDED_SOURCE_DESCRIPTOR_BYTES};
use crate::bounded_source_descriptor_parse::parse_bounded_source_descriptor;
use crate::bounded_spill_fallible::{bounded_spill_sort_fallible_to, BoundedSpillInputError};
use crate::bounded_spill_sort::{
    BoundedSpillRecord, BoundedSpillSortError, BoundedSpillSortLimits, BoundedSpillSortReport,
};
use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
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

#[derive(Debug)]
pub enum BoundedSourceStageVisitError<VisitError> {
    Invalid(&'static str),
    Io(std::io::ErrorKind),
    Visit(VisitError),
}

#[derive(Debug)]
pub struct PreparedBoundedSourceDescriptors {
    path: PathBuf,
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
        let file = File::open(&self.path)
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
        let _ = fs::remove_file(&path);
        return Err(BoundedSourceStageError::Invalid(label));
    }
    let report = match sorted {
        Ok(report) => report,
        Err(BoundedSpillInputError::Input(error)) => {
            let _ = fs::remove_file(&path);
            return Err(BoundedSourceStageError::Input(error));
        }
        Err(BoundedSpillInputError::Sort(error)) => {
            let _ = fs::remove_file(&path);
            return Err(BoundedSourceStageError::Sort(error));
        }
    };
    if let Err(error) = writer.flush() {
        let _ = fs::remove_file(&path);
        return Err(BoundedSourceStageError::Io(error.kind()));
    }
    drop(writer);
    let bytes = report.output_payload_bytes;
    let expected = report
        .output_records
        .checked_mul(BOUNDED_SOURCE_DESCRIPTOR_BYTES as u64)
        .ok_or(BoundedSourceStageError::Invalid("source descriptor stage bytes"))?;
    if bytes != expected {
        let _ = fs::remove_file(&path);
        return Err(BoundedSourceStageError::Invalid(
            "source descriptor stage bytes",
        ));
    }
    Ok(PreparedBoundedSourceDescriptors {
        path,
        records: report.output_records,
        bytes,
        report,
    })
}
