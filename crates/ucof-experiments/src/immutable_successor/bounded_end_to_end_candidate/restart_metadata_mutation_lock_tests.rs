#[test]
fn mutation_lock_blocks_compaction_and_nonce_commit_until_release() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("restart-metadata-mutation-lock", &[5, 7]);
    let lock_owner = open_journal(&directory.0, &key, prefix);
    let contender = open_journal(&directory.0, &key, prefix);
    let mut authority = contender
        .recover_authority(None)
        .expect("recover contender authority before lock");
    assert_eq!(authority.durable.generation, 2);

    let mutation = acquire_restart_metadata_mutation_lock(&lock_owner)
        .expect("acquire restart metadata mutation lock");

    let commit_error = contender
        .commit_descriptor_session(
            &mut authority,
            key,
            [0xa7; 16],
            3,
            JournalCommitCut::Complete,
        )
        .expect_err("concurrent nonce commit must fail closed");
    assert_eq!(commit_error, LinuxNonceJournalError::MutationLockBusy);
    assert!(!directory.0.join(linux_nonce_journal_name(3)).exists());

    let compaction_error = compact_restart_metadata(
        &contender,
        None,
        RestartMetadataCompactionCut::Complete,
    )
    .expect_err("concurrent compaction must fail closed");
    assert!(compaction_error.contains("restart metadata mutation lock is busy"));
    assert!(!directory.0.join(nonce_compaction_name(2)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(2)).exists());

    drop(mutation);

    let session = contender
        .commit_descriptor_session(
            &mut authority,
            key,
            [0xa8; 16],
            3,
            JournalCommitCut::Complete,
        )
        .expect("nonce commit succeeds after mutation lock release");
    assert_eq!(session.journal_generation, 3);
    assert_eq!(session.lease.first, 12);
    assert_eq!(session.lease.last, 14);

    let report = compact_restart_metadata(
        &contender,
        None,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("compaction succeeds after mutation lock release");
    assert_eq!(report.checkpoint_generation, 3);
    assert!(directory.0.join(nonce_compaction_name(3)).exists());
}
