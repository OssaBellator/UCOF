#[test]
fn compacted_source_bound_restart_retries_after_burned_generation() {
    const OBJECTS: u64 = 11;
    let source_set_id = [0x81; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(
            "compacted-retry-after-burn",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("checkpoint generation one before burned retry");

    let object_count = usize::try_from(OBJECTS).expect("object count");
    let burned_size = u64::try_from(object_count)
        .expect("object nonce count")
        .checked_add(
            consolidated_encrypted_tree_stage_record_count(object_count)
                .expect("tree nonce count"),
        )
        .expect("burned retry lease size");
    let compacted = CompactedNonceJournal::new(&journal);
    let mut authority = compacted
        .recover_authority(None)
        .expect("recover generation one before burn");
    let burned = compacted
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x82; 16],
            burned_size,
            JournalCommitCut::Complete,
        )
        .expect("burn generation two");
    assert_eq!(burned.journal_generation, 2);
    drop(burned);
    assert_eq!(
        compacted
            .scan(None)
            .expect("scan burned generation")
            .durable
            .generation,
        2
    );

    let work_directory = super::TestDirectory::new("compacted-retry-after-burn-work");
    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        super::ImmutableLimits::default(),
    )
    .expect("retry baseline writer");

    let mut sources = original;
    let mut output = Vec::new();
    let evidence = continue_compacted_source_bound_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut output,
        &mut sources,
        source_set_id,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0x83; 16],
        },
    )
    .expect("retry from generation-one stage over burned generation two");
    assert_eq!(output, baseline);
    assert_eq!(evidence.output.output, baseline_report);
    assert_eq!(evidence.crashed_generation, 1);
    assert_eq!(evidence.fresh_generation, 3);
    assert!(evidence.fresh_lease_first > evidence.crashed_lease_last);
    assert_eq!(
        compacted
            .scan(None)
            .expect("scan successful retry generation")
            .durable
            .generation,
        3
    );
    work_directory.assert_empty();
}

#[test]
fn compacted_restart_survives_pruned_burn_then_publishes_retires_and_reclaims() {
    const OBJECTS: u64 = 11;
    let source_set_id = [0x91; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(
            "compacted-full-lifecycle",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);

    let first_compaction = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("checkpoint live generation one");
    assert_eq!(first_compaction.checkpoint_generation, 1);
    assert_eq!(first_compaction.preserved_nonce_records, 1);
    assert!(journal_directory.0.join(linux_nonce_journal_name(1)).exists());

    let object_count = usize::try_from(OBJECTS).expect("object count");
    let burned_size = u64::try_from(object_count)
        .expect("object nonce count")
        .checked_add(
            consolidated_encrypted_tree_stage_record_count(object_count)
                .expect("tree nonce count"),
        )
        .expect("burned lifecycle lease size");
    let compacted = CompactedNonceJournal::new(&journal);
    let mut authority = compacted
        .recover_authority(None)
        .expect("recover generation one authority");
    let burned = compacted
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x92; 16],
            burned_size,
            JournalCommitCut::Complete,
        )
        .expect("burn generation two before lifecycle retry");
    assert_eq!(burned.journal_generation, 2);
    drop(burned);

    let second_compaction = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("checkpoint and prune burned generation two");
    assert_eq!(second_compaction.checkpoint_generation, 2);
    assert_eq!(second_compaction.pruned_nonce_records, 1);
    assert_eq!(second_compaction.preserved_nonce_records, 1);
    assert!(journal_directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(!journal_directory.0.join(linux_nonce_journal_name(2)).exists());
    assert!(journal_directory.0.join(nonce_compaction_name(2)).exists());

    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        super::ImmutableLimits::default(),
    )
    .expect("compacted lifecycle baseline");
    let work_directory = super::TestDirectory::new("compacted-full-lifecycle-work");
    let mut sources = original;
    let mut backend =
        RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    let outcome = stage_and_publish_compacted_source_bound_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut backend,
        &mut sources,
        source_set_id,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0x93; 16],
        },
    )
    .expect("publish compacted restart after pruned burn");
    let EncryptedTreeRestartPublicationOutcome::PublishedAndDurable(durable) = outcome else {
        panic!("compacted lifecycle requires durable publication");
    };
    assert_eq!(backend.destination.as_deref(), Some(baseline.as_slice()));
    assert_eq!(durable.durable.continuation.output.output, baseline_report);
    assert_eq!(durable.durable.continuation.crashed_generation, 1);
    assert_eq!(durable.durable.continuation.fresh_generation, 3);
    assert_eq!(
        compacted
            .scan(None)
            .expect("scan compacted durable publication")
            .durable
            .generation,
        3
    );
    work_directory.assert_empty();

    let prepared = prepare_compacted_encrypted_restart_retirement(
        &journal,
        &stage_directory.0,
        &durable.durable,
        restart_limits,
    )
    .expect("prepare compacted retirement after durable publication");
    assert_eq!(prepared.state, EncryptedRetirementState::Prepared);
    assert_eq!(prepared.crashed_generation, 1);
    assert_eq!(prepared.fresh_generation, 3);
    assert_eq!(prepared.output_length, durable.durable.output_length);
    assert_eq!(prepared.output_sha256, durable.durable.output_sha256);

    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &stage_directory.0,
            1,
            3,
            restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("execute compacted lifecycle retirement"),
        EncryptedRetirementOutcome::Terminal
    );
    assert_eq!(directory_entry_count(&stage_directory.0), 0);
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        3,
        EncryptedRetirementState::Terminal,
    )
    .expect("load compacted terminal retirement")
    .is_some());

    let final_compaction = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("reclaim compacted terminal lifecycle metadata");
    assert_eq!(final_compaction.checkpoint_generation, 3);
    assert_eq!(final_compaction.pruned_nonce_records, 2);
    assert_eq!(final_compaction.pruned_retirement_records, 2);
    assert_eq!(final_compaction.pruned_source_set_records, 1);
    assert_eq!(final_compaction.pruned_old_checkpoints, 1);
    assert_eq!(final_compaction.preserved_nonce_records, 0);
    assert_eq!(final_compaction.preserved_prepared_retirements, 0);
    assert_eq!(final_compaction.preserved_source_set_records, 0);

    let final_recovery = compacted
        .scan(None)
        .expect("recover final compacted lifecycle authority");
    assert_eq!(final_recovery.durable.generation, 3);
    assert_eq!(final_recovery.checkpoint_generation, Some(3));
    assert_eq!(final_recovery.journal_records, 0);
    assert!(!journal_directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(!journal_directory.0.join(linux_nonce_journal_name(3)).exists());
    assert!(!journal_directory.0.join(nonce_compaction_name(2)).exists());
    assert!(journal_directory.0.join(nonce_compaction_name(3)).exists());
    assert!(load_restart_source_set_authority(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load reclaimed source-set authority")
    .is_none());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        3,
        EncryptedRetirementState::Prepared,
    )
    .expect("load reclaimed prepared retirement")
    .is_none());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        3,
        EncryptedRetirementState::Terminal,
    )
    .expect("load reclaimed terminal retirement")
    .is_none());
    assert_eq!(directory_entry_count(&journal_directory.0), 1);
}

#[test]
fn compacted_destination_exists_burn_can_be_pruned_and_retried() {
    const OBJECTS: u64 = 11;
    let source_set_id = [0xa1; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(
            "compacted-destination-exists-retry",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("checkpoint live source before destination-exists attempt");
    let compacted = CompactedNonceJournal::new(&journal);
    let work_directory = super::TestDirectory::new("compacted-destination-exists-retry-work");
    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        super::ImmutableLimits::default(),
    )
    .expect("destination-exists retry baseline");

    let mut first_sources = original.clone();
    let mut first_backend = RestartPublicationTestBackend::new(
        super::PersistentPublicationLinkOutcome::DestinationExists,
    );
    first_backend.destination = Some(b"existing destination".to_vec());
    let first = stage_and_publish_compacted_source_bound_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut first_backend,
        &mut first_sources,
        source_set_id,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0xa2; 16],
        },
    )
    .expect("destination-exists compacted restart attempt");
    assert!(matches!(
        first,
        EncryptedTreeRestartPublicationOutcome::NotPublishedDestinationExists
    ));
    assert_eq!(
        first_backend.destination.as_deref(),
        Some(b"existing destination".as_slice())
    );
    assert!(first_backend.private.is_empty());
    assert!(first_backend.aborted);
    assert_eq!(
        compacted
            .scan(None)
            .expect("authority after destination-exists burn")
            .durable
            .generation,
        2
    );
    work_directory.assert_empty();

    let burned_compaction = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("checkpoint and prune destination-exists burned generation");
    assert_eq!(burned_compaction.checkpoint_generation, 2);
    assert_eq!(burned_compaction.pruned_nonce_records, 1);
    assert_eq!(burned_compaction.preserved_nonce_records, 1);
    assert!(!journal_directory.0.join(linux_nonce_journal_name(2)).exists());
    assert!(journal_directory.0.join(linux_nonce_journal_name(1)).exists());

    let mut retry_sources = original;
    let mut retry_backend =
        RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    let retry = stage_and_publish_compacted_source_bound_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut retry_backend,
        &mut retry_sources,
        source_set_id,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0xa3; 16],
        },
    )
    .expect("retry compacted publication after destination-exists burn");
    let EncryptedTreeRestartPublicationOutcome::PublishedAndDurable(durable) = retry else {
        panic!("retry after destination-exists burn must publish durably");
    };
    assert_eq!(durable.durable.continuation.crashed_generation, 1);
    assert_eq!(durable.durable.continuation.fresh_generation, 3);
    assert_eq!(retry_backend.destination.as_deref(), Some(baseline.as_slice()));
    assert_eq!(
        compacted
            .scan(None)
            .expect("authority after destination-exists retry")
            .durable
            .generation,
        3
    );
    work_directory.assert_empty();
}
