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
        next_unreserved: Some(11),
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
