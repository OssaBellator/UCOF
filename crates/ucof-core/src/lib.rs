//! Experimental implementation of `UCOF-EXP-0001`.
//!
//! The APIs and byte layout in this crate are intentionally unstable. The
//! normative experiment text lives in `spec/experimental/UCOF-EXP-0001.md`.

mod cbor;
mod diagnostic;
mod error;
mod format;
mod limits;
mod model;
mod reader;
mod source;
mod source_validate;
mod stream;
mod stream_writer;
mod writer;

pub use cbor::{decode_canonical, encode_canonical, Value as CborValue};
pub use diagnostic::{
    Diagnostic, DiagnosticReport, DiagnosticStage, DiagnosticStatus, DiagnosticValidator,
    PrefixSalvageReport, PrefixSalvager, SalvagedRecord,
};
pub use error::{Error, ErrorCategory};
pub use limits::Limits;
pub use model::{DirectoryEntry, Manifest, RecordKind};
pub use reader::{RecordInfo, ValidatedFile};
pub use source::{
    InspectionReport, IntegrityStatus, MetadataInspector, ReadAt, ReadStats, SeekSource,
    SliceSource,
};
pub use source_validate::{SourceValidationReport, SourceValidationStats, SourceValidator};
pub use stream::{SequentialReader, StreamCommit, StreamEvent, StreamRecord, StreamStats};
pub use stream_writer::{FinishedWriter, SeekableWriter, StreamingWriter};
pub use writer::Writer;

pub const EXPERIMENTAL_EPOCH: u32 = 1;
