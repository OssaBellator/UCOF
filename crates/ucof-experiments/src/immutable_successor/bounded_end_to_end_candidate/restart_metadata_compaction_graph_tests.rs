#[test]
fn orphan_source_set_authority_is_not_compaction_authority() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xa1; 32];
    let (journal_directory, _stage_directory, aes_key, nonce_prefix, _restart_limits, _) =
        prepared_source_bound_restart_stage(
            "orphan-source-set-compaction",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let manifest_name = encrypted_stage_manifest_name(
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    );
    std::fs::remove_file(journal_directory.0.join(manifest_name))
        .expect("remove live manifest to orphan source-set authority");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("orphan source-set authority must fail closed");
    assert!(error.contains("source-set authority without live restart or cleanup"));
    assert!(!journal_directory.0.join(nonce_compaction_name(1)).exists());
    assert!(journal_directory.0.join(linux_nonce_journal_name(1)).exists());
}

#[test]
fn authenticated_checkpoint_chain_rejects_counter_rollback() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-checkpoint-rollback",
        &[5, 7, 3],
    );
    let journal = open_journal(&directory.0, &key, prefix);
    let checkpoint_two = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 2,
        next_unreserved: Some(12),
    };
    let checkpoint_three = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 3,
        next_unreserved: Some(11),
    };
    persist_nonce_compaction_checkpoint(
        &journal,
        checkpoint_two,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist valid generation-two checkpoint");
    persist_nonce_compaction_checkpoint(
        &journal,
        checkpoint_three,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist authenticated rollback checkpoint");

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("newer authenticated checkpoint must not roll nonce floor backward");
    assert!(error.contains("nonce compaction checkpoint rollback"));
}
