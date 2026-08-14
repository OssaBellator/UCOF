#[test]
fn encrypted_tree_restart_continuation_uses_one_fresh_generation_and_canonical_output() {
    const OBJECTS: u64 = 17;
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-tree-restart-continuation",
            OBJECTS,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist encrypted-tree restart stage");
    let work_directory = super::TestDirectory::new("encrypted-tree-restart-work");
    let limits = super::ImmutableLimits::default();
    let original: Vec<_> = (1..=OBJECTS)
        .rev()
        .map(super::TinySource::new)
        .collect();

    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        limits,
    )
    .expect("baseline canonical writer");

    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut resumed_sources = original;
    let mut resumed = Vec::new();
    let evidence = continue_verified_encrypted_spill_with_fresh_tree_lease(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut resumed,
        &mut resumed_sources,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits,
            fresh_operation_id: [0x93; 16],
        },
    )
    .expect("encrypted-tree restart continuation");

    let tree_nonces = consolidated_encrypted_tree_stage_record_count(
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect("tree nonce count");
    let fresh_size = OBJECTS.checked_add(tree_nonces).expect("fresh lease size");
    assert_eq!(resumed, baseline);
    assert_eq!(evidence.output.output, baseline_report);
    assert_eq!(evidence.crashed_generation, 1);
    assert_eq!(evidence.fresh_generation, 2);
    assert_eq!(evidence.crashed_lease_first, 0);
    assert_eq!(evidence.crashed_lease_last, OBJECTS * 2 - 1);
    assert_eq!(evidence.fresh_lease_first, OBJECTS * 2);
    assert_eq!(
        evidence.fresh_lease_last,
        evidence.fresh_lease_first + fresh_size - 1
    );
    assert_ne!(evidence.tree_stage_ciphertext_sha256, [0u8; 32]);
    let manifest = load_encrypted_stage_manifest(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load persisted manifest")
    .expect("persisted manifest");
    assert_eq!(evidence.persisted_spill_sha256, manifest.stage_sha256);
    let authority = journal
        .recover_authority(None)
        .expect("recover encrypted-tree continuation authority");
    assert_eq!(authority.durable.generation, 2);
    assert_eq!(
        authority.next_unreserved(),
        Some(OBJECTS * 2 + fresh_size)
    );
    assert_eq!(directory_entry_count(&stage_directory.0), 1);
    work_directory.assert_empty();
}

#[test]
fn encrypted_tree_restart_no_manifest_does_not_allocate_combined_fresh_lease() {
    const OBJECTS: u64 = 5;
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-tree-restart-no-manifest",
            OBJECTS,
            EncryptedStageManifestCommitCut::AfterStageSyncBeforeManifest,
        );
    assert_eq!(
        persisted.expect_err("manifest cut"),
        LinuxEncryptedStageRestartError::InjectedCut(
            EncryptedStageManifestCommitCut::AfterStageSyncBeforeManifest
        )
    );
    let work_directory = super::TestDirectory::new("encrypted-tree-restart-no-manifest-work");
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut sources: Vec<_> = (1..=OBJECTS)
        .rev()
        .map(super::TinySource::new)
        .collect();
    let mut output = Vec::new();
    let error = continue_verified_encrypted_spill_with_fresh_tree_lease(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut output,
        &mut sources,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0x94; 16],
        },
    )
    .expect_err("unmanifested stage must not allocate encrypted-tree lease");
    assert!(error.contains("no durable manifest"));
    assert!(output.is_empty());
    let authority = journal
        .recover_authority(None)
        .expect("recover no-manifest authority");
    assert_eq!(authority.durable.generation, 1);
    assert_eq!(authority.next_unreserved(), Some(OBJECTS * 2));
    work_directory.assert_empty();
}

#[test]
fn encrypted_tree_restart_source_mismatch_burns_combined_fresh_range_without_reuse() {
    const OBJECTS: u64 = 7;
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-tree-restart-source-mismatch",
            OBJECTS,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist encrypted-tree mismatch stage");
    let work_directory = super::TestDirectory::new("encrypted-tree-restart-mismatch-work");
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut wrong_order_sources: Vec<_> = (1..=OBJECTS).map(super::TinySource::new).collect();
    let mut output = Vec::new();
    let error = continue_verified_encrypted_spill_with_fresh_tree_lease(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut output,
        &mut wrong_order_sources,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0x95; 16],
        },
    )
    .expect_err("source mismatch must fail after combined lease commit");
    assert!(error.contains("metadata changed"));
    assert_eq!(&output[..super::FILE_HEADER_LEN], &{
        let mut header = [0u8; super::FILE_HEADER_LEN];
        header[..8].copy_from_slice(super::FILE_MAGIC);
        header
    });
    let tree_nonces = consolidated_encrypted_tree_stage_record_count(
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect("tree nonce count");
    let fresh_size = OBJECTS.checked_add(tree_nonces).expect("fresh lease size");
    let authority = journal
        .recover_authority(None)
        .expect("recover failed encrypted-tree continuation authority");
    assert_eq!(authority.durable.generation, 2);
    assert_eq!(
        authority.next_unreserved(),
        Some(OBJECTS * 2 + fresh_size)
    );
    work_directory.assert_empty();
}
