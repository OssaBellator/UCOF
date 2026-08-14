fn nonce_compaction_fixture(
    label: &str,
    lease_sizes: &[u64],
) -> (super::TestDirectory, [u8; 32], [u8; 4]) {
    let directory = super::TestDirectory::new(label);
    let key = [0x2d; 32];
    let prefix = [0x3e; 4];
    let journal = open_journal(&directory.0, &key, prefix);
    let mut authority = DescriptorNonceAuthority::initial();
    for (index, lease_size) in lease_sizes.iter().copied().enumerate() {
        let operation = [
            u8::try_from(index + 1).expect("compaction operation index");
            16
        ];
        journal
            .commit_descriptor_session(
                &mut authority,
                key,
                operation,
                lease_size,
                JournalCommitCut::Complete,
            )
            .expect("commit compaction fixture generation");
    }
    (directory, key, prefix)
}

#[test]
fn nonce_checkpoint_replaces_prefix_and_future_generation_remains_monotonic() {
    let (directory, key, prefix) = nonce_compaction_fixture("nonce-compaction-prefix", &[5, 7, 3]);
    let journal = open_journal(&directory.0, &key, prefix);
    let compacted = CompactedNonceJournal::new(&journal);
    let before = compacted.scan(None).expect("scan before compaction");
    assert_eq!(before.durable.generation, 3);
    assert_eq!(before.durable.next_unreserved, Some(15));
    assert_eq!(before.checkpoint_generation, None);
    assert_eq!(before.journal_records, 3);

    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact nonce prefix");
    assert_eq!(report.checkpoint_generation, 3);
    assert_eq!(report.pruned_nonce_records, 3);
    assert_eq!(report.preserved_nonce_records, 0);
    assert!(directory.0.join(nonce_compaction_name(3)).exists());
    for generation in 1..=3 {
        assert!(!directory.0.join(linux_nonce_journal_name(generation)).exists());
    }

    let after = compacted.scan(None).expect("scan compacted checkpoint");
    assert_eq!(after.durable.generation, 3);
    assert_eq!(after.durable.next_unreserved, Some(15));
    assert_eq!(after.checkpoint_generation, Some(3));
    assert_eq!(after.journal_records, 0);

    let mut authority = compacted
        .recover_authority(None)
        .expect("recover compacted nonce authority");
    let session = compacted
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x44; 16],
            4,
            JournalCommitCut::Complete,
        )
        .expect("commit generation after compaction");
    assert_eq!(session.journal_generation, 4);
    assert_eq!(session.lease.first, 15);
    assert_eq!(session.lease.last, 18);
    assert_eq!(compacted.scan(None).expect("scan generation four").durable.next_unreserved, Some(19));

    let second = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact generation four");
    assert_eq!(second.checkpoint_generation, 4);
    assert_eq!(second.pruned_nonce_records, 1);
    assert_eq!(second.pruned_old_checkpoints, 1);
    assert!(!directory.0.join(nonce_compaction_name(3)).exists());
    assert!(directory.0.join(nonce_compaction_name(4)).exists());
    let final_recovery = compacted.scan(None).expect("scan second checkpoint");
    assert_eq!(final_recovery.durable.generation, 4);
    assert_eq!(final_recovery.durable.next_unreserved, Some(19));
    assert_eq!(final_recovery.journal_records, 0);
}

#[test]
fn compaction_cuts_never_delete_nonce_prefix_before_checkpoint_authority() {
    let (file_cut_directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-file-cut", &[5, 7]);
    let file_cut_journal = open_journal(&file_cut_directory.0, &key, prefix);
    let error = compact_restart_metadata(
        &file_cut_journal,
        None,
        RestartMetadataCompactionCut::AfterCheckpointFileSyncBeforeDirectorySync,
    )
    .expect_err("file-sync cut must report injection");
    assert!(error.contains("after checkpoint file sync"));
    assert!(file_cut_directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(file_cut_directory.0.join(linux_nonce_journal_name(2)).exists());
    assert_eq!(
        file_cut_journal.scan(None).expect("legacy scan after file cut").durable.generation,
        2
    );
    assert_eq!(
        CompactedNonceJournal::new(&file_cut_journal)
            .scan(None)
            .expect("compacted scan after file cut")
            .durable
            .generation,
        2
    );

    let (authority_cut_directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-authority-cut", &[5, 7]);
    let authority_cut_journal = open_journal(&authority_cut_directory.0, &key, prefix);
    let report = compact_restart_metadata(
        &authority_cut_journal,
        None,
        RestartMetadataCompactionCut::AfterCheckpointDirectorySyncBeforePrune,
    )
    .expect("directory-synced checkpoint cut");
    assert_eq!(report.pruned_nonce_records, 0);
    assert!(authority_cut_directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(authority_cut_directory.0.join(linux_nonce_journal_name(2)).exists());
    assert_eq!(
        CompactedNonceJournal::new(&authority_cut_journal)
            .scan(None)
            .expect("scan directory-synced checkpoint")
            .durable
            .generation,
        2
    );

    let (prune_cut_directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-prune-cut", &[5, 7]);
    let prune_cut_journal = open_journal(&prune_cut_directory.0, &key, prefix);
    let report = compact_restart_metadata(
        &prune_cut_journal,
        None,
        RestartMetadataCompactionCut::AfterPruneBeforeDirectorySync,
    )
    .expect("post-prune cut");
    assert_eq!(report.pruned_nonce_records, 2);
    assert_eq!(
        CompactedNonceJournal::new(&prune_cut_journal)
            .scan(None)
            .expect("scan post-prune checkpoint")
            .durable
            .generation,
        2
    );
}

#[test]
fn terminal_retirement_compaction_reclaims_pair_and_obsolete_source_binding() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0x51; 32];
    let mut fixture = encrypted_retirement_fixture("terminal-metadata-compaction", OBJECTS);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let manifest = load_encrypted_stage_manifest(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load compaction source manifest")
    .expect("compaction source manifest");
    persist_restart_source_set_authority(
        &journal,
        manifest,
        source_set_id,
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect("persist compaction source-set authority");
    prepare_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        &fixture.publication,
        fixture.restart_limits,
    )
    .expect("prepare terminal compaction retirement");
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("complete retirement before compaction"),
        EncryptedRetirementOutcome::Terminal
    );

    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact terminal metadata");
    assert_eq!(report.checkpoint_generation, 2);
    assert_eq!(report.pruned_nonce_records, 2);
    assert_eq!(report.pruned_retirement_records, 2);
    assert_eq!(report.pruned_source_set_records, 1);
    assert_eq!(report.preserved_prepared_retirements, 0);
    assert_eq!(report.preserved_source_set_records, 0);
    assert_eq!(
        CompactedNonceJournal::new(&journal)
            .scan(None)
            .expect("scan terminal compaction")
            .durable
            .generation,
        2
    );
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Prepared,
    )
    .expect("load compacted prepared")
    .is_none());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Terminal,
    )
    .expect("load compacted terminal")
    .is_none());
    assert!(load_restart_source_set_authority(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load compacted source-set")
    .is_none());
}

#[test]
fn outstanding_prepared_and_live_source_authority_survive_until_terminal() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0x52; 32];
    let mut fixture = encrypted_retirement_fixture("prepared-metadata-compaction", OBJECTS);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let manifest = load_encrypted_stage_manifest(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load prepared compaction manifest")
    .expect("prepared compaction manifest");
    persist_restart_source_set_authority(
        &journal,
        manifest,
        source_set_id,
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect("persist prepared compaction source-set");
    prepare_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        &fixture.publication,
        fixture.restart_limits,
    )
    .expect("prepare outstanding retirement");

    let first = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact with outstanding prepared");
    assert_eq!(first.checkpoint_generation, 2);
    assert_eq!(first.preserved_prepared_retirements, 1);
    assert_eq!(first.preserved_source_set_records, 1);
    assert_eq!(first.preserved_nonce_records, 1);
    assert_eq!(first.pruned_nonce_records, 1);
    assert_eq!(first.pruned_retirement_records, 0);
    assert_eq!(first.pruned_source_set_records, 0);
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Prepared,
    )
    .expect("load preserved prepared")
    .is_some());
    assert!(load_restart_source_set_authority(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load preserved source-set")
    .is_some());
    assert!(fixture
        .journal_directory
        .0
        .join(linux_nonce_journal_name(1))
        .exists());
    assert!(!fixture
        .journal_directory
        .0
        .join(linux_nonce_journal_name(2))
        .exists());

    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("complete preserved retirement"),
        EncryptedRetirementOutcome::Terminal
    );
    let second = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact terminalized prepared metadata");
    assert_eq!(second.pruned_nonce_records, 1);
    assert_eq!(second.pruned_retirement_records, 2);
    assert_eq!(second.pruned_source_set_records, 1);
    assert_eq!(second.preserved_prepared_retirements, 0);
    assert_eq!(second.preserved_source_set_records, 0);
}

#[test]
fn live_source_bound_restart_remains_usable_after_checkpoint_compaction() {
    const OBJECTS: u64 = 11;
    let source_set_id = [0x53; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(
            "live-source-bound-compaction",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact live source-bound restart");
    assert_eq!(report.checkpoint_generation, 1);
    assert_eq!(report.pruned_nonce_records, 0);
    assert_eq!(report.preserved_nonce_records, 1);
    assert_eq!(report.preserved_source_set_records, 1);

    let work_directory = super::TestDirectory::new("live-source-bound-compaction-work");
    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        super::ImmutableLimits::default(),
    )
    .expect("compacted source baseline");
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
            fresh_operation_id: [0x63; 16],
        },
    )
    .expect("continue compacted source-bound restart");
    assert_eq!(output, baseline);
    assert_eq!(evidence.output.output, baseline_report);
    assert_eq!(evidence.fresh_generation, 2);
    let recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("scan after compacted source restart");
    assert_eq!(recovery.durable.generation, 2);
    work_directory.assert_empty();
}

#[test]
fn trusted_floor_rejects_external_checkpoint_deletion_rollback() {
    let (directory, key, prefix) = nonce_compaction_fixture("nonce-compaction-floor", &[5, 7]);
    let journal = open_journal(&directory.0, &key, prefix);
    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact before floor rollback test");
    let checkpoint = nonce_compaction_name(2);
    std::fs::remove_file(directory.0.join(&checkpoint)).expect("delete checkpoint externally");
    let no_floor = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("scan external deletion without floor");
    assert_eq!(no_floor.durable, DurableNonceState::initial());
    let error = CompactedNonceJournal::new(&journal)
        .scan(Some(TrustedNonceFloor {
            generation: 2,
            next_unreserved: Some(12),
        }))
        .expect_err("trusted floor must reject checkpoint deletion rollback");
    assert!(error.contains("below trusted floor"));
}
