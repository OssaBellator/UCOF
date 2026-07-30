//! Non-normative research models for Phase 3.
//!
//! Nothing in this crate defines UCOF wire compatibility. The models exist to
//! test paged-directory, snapshot-selection, enumeration, recovery,
//! publication, repair, compaction, and provisional EXP-0002 byte invariants.
//! Range-source APIs deliberately separate targeted authenticated lookup from
//! full exact-end validation and explicitly requested recovery.

mod compaction;
mod directory;
mod enumeration;
pub mod exp0002;
pub mod exp0002_lookup;
pub mod exp0002_recovery;
pub mod exp0002_rewrite;
pub mod exp0002_source;
pub mod exp0002_source_recovery;
mod exp0002_source_strict;
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
pub use exp0002_source_recovery::{
    scan_valid_prefixes_at, Exp0002SourceRecoveryLimits, Exp0002SourceRecoveryReport,
    RecoveredExp0002SourcePrefix,
};
pub use exp0002_source_strict::{validate_strict_at, VerifiedExp0002Source};
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
