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
