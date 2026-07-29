use crate::format::{
    read_u16_le, read_u32_le, read_u64_le, FILE_MAGIC, HEADER_LEN, RECORD_HEADER_LEN, RECORD_MAGIC,
};
use crate::{
    Error, ErrorCategory, InspectionReport, Limits, MetadataInspector, ReadAt, RecordKind,
    SourceValidationReport, SourceValidator,
};
use std::collections::BTreeSet;

/// Validation stage that produced a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticStage {
    Bootstrap,
    Structure,
    Integrity,
    Salvage,
}

/// One bounded, implementation-neutral diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub stage: DiagnosticStage,
    pub category: ErrorCategory,
    pub offset: Option<u64>,
    pub message: String,
}

/// Highest assurance established by a diagnostic operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticStatus {
    Invalid,
    StructurallyValid,
    Verified,
    /// Complete records were found, but no active commit was validated.
    UnverifiedPrefix,
}

/// Result of strict structural and integrity diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticReport {
    pub status: DiagnosticStatus,
    pub diagnostics: Vec<Diagnostic>,
    pub inspection: Option<InspectionReport>,
    pub validation: Option<SourceValidationReport>,
}

/// Strict diagnostic facade over metadata inspection and source validation.
#[derive(Debug, Clone, Copy)]
pub struct DiagnosticValidator {
    limits: Limits,
}

impl DiagnosticValidator {
    #[must_use]
    pub const fn new(limits: Limits) -> Self {
        Self { limits }
    }

    pub fn diagnose<S: ReadAt>(&self, source: &mut S) -> Result<DiagnosticReport, Error> {
        if self.limits.max_diagnostics == 0 {
            return Err(Error::LimitExceeded("diagnostics"));
        }

        let inspection = match MetadataInspector::new(self.limits).inspect(source) {
            Ok(report) => report,
            Err(error) => {
                return Ok(DiagnosticReport {
                    status: DiagnosticStatus::Invalid,
                    diagnostics: vec![diagnostic(DiagnosticStage::Structure, None, &error)],
                    inspection: None,
                    validation: None,
                });
            }
        };

        match SourceValidator::new(self.limits).validate(source) {
            Ok(validation) => Ok(DiagnosticReport {
                status: DiagnosticStatus::Verified,
                diagnostics: Vec::new(),
                inspection: Some(inspection),
                validation: Some(validation),
            }),
            Err(error) => Ok(DiagnosticReport {
                status: DiagnosticStatus::Invalid,
                diagnostics: vec![diagnostic(DiagnosticStage::Integrity, None, &error)],
                inspection: Some(inspection),
                validation: None,
            }),
        }
    }
}

impl Default for DiagnosticValidator {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

/// One complete physical record discovered by prefix salvage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SalvagedRecord {
    pub kind: RecordKind,
    pub object_id: u64,
    pub offset: u64,
    pub stored_len: u64,
    pub logical_len: u64,
}

/// Non-conformance salvage result. This type intentionally has no `valid` flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixSalvageReport {
    pub status: DiagnosticStatus,
    pub file_len: u64,
    pub records: Vec<SalvagedRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub reached_directory: bool,
    pub bytes_read: u64,
}

/// Scans complete record framing from the beginning of a source.
///
/// Payload bodies are not read. A returned record is guaranteed only to have a
/// complete in-bounds physical range. No digest, manifest, directory, or active
/// commit is accepted by this API, even when the source happens to be complete.
#[derive(Debug, Clone, Copy)]
pub struct PrefixSalvager {
    limits: Limits,
}

impl PrefixSalvager {
    #[must_use]
    pub const fn new(limits: Limits) -> Self {
        Self { limits }
    }

    pub fn scan<S: ReadAt>(&self, source: &mut S) -> Result<PrefixSalvageReport, Error> {
        if self.limits.max_diagnostics == 0 {
            return Err(Error::LimitExceeded("diagnostics"));
        }

        let file_len = source
            .len()
            .map_err(|_| Error::Io("salvage source length"))?;
        if file_len > self.limits.max_file_bytes {
            return Err(Error::LimitExceeded("file bytes"));
        }

        let mut report = PrefixSalvageReport {
            status: DiagnosticStatus::UnverifiedPrefix,
            file_len,
            records: Vec::new(),
            diagnostics: Vec::new(),
            reached_directory: false,
            bytes_read: 0,
        };

        let header_len = u64::try_from(HEADER_LEN).expect("fixed header length");
        if file_len < header_len {
            push_diagnostic(
                &mut report.diagnostics,
                self.limits.max_diagnostics,
                DiagnosticStage::Bootstrap,
                Some(0),
                Error::Truncated("file header"),
            );
            return Ok(report);
        }

        let mut header = [0_u8; HEADER_LEN];
        read_bounded(
            source,
            0,
            &mut header,
            &mut report.bytes_read,
            &self.limits,
            "salvage file header",
        )?;
        if let Err(error) = validate_header(&header) {
            push_diagnostic(
                &mut report.diagnostics,
                self.limits.max_diagnostics,
                DiagnosticStage::Bootstrap,
                Some(0),
                error,
            );
            return Ok(report);
        }

        let mut offset = header_len;
        let mut identifiers = BTreeSet::new();
        while offset < file_len {
            if u64::try_from(report.records.len())
                .map_or(true, |count| count >= self.limits.max_records)
            {
                push_diagnostic(
                    &mut report.diagnostics,
                    self.limits.max_diagnostics,
                    DiagnosticStage::Salvage,
                    Some(offset),
                    Error::LimitExceeded("record count"),
                );
                break;
            }

            let header_end = match offset
                .checked_add(u64::try_from(RECORD_HEADER_LEN).expect("fixed record header length"))
            {
                Some(end) => end,
                None => {
                    push_diagnostic(
                        &mut report.diagnostics,
                        self.limits.max_diagnostics,
                        DiagnosticStage::Salvage,
                        Some(offset),
                        Error::RangeOutOfBounds("record header"),
                    );
                    break;
                }
            };
            if header_end > file_len {
                push_diagnostic(
                    &mut report.diagnostics,
                    self.limits.max_diagnostics,
                    DiagnosticStage::Salvage,
                    Some(offset),
                    Error::Truncated("record header"),
                );
                break;
            }

            let mut bytes = [0_u8; RECORD_HEADER_LEN];
            read_bounded(
                source,
                offset,
                &mut bytes,
                &mut report.bytes_read,
                &self.limits,
                "salvage record header",
            )?;
            let record = match parse_record(&bytes, offset, &self.limits, &mut identifiers) {
                Ok(record) => record,
                Err(error) => {
                    push_diagnostic(
                        &mut report.diagnostics,
                        self.limits.max_diagnostics,
                        DiagnosticStage::Salvage,
                        Some(offset),
                        error,
                    );
                    break;
                }
            };

            let payload_end = match header_end.checked_add(record.stored_len) {
                Some(end) => end,
                None => {
                    push_diagnostic(
                        &mut report.diagnostics,
                        self.limits.max_diagnostics,
                        DiagnosticStage::Salvage,
                        Some(offset),
                        Error::RangeOutOfBounds("record payload"),
                    );
                    break;
                }
            };
            if payload_end > file_len {
                push_diagnostic(
                    &mut report.diagnostics,
                    self.limits.max_diagnostics,
                    DiagnosticStage::Salvage,
                    Some(offset),
                    Error::Truncated("record payload"),
                );
                break;
            }

            report.records.push(record);
            offset = payload_end;
            if record.kind == RecordKind::Directory {
                report.reached_directory = true;
                break;
            }
        }

        Ok(report)
    }
}

impl Default for PrefixSalvager {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

fn validate_header(bytes: &[u8; HEADER_LEN]) -> Result<(), Error> {
    if bytes[..FILE_MAGIC.len()] != FILE_MAGIC {
        return Err(Error::InvalidMagic("file"));
    }
    let epoch = read_u32_le(bytes, 8, "epoch")?;
    if epoch != crate::EXPERIMENTAL_EPOCH {
        return Err(Error::UnsupportedEpoch(epoch));
    }
    if read_u32_le(bytes, 12, "file flags")? != 0 {
        return Err(Error::UnsupportedFlags(
            "file",
            u64::from(read_u32_le(bytes, 12, "file flags")?),
        ));
    }
    if read_u32_le(bytes, 16, "file header length")?
        != u32::try_from(HEADER_LEN).expect("fixed header length")
    {
        return Err(Error::InvalidLength("file header"));
    }
    if bytes[20..].iter().any(|byte| *byte != 0) {
        return Err(Error::InvalidReserved("file header"));
    }
    Ok(())
}

fn parse_record(
    bytes: &[u8; RECORD_HEADER_LEN],
    offset: u64,
    limits: &Limits,
    identifiers: &mut BTreeSet<u64>,
) -> Result<SalvagedRecord, Error> {
    if bytes[..RECORD_MAGIC.len()] != RECORD_MAGIC {
        return Err(Error::InvalidMagic("record"));
    }
    let kind = RecordKind::try_from(read_u16_le(bytes, 4, "record kind")?)?;
    let flags = read_u16_le(bytes, 6, "record flags")?;
    if flags != 0 {
        return Err(Error::UnsupportedFlags("record", u64::from(flags)));
    }
    if read_u32_le(bytes, 8, "record header length")?
        != u32::try_from(RECORD_HEADER_LEN).expect("fixed record header length")
    {
        return Err(Error::InvalidLength("record header"));
    }
    let stored_len = read_u64_le(bytes, 12, "stored length")?;
    let logical_len = read_u64_le(bytes, 20, "logical length")?;
    if stored_len != logical_len {
        return Err(Error::InvalidLength("transformed logical length"));
    }
    if stored_len > limits.max_payload_bytes {
        return Err(Error::LimitExceeded("record payload bytes"));
    }
    let object_id = read_u64_le(bytes, 28, "object identifier")?;
    if read_u32_le(bytes, 36, "record reserved")? != 0 {
        return Err(Error::InvalidReserved("record header"));
    }
    if kind == RecordKind::Directory {
        if object_id != 0 {
            return Err(Error::InvalidRecordOrder(
                "directory identifier must be zero",
            ));
        }
    } else if object_id == 0 {
        return Err(Error::InvalidRecordOrder(
            "non-directory identifier is zero",
        ));
    } else if !identifiers.insert(object_id) {
        return Err(Error::DuplicateObjectId(object_id));
    }

    Ok(SalvagedRecord {
        kind,
        object_id,
        offset,
        stored_len,
        logical_len,
    })
}

fn read_bounded<S: ReadAt>(
    source: &mut S,
    offset: u64,
    bytes: &mut [u8],
    bytes_read: &mut u64,
    limits: &Limits,
    context: &'static str,
) -> Result<(), Error> {
    let length =
        u64::try_from(bytes.len()).map_err(|_| Error::LimitExceeded("total bytes read"))?;
    let next = bytes_read
        .checked_add(length)
        .ok_or(Error::LimitExceeded("total bytes read"))?;
    if next > limits.max_total_bytes_read {
        return Err(Error::LimitExceeded("total bytes read"));
    }
    source
        .read_exact_at(offset, bytes)
        .map_err(|_| Error::Io(context))?;
    *bytes_read = next;
    Ok(())
}

fn diagnostic(stage: DiagnosticStage, offset: Option<u64>, error: &Error) -> Diagnostic {
    Diagnostic {
        stage,
        category: error.category(),
        offset,
        message: error.to_string(),
    }
}

fn push_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    maximum: usize,
    stage: DiagnosticStage,
    offset: Option<u64>,
    error: Error,
) {
    if diagnostics.len() < maximum {
        diagnostics.push(diagnostic(stage, offset, &error));
    }
}
