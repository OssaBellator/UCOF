#[test]
fn compacted_scan_reports_nonce_and_checkpoint_authenticated_bytes() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-byte-accounting", &[5, 7]);
    let journal = open_journal(&directory.0, &key, prefix);
    let compacted = CompactedNonceJournal::new(&journal);

    let before = compacted.scan(None).expect("scan nonce bytes before compaction");
    assert_eq!(before.journal_records, 2);
    assert_eq!(
        before.bytes_read,
        2 * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width")
    );

    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact byte-accounting fixture");
    let after = compacted.scan(None).expect("scan checkpoint bytes after compaction");
    assert_eq!(after.journal_records, 0);
    assert_eq!(after.checkpoint_generation, Some(2));
    assert_eq!(
        after.bytes_read,
        u64::try_from(NONCE_COMPACTION_BYTES).expect("checkpoint width")
    );
}

#[test]
fn checkpoint_overhead_does_not_consume_legacy_journal_byte_capacity_on_retry() {
    let directory = private_directory("nonce-compaction-journal-cap-retry");
    let key = [0xb3; 32];
    let prefix = [0x49; 4];
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 16,
            max_generations: 2,
            max_journal_bytes: 2
                * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"),
            max_lease_size: 64,
        },
    )
    .expect("open exact-cap journal");
    let mut authority = journal
        .recover_authority(None)
        .expect("initial exact-cap authority");
    journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x21; 16],
            5,
            JournalCommitCut::Complete,
        )
        .expect("first exact-cap generation");
    journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x22; 16],
            7,
            JournalCommitCut::Complete,
        )
        .expect("second exact-cap generation");

    let report = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::AfterCheckpointDirectorySyncBeforePrune,
    )
    .expect("checkpoint exact-cap journal before prune");
    assert_eq!(report.checkpoint_generation, 2);
    assert_eq!(report.pruned_nonce_records, 0);

    let coexistence = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("checkpoint plus full legacy journal must remain recoverable");
    assert_eq!(coexistence.durable.generation, 2);
    assert_eq!(coexistence.durable.next_unreserved, Some(12));
    assert_eq!(coexistence.journal_records, 0);
    assert_eq!(
        coexistence.bytes_read,
        2 * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width")
            + u64::try_from(NONCE_COMPACTION_BYTES).expect("checkpoint width")
    );

    let retried = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("retry exact-cap compaction");
    assert_eq!(retried.pruned_nonce_records, 2);
    let final_recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("recover exact-cap journal after prune");
    assert_eq!(final_recovery.durable.generation, 2);
    assert_eq!(final_recovery.durable.next_unreserved, Some(12));
    assert_eq!(final_recovery.journal_records, 0);
}

#[test]
fn authenticated_checkpoint_gets_exactly_one_transient_directory_entry_at_ceiling() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-directory-ceiling", &[5, 7]);
    assert_eq!(directory_entry_count(&directory.0), 2);
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 2,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open exact-directory-ceiling journal");

    let error = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::AfterCheckpointFileSyncBeforeDirectorySync,
    )
    .expect_err("stop after checkpoint creates the one transient directory entry");
    assert!(error.contains("after checkpoint file sync"));
    assert_eq!(directory_entry_count(&directory.0), 3);
    assert!(directory.0.join(nonce_compaction_name(2)).exists());

    let recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("authenticated checkpoint permits one transient entry over configured cap");
    assert_eq!(recovery.durable.generation, 2);
    assert_eq!(recovery.checkpoint_generation, Some(2));
    let inventory = scan_compacted_persistent_inventory(&journal)
        .expect("quota inventory permits same authenticated checkpoint headroom");
    assert_eq!(inventory.nonce_records, 2);
    assert_eq!(inventory.checkpoint_records, 1);

    let retry = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("retry exact-directory-ceiling checkpoint through prune");
    assert_eq!(retry.checkpoint_generation, 2);
    assert_eq!(retry.pruned_nonce_records, 2);
    assert_eq!(directory_entry_count(&directory.0), 1);
    assert_eq!(
        CompactedNonceJournal::new(&journal)
            .scan(None)
            .expect("scan after directory-ceiling retry")
            .durable
            .generation,
        2
    );
}

#[test]
fn unrelated_extra_directory_entry_does_not_receive_checkpoint_headroom() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-unrelated-directory-extra", &[5, 7]);
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 2,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open unrelated-extra constrained journal");
    std::fs::write(directory.0.join("unrelated-extra"), b"x")
        .expect("create unrelated directory entry");
    assert_eq!(directory_entry_count(&directory.0), 3);

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("unrelated extra entry must not receive checkpoint headroom");
    assert!(error.contains("compacted nonce directory entry limit"));
    let inventory_error = scan_compacted_persistent_inventory(&journal)
        .expect_err("quota inventory must reject unrelated extra entry too");
    assert!(inventory_error.contains("compacted inventory directory entry limit"));
    assert!(!directory.0.join(nonce_compaction_name(2)).exists());
}
