#![cfg_attr(test, allow(clippy::io_other_error))]

//! Non-normative research models for Phase 3.
//!
//! Nothing in this crate defines UCOF wire compatibility. The models exist to
//! test paged-directory, snapshot-selection, enumeration, recovery,
//! publication, repair, compaction, and provisional EXP-0002 byte invariants.
//! Range-source APIs deliberately separate targeted authenticated lookup from
//! full exact-end validation and explicitly requested recovery.

mod active_file_selected_streaming;
pub mod bounded_spill_sort;
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
mod mixed_reference_plan;
mod mixed_tree_plan;
mod mixed_update_plan;
#[cfg(test)]
mod private_stage_crypto_contract_v2;
mod publication;
mod recovery;
mod repair;
mod semantic_history_multi_plan;
mod semantic_history_streaming_compaction;
mod semantic_streaming_compaction;
mod snapshots;

pub use active_file_selected_streaming::{
    rewrite_selected_active_file_to, ImmutableSelectedActiveStreamingReport,
};
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
pub use mixed_reference_plan::{
    plan_mixed_page_references, MixedReferencePlan, MixedReferencePlanError, PlannedPageIdentity,
};
pub use mixed_tree_plan::{
    plan_mixed_tree_updates, MixedRootTransition, MixedTreePlan, MixedTreePlanError,
    MixedTreePlanLimits, MixedTreeShape,
};
pub use mixed_update_plan::{
    plan_mixed_leaf_updates, MixedLeafPlan, MixedLeafPlanError, MixedLeafPlanLimits,
    MixedLeafRepairAction, MixedPlanOperation,
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
pub use semantic_history_multi_plan::{
    plan_historical_semantic_selections, HistoricalSemanticSelectionEntry,
    HistoricalSemanticSelectionError, HistoricalSemanticSelectionLimits,
    HistoricalSemanticSelectionPlan, HistoricalSemanticSelectionRequest,
};
pub use semantic_history_streaming_compaction::{
    rewrite_compacted_versioned_history_sequence_to, ImmutableHistoricalSemanticStreamingError,
    ImmutableHistoricalSemanticStreamingOptions, ImmutableHistoricalSemanticStreamingReport,
};
pub use semantic_streaming_compaction::{
    rewrite_compacted_active_file_to, ImmutableSemanticStreamingError,
    ImmutableSemanticStreamingReport,
};
pub use snapshots::{
    CandidateStatus, CheckpointKind, RejectedCandidate, RootRejection, RootSelectionError,
    RootSelectionLimits, RootSelectionMode, RootSelectionReport, SnapshotCandidate,
    SnapshotIdentity,
};
