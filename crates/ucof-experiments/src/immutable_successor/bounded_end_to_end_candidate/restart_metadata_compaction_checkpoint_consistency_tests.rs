#[test]
fn newer_checkpoint_cannot_mask_older_checkpoint_below_surviving_record() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-masked-older-floor",
        &[5, 7, 3],
    );
    let journal = open_journal(&directory.0, &key, prefix);

    let older = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 2,
        next_unreserved: Some(4),
    };
    let newer = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 3,
        next_unreserved: Some(15),
    };
    persist_nonce_compaction_checkpoint(
        &journal,
        older,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist authenticated older checkpoint below generation-one record");
    persist_nonce_compaction_checkpoint(
        &journal,
        newer,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist valid newer checkpoint");

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("newer checkpoint must not mask older checkpoint below surviving record");
    assert!(error.contains("nonce compaction checkpoint rollback"));
}

#[test]
fn newer_checkpoint_cannot_mask_older_same_generation_record_mismatch() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-masked-same-generation-mismatch",
        &[5, 7, 3],
    );
    let journal = open_journal(&directory.0, &key, prefix);

    let older = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 2,
        next_unreserved: Some(13),
    };
    let newer = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 3,
        next_unreserved: Some(15),
    };
    persist_nonce_compaction_checkpoint(
        &journal,
        older,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist authenticated older same-generation mismatch");
    persist_nonce_compaction_checkpoint(
        &journal,
        newer,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist valid newer checkpoint");

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("newer checkpoint must not mask older same-generation mismatch");
    assert!(error.contains("nonce compaction checkpoint generation mismatch"));
}

fn exhausted_nonce_compaction_fixture(
    label: &str,
) -> (super::TestDirectory, [u8; 32], [u8; 4]) {
    let directory = private_directory(label);
    let key = [0xc7; 32];
    let prefix = [0x5d; 4];
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 16,
            max_generations: 16,
            max_journal_bytes: 16
                * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"),
            max_lease_size: u64::MAX,
        },
    )
    .expect("open exhaustion fixture journal");
    let mut authority = journal
        .recover_authority(None)
        .expect("initial exhaustion fixture authority");
    let huge = journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x61; 16],
            u64::MAX,
            JournalCommitCut::Complete,
        )
        .expect("commit through penultimate nonce counter");
    assert_eq!(huge.journal_generation, 1);
    assert_eq!(huge.lease.first, 0);
    assert_eq!(huge.lease.last, u64::MAX - 1);
    drop(huge);
    let final_counter = journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x62; 16],
            1,
            JournalCommitCut::Complete,
        )
        .expect("commit final nonce counter");
    assert_eq!(final_counter.journal_generation, 2);
    assert_eq!(final_counter.lease.first, u64::MAX);
    assert_eq!(final_counter.lease.last, u64::MAX);
    drop(final_counter);
    assert_eq!(authority.durable.generation, 2);
    assert_eq!(authority.durable.next_unreserved, None);
    (directory, key, prefix)
}

#[test]
fn exhausted_nonce_authority_round_trips_through_checkpoint_and_rejects_future_commit() {
    let (directory, key, prefix) =
        exhausted_nonce_compaction_fixture("nonce-compaction-exhausted-roundtrip");
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 16,
            max_generations: 16,
            max_journal_bytes: 16
                * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"),
            max_lease_size: u64::MAX,
        },
    )
    .expect("reopen exhausted journal");
    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact exhausted nonce authority");
    assert_eq!(report.checkpoint_generation, 2);
    assert_eq!(report.pruned_nonce_records, 2);
    let compacted = CompactedNonceJournal::new(&journal);
    let recovered = compacted.scan(None).expect("recover exhausted checkpoint");
    assert_eq!(recovered.durable.generation, 2);
    assert_eq!(recovered.durable.next_unreserved, None);
    assert_eq!(recovered.checkpoint_generation, Some(2));
    let mut authority = compacted
        .recover_authority(None)
        .expect("recover exhausted authority for rejected commit");
    let error = compacted
        .commit_descriptor_session(
            &mut authority,
            key,
            [0x63; 16],
            1,
            JournalCommitCut::Complete,
        )
        .expect_err("exhausted checkpoint must reject future nonce reservation");
    assert!(error.contains("CounterExhausted"));
    assert_eq!(authority.durable.next_unreserved, None);
    assert!(!directory.0.join(linux_nonce_journal_name(3)).exists());
}

#[test]
fn exhausted_checkpoint_cannot_roll_back_to_finite_counter() {
    let (directory, key, prefix) =
        exhausted_nonce_compaction_fixture("nonce-compaction-exhausted-rollback");
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 16,
            max_generations: 16,
            max_journal_bytes: 16
                * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"),
            max_lease_size: u64::MAX,
        },
    )
    .expect("reopen exhausted rollback journal");
    persist_nonce_compaction_checkpoint(
        &journal,
        NonceCompactionCheckpoint {
            key_id: journal.key_id,
            nonce_prefix: journal.nonce_prefix,
            generation: 2,
            next_unreserved: None,
        },
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist exhausted checkpoint");
    persist_nonce_compaction_checkpoint(
        &journal,
        NonceCompactionCheckpoint {
            key_id: journal.key_id,
            nonce_prefix: journal.nonce_prefix,
            generation: 3,
            next_unreserved: Some(u64::MAX),
        },
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist authenticated finite rollback after exhaustion");

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("finite counter after exhausted checkpoint must be rollback");
    assert!(error.contains("nonce compaction checkpoint rollback"));
}

#[test]
fn exhausted_trusted_floor_rejects_finite_checkpoint_authority() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-exhausted-trusted-floor",
        &[5, 7],
    );
    let journal = open_journal(&directory.0, &key, prefix);
    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact finite authority before exhausted floor");
    let error = CompactedNonceJournal::new(&journal)
        .scan(Some(TrustedNonceFloor {
            generation: 2,
            next_unreserved: None,
        }))
        .expect_err("finite current authority must be below exhausted trusted floor");
    assert!(error.contains("compacted nonce below trusted floor"));
}
