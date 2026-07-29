//! Non-normative research models for Phase 3.
//!
//! Nothing in this crate defines UCOF wire compatibility. The models exist to
//! test paged-directory, snapshot-selection, recovery, and compaction
//! invariants before an EXP-0002 byte layout is selected.

mod compaction;
mod directory;
mod snapshots;

pub use compaction::{CompactionError, CompactionLimits, CompactionPlan, ObjectGraph};
pub use directory::{
    DirectoryBuildError, DirectoryLookupError, DirectoryStats, LookupResult, ObjectLocator,
    PagedDirectory,
};
pub use snapshots::{
    CandidateStatus, CheckpointKind, RootSelectionError, RootSelectionLimits, RootSelectionMode,
    RootSelectionReport, SnapshotCandidate, SnapshotIdentity,
};
