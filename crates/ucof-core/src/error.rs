use std::fmt;

/// Stable conceptual error categories for the experimental conformance suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Truncated,
    InvalidMagic,
    UnsupportedEpoch,
    UnsupportedFlags,
    InvalidReserved,
    InvalidLength,
    RangeOutOfBounds,
    DuplicateObjectId,
    InvalidRecordOrder,
    UnsupportedRecordKind,
    NonCanonicalMetadata,
    InvalidMetadataSchema,
    DirectoryMismatch,
    MissingManifest,
    UnsupportedRequiredCapability,
    DigestMismatch,
    LimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated(&'static str),
    InvalidMagic(&'static str),
    UnsupportedEpoch(u32),
    UnsupportedFlags(&'static str, u64),
    InvalidReserved(&'static str),
    InvalidLength(&'static str),
    RangeOutOfBounds(&'static str),
    DuplicateObjectId(u64),
    InvalidRecordOrder(&'static str),
    UnsupportedRecordKind(u16),
    NonCanonicalMetadata(&'static str),
    InvalidMetadataSchema(&'static str),
    DirectoryMismatch(&'static str),
    MissingManifest(u64),
    UnsupportedRequiredCapability(u64),
    DigestMismatch,
    LimitExceeded(&'static str),
}

impl Error {
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        match self {
            Self::Truncated(_) => ErrorCategory::Truncated,
            Self::InvalidMagic(_) => ErrorCategory::InvalidMagic,
            Self::UnsupportedEpoch(_) => ErrorCategory::UnsupportedEpoch,
            Self::UnsupportedFlags(_, _) => ErrorCategory::UnsupportedFlags,
            Self::InvalidReserved(_) => ErrorCategory::InvalidReserved,
            Self::InvalidLength(_) => ErrorCategory::InvalidLength,
            Self::RangeOutOfBounds(_) => ErrorCategory::RangeOutOfBounds,
            Self::DuplicateObjectId(_) => ErrorCategory::DuplicateObjectId,
            Self::InvalidRecordOrder(_) => ErrorCategory::InvalidRecordOrder,
            Self::UnsupportedRecordKind(_) => ErrorCategory::UnsupportedRecordKind,
            Self::NonCanonicalMetadata(_) => ErrorCategory::NonCanonicalMetadata,
            Self::InvalidMetadataSchema(_) => ErrorCategory::InvalidMetadataSchema,
            Self::DirectoryMismatch(_) => ErrorCategory::DirectoryMismatch,
            Self::MissingManifest(_) => ErrorCategory::MissingManifest,
            Self::UnsupportedRequiredCapability(_) => {
                ErrorCategory::UnsupportedRequiredCapability
            }
            Self::DigestMismatch => ErrorCategory::DigestMismatch,
            Self::LimitExceeded(_) => ErrorCategory::LimitExceeded,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(context) => write!(f, "truncated {context}"),
            Self::InvalidMagic(context) => write!(f, "invalid {context} magic"),
            Self::UnsupportedEpoch(epoch) => write!(f, "unsupported experimental epoch {epoch}"),
            Self::UnsupportedFlags(context, flags) => {
                write!(f, "unsupported {context} flags 0x{flags:x}")
            }
            Self::InvalidReserved(context) => write!(f, "non-zero reserved {context} bytes"),
            Self::InvalidLength(context) => write!(f, "invalid {context} length"),
            Self::RangeOutOfBounds(context) => write!(f, "{context} range is out of bounds"),
            Self::DuplicateObjectId(id) => write!(f, "duplicate object identifier {id}"),
            Self::InvalidRecordOrder(context) => write!(f, "invalid record order: {context}"),
            Self::UnsupportedRecordKind(kind) => write!(f, "unsupported record kind {kind}"),
            Self::NonCanonicalMetadata(context) => {
                write!(f, "non-canonical metadata: {context}")
            }
            Self::InvalidMetadataSchema(context) => {
                write!(f, "invalid metadata schema: {context}")
            }
            Self::DirectoryMismatch(context) => write!(f, "directory mismatch: {context}"),
            Self::MissingManifest(id) => write!(f, "active manifest {id} is missing or invalid"),
            Self::UnsupportedRequiredCapability(id) => {
                write!(f, "unsupported required capability {id}")
            }
            Self::DigestMismatch => write!(f, "committed-prefix digest mismatch"),
            Self::LimitExceeded(limit) => write!(f, "resource limit exceeded: {limit}"),
        }
    }
}

impl std::error::Error for Error {}
