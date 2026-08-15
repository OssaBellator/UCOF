fn nonce_compaction_fixture(
    label: &str,
    lease_sizes: &[u64],
) -> (super::TestDirectory, [u8; 32], [u8; 4]) {
    let directory = private_directory(label);
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
    assert_eq!(
        compacted
            .scan(None)
            .expect("scan generation four")
            .durable
            .next_unreserved,
        Some(19)
    );

    let second = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact generation four");
    assert_eq!(second.checkpoint_generation, 4);
    assert_eq!(second.pruned_nonce_records, 1);
    assert_eq!(second.pruned_old_checkpoints, 1);
    assert!(directory.0.join(nonce_compaction_name(4)).exists());
    assert!(!directory.0.join(nonce_compaction_name(3)).exists());
}

#[test]
fn trusted_floor_rejects_external_checkpoint_deletion_rollback() {
    let (directory, key, prefix) = nonce_compaction_fixture("nonce-compaction-floor", &[5, 7]);
    let journal = open_journal(&directory.0, &key, prefix);
    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("initial compaction for trusted floor");
    let compacted = CompactedNonceJournal::new(&journal);
    let recovery = compacted.scan(None).expect("recovery after compaction");
    let floor = TrustedNonceFloor {
        generation: recovery.durable.generation,
        next_unreserved: recovery.durable.next_unreserved,
    };
    std::fs::remove_file(directory.0.join(nonce_compaction_name(2)))
        .expect("delete checkpoint to simulate rollback");
    let error = compacted
        .scan(Some(floor))
        .expect_err("trusted floor must reject deleted checkpoint rollback");
    assert!(error.contains("compacted nonce below trusted floor"));
}

#[test]
fn authenticated_checkpoint_chain_rejects_counter_rollback() {
    let (directory, key, prefix) = nonce_compaction_fixture("nonce-compaction-chain", &[5, 7]);
    let journal = open_journal(&directory.0, &key, prefix);
    let first = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 1,
        next_unreserved: Some(8),
    };
    let second = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 2,
        next_unreserved: Some(7),
    };
    persist_nonce_compaction_checkpoint(
        &journal,
        first,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist first rollback checkpoint");
    persist_nonce_compaction_checkpoint(
        &journal,
        second,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist second rollback checkpoint");
    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("checkpoint counter rollback must fail");
    assert!(error.contains("nonce compaction checkpoint rollback"));
}

#[test]
fn compaction_cuts_never_delete_nonce_prefix_before_checkpoint_authority() {
    for (index, cut) in [
        RestartMetadataCompactionCut::AfterCheckpointFileSyncBeforeDirectorySync,
        RestartMetadataCompactionCut::AfterCheckpointDirectorySyncBeforePrune,
    ]
    .into_iter()
    .enumerate()
    {
        let (directory, key, prefix) = nonce_compaction_fixture(
            &format!("nonce-compaction-cut-{index}"),
            &[5, 7],
        );
        let journal = open_journal(&directory.0, &key, prefix);
        let result = compact_restart_metadata(&journal, None, cut);
        match cut {
            RestartMetadataCompactionCut::AfterCheckpointFileSyncBeforeDirectorySync => {
                assert!(result.is_err());
            }
            RestartMetadataCompactionCut::AfterCheckpointDirectorySyncBeforePrune => {
                let report = result.expect("directory-synced checkpoint cut");
                assert_eq!(report.pruned_nonce_records, 0);
            }
            _ => unreachable!(),
        }
        assert!(directory.0.join(linux_nonce_journal_name(1)).exists());
        assert!(directory.0.join(linux_nonce_journal_name(2)).exists());
    }
}

#[test]
fn retry_from_file_synced_checkpoint_resyncs_directory_before_pruning() {
    let (directory, key, prefix) = nonce_compaction_fixture("nonce-compaction-retry", &[5, 7]);
    let journal = open_journal(&directory.0, &key, prefix);
    compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::AfterCheckpointFileSyncBeforeDirectorySync,
    )
    .expect_err("first compaction cut before directory sync");
    assert!(directory.0.join(nonce_compaction_name(2)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(2)).exists());

    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("retry compaction after file-synced checkpoint");
    assert_eq!(report.checkpoint_generation, 2);
    assert_eq!(report.pruned_nonce_records, 2);
    assert_eq!(CompactedNonceJournal::new(&journal).scan(None).unwrap().durable.generation, 2);
}

#[test]
fn terminal_retirement_compaction_reclaims_pair_and_obsolete_source_binding() {
    let source_set_id = [0x61; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage("nonce-compaction-terminal", 7, source_set_id);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let work_directory = super::TestDirectory::new("nonce-compaction-terminal-work");
    let mut sources: Vec<_> = (1..=7).rev().map(super::TinySource::new).collect();
    let mut backend =
        RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    let outcome = stage_and_publish_source_bound_encrypted_tree_restart(
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
            fresh_operation_id: [0x62; 16],
        },
    )
    .expect("publish source-bound restart for compaction");
    let EncryptedTreeRestartPublicationOutcome::PublishedAndDurable(durable) = outcome else {
        panic!("compaction fixture requires durable publication");
    };
    let prepared = prepare_encrypted_restart_retirement(
        &journal,
        &stage_directory.0,
        &durable.durable,
        restart_limits,
    )
    .expect("prepare compaction retirement");
    assert_eq!(prepared.state, EncryptedRetirementState::Prepared);
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &stage_directory.0,
            1,
            2,
            restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("terminal compaction retirement"),
        EncryptedRetirementOutcome::Terminal
    );

    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact terminal restart metadata");
    assert_eq!(report.checkpoint_generation, 2);
    assert_eq!(report.pruned_nonce_records, 2);
    assert_eq!(report.pruned_retirement_records, 2);
    assert_eq!(report.pruned_source_set_records, 1);
    assert_eq!(report.preserved_nonce_records, 0);
    assert_eq!(report.preserved_prepared_retirements, 0);
    assert_eq!(report.preserved_source_set_records, 0);
    assert!(load_restart_source_set_authority(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load reclaimed source-set")
    .is_none());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Prepared,
    )
    .expect("load reclaimed prepared")
    .is_none());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Terminal,
    )
    .expect("load reclaimed terminal")
    .is_none());
    assert_eq!(directory_entry_count(&stage_directory.0), 0);
    work_directory.assert_empty();
}

#[test]
fn outstanding_prepared_allows_fresh_nonce_record_compaction_before_terminal() {
    let source_set_id = [0x71; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage("nonce-compaction-prepared", 7, source_set_id);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let work_directory = super::TestDirectory::new("nonce-compaction-prepared-work");
    let mut sources: Vec<_> = (1..=7).rev().map(super::TinySource::new).collect();
    let mut backend =
        RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    let outcome = stage_and_publish_source_bound_encrypted_tree_restart(
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
            fresh_operation_id: [0x72; 16],
        },
    )
    .expect("publish prepared compaction restart");
    let EncryptedTreeRestartPublicationOutcome::PublishedAndDurable(durable) = outcome else {
        panic!("prepared compaction fixture requires durable publication");
    };
    prepare_encrypted_restart_retirement(
        &journal,
        &stage_directory.0,
        &durable.durable,
        restart_limits,
    )
    .expect("prepare restart retirement before compaction");

    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact with outstanding Prepared retirement");
    assert_eq!(report.checkpoint_generation, 2);
    assert_eq!(report.pruned_nonce_records, 1);
    assert_eq!(report.preserved_nonce_records, 1);
    assert_eq!(report.pruned_retirement_records, 0);
    assert_eq!(report.preserved_prepared_retirements, 1);
    assert_eq!(report.preserved_source_set_records, 1);
    assert!(journal_directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(!journal_directory.0.join(linux_nonce_journal_name(2)).exists());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Prepared,
    )
    .expect("load preserved Prepared retirement")
    .is_some());
    assert!(load_restart_source_set_authority(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load preserved source-set")
    .is_some());
    work_directory.assert_empty();
}
