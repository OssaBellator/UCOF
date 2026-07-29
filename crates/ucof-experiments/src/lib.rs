//! Non-normative research models for Phase 3.
//!
//! Nothing in this crate defines UCOF wire compatibility. The models exist to
//! test paged-directory, snapshot-selection, recovery, and compaction
//! invariants before an EXP-0002 byte layout is selected.

mod compaction;
mod directory;
mod recovery;
mod snapshots;

pub use compaction::{CompactionError, CompactionLimits, CompactionPlan, ObjectGraph};
pub use directory::{
    DirectoryBuildError, DirectoryLookupError, DirectoryStats, LookupResult, ObjectLocator,
    PagedDirectory,
};
pub use recovery::{
    has_exact_end_candidate, scan_backwards, RecoveryScanError, RecoveryScanLimits,
    RecoveryScanReport, ScannedCandidate,
};
pub use snapshots::{
    CandidateStatus, CheckpointKind, RejectedCandidate, RootRejection, RootSelectionError,
    RootSelectionLimits, RootSelectionMode, RootSelectionReport, SnapshotCandidate,
    SnapshotIdentity,
};
