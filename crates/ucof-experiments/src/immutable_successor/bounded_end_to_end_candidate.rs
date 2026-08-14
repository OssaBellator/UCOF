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
        include!("bounded_end_to_end_candidate/encrypted_sorter.rs");
        include!("bounded_end_to_end_candidate/encrypted_tree_stage.rs");
        include!("../private_nonce_lease_contract.rs");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    use encrypted_descriptor_crypto::{
        encrypt_descriptor_for_sorter, encrypted_sorter_limits,
        sort_encrypted_descriptors_to_retained_stage, transcode_descriptor_stage,
        DescriptorCryptoContext, DescriptorEncryptionSession, DescriptorNonceAuthority,
        EncryptedDescriptorReader, EncryptedDescriptorStage, EncryptedRecordStage,
        EncryptedTreeStageKind, ENCRYPTED_DESCRIPTOR_STAGE_BYTES, ENCRYPTED_LOCATOR_STAGE_BYTES,
        ENCRYPTED_PAGE_REF_STAGE_BYTES, ENCRYPTED_SORTER_FRAME_BYTES,
        ENCRYPTED_SORTER_PAYLOAD_BYTES,
    };

    include!("bounded_end_to_end_candidate/prepared.rs");
    include!("bounded_end_to_end_candidate/quota.rs");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    include!("bounded_end_to_end_candidate/encrypted_sorter_writer.rs");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    include!("bounded_end_to_end_candidate/encrypted_sorter_quota.rs");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    include!("bounded_end_to_end_candidate/encrypted_tree_writer.rs");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    include!("bounded_end_to_end_candidate/encrypted_tree_quota.rs");
    include!("bounded_end_to_end_candidate/published_quota.rs");
    include!("bounded_end_to_end_candidate/staged_publication.rs");
    include!("bounded_end_to_end_candidate/tests.rs");
    include!("bounded_end_to_end_candidate/quota_tests.rs");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    include!("bounded_end_to_end_candidate/encrypted_descriptor_tests.rs");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    include!("bounded_end_to_end_candidate/encrypted_sorter_tests.rs");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    include!("bounded_end_to_end_candidate/encrypted_tree_tests.rs");
    include!("bounded_end_to_end_candidate/post_preflight_failure_tests.rs");
    include!("bounded_end_to_end_candidate/prepared_tests.rs");
    include!("bounded_end_to_end_candidate/staged_publication_tests.rs");
}
