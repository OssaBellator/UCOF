#[test]
fn compacted_publication_one_journal_slot_short_rejects_before_nonce_or_backend_side_effects() {
    const OBJECTS: u64 = 11;
    let source_set_id = [0xb1; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(
            "compacted-publication-journal-short",
            OBJECTS,
            source_set_id,
        );
    let initial = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    compact_restart_metadata(&initial, None, RestartMetadataCompactionCut::Complete)
        .expect("checkpoint live restart before constrained publication");
    drop(initial);
    assert_eq!(directory_entry_count(&journal_directory.0), 4);

    let journal = LinuxDurableNonceJournal::open(
        &journal_directory.0,
        &aes_key,
        nonce_prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 5,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("reopen one-slot-short compacted journal");
    let work_directory = super::TestDirectory::new("compacted-publication-journal-short-work");
    let mut sources: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut backend =
        RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);

    let error = stage_and_publish_compacted_source_bound_encrypted_tree_restart(
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
            fresh_operation_id: [0xb2; 16],
        },
    )
    .expect_err("one free journal slot cannot cover fresh nonce plus Prepared cleanup");
    assert!(error.contains("compacted publication retirement directory headroom"));
    assert!(!journal_directory.0.join(linux_nonce_journal_name(2)).exists());
    assert_eq!(
        CompactedNonceJournal::new(&journal)
            .scan(None)
            .expect("authority unchanged after headroom rejection")
            .durable
            .generation,
        1
    );
    assert!(backend.private.is_empty());
    assert!(backend.destination.is_none());
    assert!(!backend.aborted);
    work_directory.assert_empty();
}

#[test]
fn compacted_publication_exact_two_journal_slots_reaches_terminal_and_reclaims() {
    const OBJECTS: u64 = 11;
    let source_set_id = [0xb3; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(
            "compacted-publication-journal-exact",
            OBJECTS,
            source_set_id,
        );
    let initial = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    compact_restart_metadata(&initial, None, RestartMetadataCompactionCut::Complete)
        .expect("checkpoint live restart before exact publication");
    drop(initial);
    assert_eq!(directory_entry_count(&journal_directory.0), 4);

    let journal = LinuxDurableNonceJournal::open(
        &journal_directory.0,
        &aes_key,
        nonce_prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 6,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("reopen exact-two-slot compacted journal");
    let work_directory = super::TestDirectory::new("compacted-publication-journal-exact-work");
    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        super::ImmutableLimits::default(),
    )
    .expect("exact-headroom baseline");
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
            fresh_operation_id: [0xb4; 16],
        },
    )
    .expect("two free journal slots permit durable compacted publication");
    let EncryptedTreeRestartPublicationOutcome::PublishedAndDurable(durable) = outcome else {
        panic!("exact journal headroom must publish durably");
    };
    assert_eq!(durable.durable.continuation.fresh_generation, 2);
    assert_eq!(backend.destination.as_deref(), Some(baseline.as_slice()));
    assert_eq!(directory_entry_count(&journal_directory.0), 5);

    let prepared = prepare_compacted_encrypted_restart_retirement(
        &journal,
        &stage_directory.0,
        &durable.durable,
        restart_limits,
    )
    .expect("exact journal headroom permits Prepared retirement");
    assert_eq!(prepared.state, EncryptedRetirementState::Prepared);
    assert_eq!(directory_entry_count(&journal_directory.0), 6);

    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &stage_directory.0,
            1,
            2,
            restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("exact journal headroom reaches Terminal after manifest unlink"),
        EncryptedRetirementOutcome::Terminal
    );
    assert_eq!(directory_entry_count(&stage_directory.0), 0);
    assert_eq!(directory_entry_count(&journal_directory.0), 6);

    let final_compaction = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("current checkpoint may use one transient entry then reclaim terminal lineage");
    assert_eq!(final_compaction.checkpoint_generation, 2);
    assert_eq!(final_compaction.pruned_nonce_records, 2);
    assert_eq!(final_compaction.pruned_retirement_records, 2);
    assert_eq!(final_compaction.pruned_source_set_records, 1);
    assert_eq!(final_compaction.pruned_old_checkpoints, 1);
    assert_eq!(directory_entry_count(&journal_directory.0), 1);
    let recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("recover exact-headroom final checkpoint");
    assert_eq!(recovery.durable.generation, 2);
    assert_eq!(recovery.checkpoint_generation, Some(2));
    work_directory.assert_empty();
}
