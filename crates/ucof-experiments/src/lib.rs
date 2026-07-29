//! Non-normative research models for Phase 3.
//!
//! Nothing in this crate defines UCOF wire compatibility. The models exist to
//! test paged-directory, snapshot-selection, enumeration, recovery,
//! publication, repair, and compaction invariants before an EXP-0002 byte
//! layout is selected.

mod compaction;
mod directory;
mod enumeration;
mod publication;
mod recovery;
mod repair;
mod snapshots;

pub use compaction::{CompactionError, CompactionLimits, CompactionPlan, ObjectGraph};
pub use directory::{
    DirectoryBuildError, DirectoryLookupError, DirectoryStats, LookupResult, ObjectLocator,
    PagedDirectory,
};
pub use enumeration::{
    EnumeratedRoot, EnumeratedRootStatus, EnumerationLimits, RootEnumerationError,
    RootEnumerationReport,
};
pub use publication::{
    PublicationError, PublicationLimits, PublicationModel, PublicationReport, PublicationStage,
    PublishedCheckpoint,
};
pub use recovery::{
    has_exact_end_candidate, scan_backwards, RecoveryScanError, RecoveryScanLimits,
    RecoveryScanReport, ScannedCandidate,
};
pub use repair::{CopyRange, RepairError, RepairLimits, RepairPlan};
pub use snapshots::{
    CandidateStatus, CheckpointKind, RejectedCandidate, RootRejection, RootSelectionError,
    RootSelectionLimits, RootSelectionMode, RootSelectionReport, SnapshotCandidate,
    SnapshotIdentity,
};
