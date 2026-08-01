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
pub mod exp0002_source_chain;
pub mod exp0002_source_recovery;
#[allow(dead_code, unused_imports)]
mod exp0002_source_strict;
pub mod exp0002_source_version;
/// Reusable, non-normative immutable-page successor byte experiment.
pub mod immutable_successor;
mod publication;
mod recovery;
mod repair;
mod snapshots;
mod spill_publication;
mod spill_restart;
#[cfg(unix)]
mod unix_spill_fault_injection;
#[cfg(unix)]
mod unix_spill_publication;
#[cfg(unix)]
mod unix_spill_restart;

pub use compaction::{CompactionError, CompactionLimits, CompactionPlan, ObjectGraph};
pub use directory::{
    DirectoryBuildError, DirectoryLookupError, DirectoryStats, LookupResult, ObjectLocator,
    PagedDirectory,
};
pub use enumeration::{
    EnumeratedRoot, EnumeratedRootStatus, EnumerationLimits, RootEnumerationError,
    RootEnumerationReport,
};
pub use exp0002_source_chain::{
    enumerate_previous_chain_at, Exp0002SourceChainLimits, Exp0002SourceChainReport,
};
pub use exp0002_source_recovery::{
    scan_valid_prefixes_at, Exp0002SourceRecoveryLimits, Exp0002SourceRecoveryReport,
    RecoveredExp0002SourcePrefix,
};
pub use exp0002_source_strict::{validate_strict_at, VerifiedExp0002Source};
pub use exp0002_source_version::{
    Exp0002SourceVersion, Exp0002StableSource, Exp0002VersionedReadAt,
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
pub use spill_publication::{
    NoOverwriteLinkResult, SpillConfidentialityPolicy, SpillPublicationError,
    SpillPublicationLimits, SpillPublicationOutcome, SpillPublicationSession,
    SpillPublicationStage,
};
pub use spill_restart::{
    classify_spill_restart, SpillRestartDisposition, SpillRestartFacts,
    SpillRestartJournalEvidence, SpillRestartJournalPhase, SpillRestartOwnership,
    SpillRestartValidation,
};
#[cfg(unix)]
pub use unix_spill_fault_injection::{
    run_fault_injected_unix_publication, UnixSpillFaultPoint, UnixSpillFaultReport,
};
#[cfg(unix)]
pub use unix_spill_publication::{
    publish_bytes_no_overwrite, UnixSpillPublicationError, UnixSpillPublicationReport,
};
#[cfg(unix)]
pub use unix_spill_restart::{
    inspect_unix_spill_after_restart, UnixSpillRestartExpectedArtifact, UnixSpillRestartInspection,
};
