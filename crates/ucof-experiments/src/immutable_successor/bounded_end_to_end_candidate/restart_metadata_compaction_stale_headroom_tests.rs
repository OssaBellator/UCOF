#[test]
fn stale_checkpoint_cannot_authorize_second_checkpoint_from_transient_over_cap_state() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-stale-headroom",
        &[5, 7],
    );
    let original = open_journal(&directory.0, &key, prefix);
    persist_nonce_compaction_checkpoint(
        &original,
        NonceCompactionCheckpoint {
            key_id: original.key_id,
            nonce_prefix: original.nonce_prefix,
            generation: 1,
            next_unreserved: Some(5),
        },
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist stale generation-one checkpoint beside two ordinary records");
    assert_eq!(directory_entry_count(&directory.0), 3);
    assert!(directory.0.join(nonce_compaction_name(1)).exists());
    assert!(!directory.0.join(nonce_compaction_name(2)).exists());

    let constrained = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 2,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open stale-checkpoint transient-over-cap journal");

    let error = CompactedNonceJournal::new(&constrained)
        .scan(None)
        .expect_err("stale checkpoint must not justify transient over-cap state");
    assert!(error.contains("compacted nonce stale checkpoint headroom"));

    let compaction_error = compact_restart_metadata(
        &constrained,
        None,
        RestartMetadataCompactionCut::Complete,
    )
    .expect_err("compaction must not create a second checkpoint from max+1 stale state");
    assert!(compaction_error.contains("compacted nonce stale checkpoint headroom"));
    assert_eq!(directory_entry_count(&directory.0), 3);
    assert!(directory.0.join(nonce_compaction_name(1)).exists());
    assert!(!directory.0.join(nonce_compaction_name(2)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(2)).exists());
}

#[test]
fn current_checkpoint_still_authorizes_single_transient_entry_over_cap() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-current-headroom",
        &[5, 7],
    );
    let original = open_journal(&directory.0, &key, prefix);
    persist_nonce_compaction_checkpoint(
        &original,
        NonceCompactionCheckpoint {
            key_id: original.key_id,
            nonce_prefix: original.nonce_prefix,
            generation: 2,
            next_unreserved: Some(12),
        },
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist current generation-two checkpoint beside two ordinary records");
    assert_eq!(directory_entry_count(&directory.0), 3);

    let constrained = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 2,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open current-checkpoint transient-over-cap journal");
    let recovery = CompactedNonceJournal::new(&constrained)
        .scan(None)
        .expect("current checkpoint may authorize the one-entry retry transient");
    assert_eq!(recovery.durable.generation, 2);
    assert_eq!(recovery.durable.next_unreserved, Some(12));
    assert_eq!(recovery.checkpoint_generation, Some(2));
}

#[test]
fn maximum_directory_entry_limit_does_not_overflow_compacted_scan_ceiling() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-usize-max-headroom",
        &[5],
    );
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: usize::MAX,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open usize-max directory-limit journal");
    let recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("saturating scan headroom must not overflow usize-max limit");
    assert_eq!(recovery.durable.generation, 1);
    assert_eq!(recovery.durable.next_unreserved, Some(5));
}
