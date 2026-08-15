#[cfg(test)]
mod bounded_end_to_end_candidate_tests {
    use super::*;
    use crate::bounded_spill_sort::{
        bounded_spill_sort_to, BoundedSpillRecord, BoundedSpillSortLimits, BoundedSpillSortReport,
    };
    use std::fs::{self, File, OpenOptions};
    use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    mod group_iter {
        include!("../canonical_group_iter_candidate.rs");
    }
    use group_iter::CanonicalGroupSizesIter;

    include!("bounded_end_to_end_candidate/stage.rs");
    include!("bounded_end_to_end_candidate/writer.rs");

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    mod encrypted_descriptor_crypto {
        include!("bounded_end_to_end_candidate/encrypted_descriptor.rs");
        include!("bounded_end_to_end_candidate/encrypted_descriptor_spill.rs");
        include!("bounded_end_to_end_candidate/linux_durable_nonce_journal.rs");
        include!("bounded_end_to_end_candidate/linux_encrypted_stage_restart.rs");
        include!("bounded_end_to_end_candidate/encrypted_restart_continuation.rs");
        include!("bounded_end_to_end_candidate/encrypted_restart_publication.rs");
        include!("bounded_end_to_end_candidate/encrypted_restart_retirement.rs");
        include!("bounded_end_to_end_candidate/encrypted_private_lifecycle_quota.rs");
        include!("bounded_end_to_end_candidate/encrypted_private_lifecycle_quota_enforcement.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_stage.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_emission.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_pipeline.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_lifecycle_quota.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_restart_continuation.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_restart_publication.rs");
        include!("bounded_end_to_end_candidate/restart_source_set_authority.rs");
        include!("bounded_end_to_end_candidate/source_bound_restart_continuation.rs");
        include!("bounded_end_to_end_candidate/source_bound_lifecycle_quota.rs");
        include!("bounded_end_to_end_candidate/source_bound_restart_enforcement.rs");
        include!("bounded_end_to_end_candidate/restart_metadata_compaction.rs");
        include!("bounded_end_to_end_candidate/compacted_restart_classification.rs");
        include!("bounded_end_to_end_candidate/compacted_source_bound_restart.rs");
        include!("bounded_end_to_end_candidate/compacted_private_lifecycle_quota.rs");
        include!("bounded_end_to_end_candidate/encrypted_descriptor_spill_tests.rs");
        include!("bounded_end_to_end_candidate/linux_durable_nonce_journal_tests.rs");
        include!("bounded_end_to_end_candidate/linux_durable_nonce_journal_capacity_tests.rs");
        include!("bounded_end_to_end_candidate/linux_encrypted_stage_restart_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_restart_continuation_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_restart_publication_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_restart_retirement_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_private_lifecycle_quota_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_private_lifecycle_quota_enforcement_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_stage_consolidation_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_pipeline_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_lifecycle_quota_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_restart_continuation_tests.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_restart_publication_tests.rs");
        include!("bounded_end_to_end_candidate/restart_source_set_authority_tests.rs");
        include!("bounded_end_to_end_candidate/source_bound_lifecycle_quota_tests.rs");
        include!("bounded_end_to_end_candidate/restart_metadata_compaction_tests.rs");
        include!("bounded_end_to_end_candidate/restart_metadata_compaction_retry_tests.rs");
        include!("bounded_end_to_end_candidate/restart_metadata_compaction_graph_tests.rs");
        include!("bounded_end_to_end_candidate/restart_metadata_compaction_checkpoint_consistency_tests.rs");
        include!("bounded_end_to_end_candidate/restart_metadata_compaction_property_tests.rs");
        include!("bounded_end_to_end_candidate/restart_metadata_compaction_accounting_tests.rs");
        include!("bounded_end_to_end_candidate/compacted_restart_retry_tests.rs");
        include!("bounded_end_to_end_candidate/compacted_private_lifecycle_quota_tests.rs");
        include!("../private_nonce_lease_contract.rs");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    use encrypted_descriptor_crypto::{
        transcode_descriptor_stage, DescriptorEncryptionSession, DescriptorNonceAuthority,
        EncryptedDescriptorReader, EncryptedDescriptorStage, ENCRYPTED_DESCRIPTOR_STAGE_BYTES,
    };

    include!("bounded_end_to_end_candidate/prepared.rs");
    include!("bounded_end_to_end_candidate/quota.rs");
    include!("bounded_end_to_end_candidate/published_quota.rs");
    include!("bounded_end_to_end_candidate/staged_publication.rs");
    include!("bounded_end_to_end_candidate/tests.rs");
    include!("bounded_end_to_end_candidate/quota_tests.rs");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    include!("bounded_end_to_end_candidate/encrypted_descriptor_tests.rs");
    include!("bounded_end_to_end_candidate/post_preflight_failure_tests.rs");
    include!("bounded_end_to_end_candidate/prepared_tests.rs");
    include!("bounded_end_to_end_candidate/staged_publication_tests.rs");
}
