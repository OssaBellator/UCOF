fn exact_capacity_journal(
    label: &str,
    key: &[u8; 32],
    prefix: [u8; 4],
    limits: LinuxNonceJournalLimits,
) -> (super::TestDirectory, LinuxDurableNonceJournal) {
    let directory = private_directory(label);
    let journal = LinuxDurableNonceJournal::open(
        &directory.0,
        key,
        prefix,
        [0x5a; 32],
        limits,
    )
    .expect("open exact-capacity journal");
    (directory, journal)
}

fn commit_capacity_generation_one(
    journal: &LinuxDurableNonceJournal,
    key: [u8; 32],
) -> DescriptorNonceAuthority {
    let mut authority = journal
        .recover_authority(None)
        .expect("recover initial exact-capacity authority");
    let first = journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0xd1; 16],
            5,
            JournalCommitCut::Complete,
        )
        .expect("commit exact-capacity generation one");
    assert_eq!(first.journal_generation, 1);
    drop(first);
    authority
}

#[test]
fn legacy_commit_rejects_exact_generation_capacity_before_creating_next_record() {
    let key = [0xd2; 32];
    let prefix = [0x71; 4];
    let (directory, journal) = exact_capacity_journal(
        "nonce-journal-generation-capacity",
        &key,
        prefix,
        LinuxNonceJournalLimits {
            max_directory_entries: 8,
            max_generations: 1,
            max_journal_bytes: 8
                * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"),
            max_lease_size: 64,
        },
    );
    let mut authority = commit_capacity_generation_one(&journal, key);

    let error = journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0xd3; 16],
            5,
            JournalCommitCut::Complete,
        )
        .err()
        .expect("exact generation capacity must reject before create");
    assert_eq!(
        error,
        LinuxNonceJournalError::Limit("journal generation capacity")
    );
    assert_eq!(authority.durable.generation, 1);
    assert!(!directory.0.join(linux_nonce_journal_name(2)).exists());
    let recovery = journal.scan(None).expect("journal remains recoverable at generation cap");
    assert_eq!(recovery.durable.generation, 1);
    assert_eq!(recovery.generations, 1);
}

#[test]
fn legacy_commit_rejects_exact_journal_byte_capacity_before_creating_next_record() {
    let key = [0xd4; 32];
    let prefix = [0x72; 4];
    let (directory, journal) = exact_capacity_journal(
        "nonce-journal-byte-capacity",
        &key,
        prefix,
        LinuxNonceJournalLimits {
            max_directory_entries: 8,
            max_generations: 8,
            max_journal_bytes: u64::try_from(LINUX_NONCE_JOURNAL_BYTES)
                .expect("journal width"),
            max_lease_size: 64,
        },
    );
    let mut authority = commit_capacity_generation_one(&journal, key);

    let error = journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0xd5; 16],
            5,
            JournalCommitCut::Complete,
        )
        .err()
        .expect("exact journal byte capacity must reject before create");
    assert_eq!(error, LinuxNonceJournalError::Limit("journal byte capacity"));
    assert_eq!(authority.durable.generation, 1);
    assert!(!directory.0.join(linux_nonce_journal_name(2)).exists());
    let recovery = journal.scan(None).expect("journal remains recoverable at byte cap");
    assert_eq!(recovery.durable.generation, 1);
    assert_eq!(
        recovery.bytes_read,
        u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width")
    );
}

#[test]
fn legacy_commit_rejects_exact_directory_capacity_before_creating_next_record() {
    let key = [0xd6; 32];
    let prefix = [0x73; 4];
    let (directory, journal) = exact_capacity_journal(
        "nonce-journal-directory-capacity",
        &key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: 1,
            max_generations: 8,
            max_journal_bytes: 8
                * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"),
            max_lease_size: 64,
        },
    );
    let mut authority = commit_capacity_generation_one(&journal, key);

    let error = journal
        .commit_descriptor_session(
            &mut authority,
            key,
            [0xd7; 16],
            5,
            JournalCommitCut::Complete,
        )
        .err()
        .expect("exact directory capacity must reject before create");
    assert_eq!(
        error,
        LinuxNonceJournalError::Limit("directory entry capacity")
    );
    assert_eq!(authority.durable.generation, 1);
    assert_eq!(directory_entry_count(&directory.0), 1);
    assert!(!directory.0.join(linux_nonce_journal_name(2)).exists());
    assert_eq!(
        journal
            .scan(None)
            .expect("journal remains recoverable at directory cap")
            .durable
            .generation,
        1
    );
}

#[test]
fn compaction_restores_ordinary_generation_capacity_for_future_commit() {
    let key = [0xd8; 32];
    let prefix = [0x74; 4];
    let (directory, journal) = exact_capacity_journal(
        "compacted-generation-capacity",
        &key,
        prefix,
        LinuxNonceJournalLimits {
            max_directory_entries: 8,
            max_generations: 1,
            max_journal_bytes: 8
                * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"),
            max_lease_size: 64,
        },
    );
    let authority = commit_capacity_generation_one(&journal, key);
    let compacted = CompactedNonceJournal::new(&journal);
    let mut compacted_authority = compacted
        .recover_authority(None)
        .expect("recover compacted authority at generation capacity");
    assert_eq!(compacted_authority.durable, authority.durable);

    let error = compacted
        .commit_descriptor_session(
            &mut compacted_authority,
            key,
            [0xd9; 16],
            5,
            JournalCommitCut::Complete,
        )
        .err()
        .expect("compacted path must honor ordinary generation capacity");
    assert!(error.contains("journal generation capacity"));
    assert!(!directory.0.join(linux_nonce_journal_name(2)).exists());

    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compaction reclaims generation-one ordinary record");
    assert_eq!(report.checkpoint_generation, 1);
    assert_eq!(report.pruned_nonce_records, 1);
    assert!(!directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(directory.0.join(nonce_compaction_name(1)).exists());

    let second = compacted
        .commit_descriptor_session(
            &mut compacted_authority,
            key,
            [0xda; 16],
            5,
            JournalCommitCut::Complete,
        )
        .expect("allocation resumes after ordinary generation capacity is reclaimed");
    assert_eq!(second.journal_generation, 2);
    assert_eq!(second.lease.first, 5);
}

#[test]
fn compaction_restores_ordinary_byte_capacity_for_future_commit() {
    let key = [0xdb; 32];
    let prefix = [0x75; 4];
    let (directory, journal) = exact_capacity_journal(
        "compacted-byte-capacity",
        &key,
        prefix,
        LinuxNonceJournalLimits {
            max_directory_entries: 8,
            max_generations: 8,
            max_journal_bytes: u64::try_from(LINUX_NONCE_JOURNAL_BYTES)
                .expect("journal width"),
            max_lease_size: 64,
        },
    );
    commit_capacity_generation_one(&journal, key);
    let compacted = CompactedNonceJournal::new(&journal);
    let mut authority = compacted
        .recover_authority(None)
        .expect("recover compacted authority at byte capacity");

    let error = compacted
        .commit_descriptor_session(
            &mut authority,
            key,
            [0xdc; 16],
            5,
            JournalCommitCut::Complete,
        )
        .err()
        .expect("compacted path must honor ordinary journal byte capacity");
    assert!(error.contains("journal byte capacity"));
    assert!(!directory.0.join(linux_nonce_journal_name(2)).exists());

    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compaction reclaims ordinary bytes");
    let second = compacted
        .commit_descriptor_session(
            &mut authority,
            key,
            [0xdd; 16],
            5,
            JournalCommitCut::Complete,
        )
        .expect("allocation resumes after ordinary byte capacity is reclaimed");
    assert_eq!(second.journal_generation, 2);
    assert_eq!(second.lease.first, 5);
}
