#[test]
fn exact_verified_restart_stage_continues_to_canonical_output_with_fresh_lease() {
    const OBJECTS: u64 = 17;
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, persisted) =
        prepared_encrypted_restart_stage(
            "restart-continuation-exact",
            OBJECTS,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist exact restart stage");
    let work_directory = super::TestDirectory::new("restart-continuation-work");
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
    let evidence = continue_verified_encrypted_spill_with_fresh_lease(
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
        fresh_operation_id: [0x73; 16],
    },
    )
    .expect("fresh-lease restart continuation");

    assert_eq!(resumed, baseline);
    assert_eq!(evidence.output.output, baseline_report);
    assert_eq!(evidence.crashed_generation, 1);
    assert_eq!(evidence.fresh_generation, 2);
    assert_eq!(evidence.crashed_lease_first, 0);
    assert_eq!(evidence.crashed_lease_last, OBJECTS * 2 - 1);
    assert_eq!(evidence.fresh_lease_first, OBJECTS * 2);
    assert_eq!(evidence.fresh_lease_last, OBJECTS * 3 - 1);
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
        .expect("recover post-continuation authority");
    assert_eq!(authority.durable.generation, 2);
    assert_eq!(authority.next_unreserved(), Some(OBJECTS * 3));
    assert_eq!(directory_entry_count(&stage_directory.0), 1);
    work_directory.assert_empty();
}

#[test]
fn renamed_verified_restart_stage_continues_to_same_canonical_output() {
    const OBJECTS: u64 = 9;
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, persisted) =
        prepared_encrypted_restart_stage(
            "restart-continuation-renamed",
            OBJECTS,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist renamed restart stage");
    let expected_name = encrypted_stage_file_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let renamed = OsString::from("restart-continuation-renamed.bin");
    std::fs::rename(
        stage_directory.0.join(expected_name),
        stage_directory.0.join(&renamed),
    )
    .expect("rename persisted stage");
    let pinned = linux_nonce_open_private_directory(&stage_directory.0).expect("pin renamed directory");
    pinned.sync_all().expect("sync renamed directory");

    let work_directory = super::TestDirectory::new("restart-continuation-renamed-work");
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
    .expect("baseline renamed canonical writer");

    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut resumed_sources = original;
    let mut resumed = Vec::new();
    let evidence = continue_verified_encrypted_spill_with_fresh_lease(
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
        fresh_operation_id: [0x74; 16],
    },
    )
    .expect("renamed fresh-lease continuation");
    assert_eq!(resumed, baseline);
    assert_eq!(evidence.output.output, baseline_report);
    assert_eq!(evidence.fresh_lease_first, OBJECTS * 2);
    assert_eq!(directory_entry_count(&stage_directory.0), 1);
    work_directory.assert_empty();
}

#[test]
fn unmanifested_durable_stage_does_not_allocate_fresh_restart_lease() {
    const OBJECTS: u64 = 5;
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, persisted) =
        prepared_encrypted_restart_stage(
            "restart-continuation-no-manifest",
            OBJECTS,
            EncryptedStageManifestCommitCut::AfterStageSyncBeforeManifest,
        );
    assert_eq!(
        persisted.expect_err("manifest cut"),
        LinuxEncryptedStageRestartError::InjectedCut(
            EncryptedStageManifestCommitCut::AfterStageSyncBeforeManifest
        )
    );
    let work_directory = super::TestDirectory::new("restart-continuation-no-manifest-work");
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut sources: Vec<_> = (1..=OBJECTS)
        .rev()
        .map(super::TinySource::new)
        .collect();
    let mut output = Vec::new();
    let error = continue_verified_encrypted_spill_with_fresh_lease(
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
        fresh_operation_id: [0x75; 16],
    },
    )
    .expect_err("unmanifested stage must not continue");
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
fn source_mismatch_after_fresh_lease_burns_new_range_without_old_nonce_reuse() {
    const OBJECTS: u64 = 7;
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, persisted) =
        prepared_encrypted_restart_stage(
            "restart-continuation-source-mismatch",
            OBJECTS,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist source-mismatch restart stage");
    let work_directory = super::TestDirectory::new("restart-continuation-source-mismatch-work");
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut wrong_order_sources: Vec<_> = (1..=OBJECTS).map(super::TinySource::new).collect();
    let mut output = Vec::new();
    let error = continue_verified_encrypted_spill_with_fresh_lease(
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
        fresh_operation_id: [0x76; 16],
    },
    )
    .expect_err("source index mismatch must fail");
    assert!(error.contains("metadata changed"));
    assert_eq!(&output[..super::FILE_HEADER_LEN], &{
        let mut header = [0u8; super::FILE_HEADER_LEN];
        header[..8].copy_from_slice(super::FILE_MAGIC);
        header
    });
    let authority = journal
        .recover_authority(None)
        .expect("recover failed continuation authority");
    assert_eq!(authority.durable.generation, 2);
    assert_eq!(authority.next_unreserved(), Some(OBJECTS * 3));
    work_directory.assert_empty();
}
