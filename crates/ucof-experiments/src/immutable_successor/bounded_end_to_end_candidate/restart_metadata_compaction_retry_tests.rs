#[test]
fn retry_from_file_synced_checkpoint_resyncs_directory_before_pruning() {
    let (directory, key, prefix) =
        nonce_compaction_fixture("nonce-compaction-file-cut-retry", &[5, 7]);
    let journal = open_journal(&directory.0, &key, prefix);
    let error = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::AfterCheckpointFileSyncBeforeDirectorySync,
    )
    .expect_err("first compaction stops before checkpoint directory sync");
    assert!(error.contains("after checkpoint file sync"));
    assert!(directory.0.join(nonce_compaction_name(2)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(2)).exists());

    let report = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("retry existing checkpoint through directory sync and prune");
    assert_eq!(report.checkpoint_generation, 2);
    assert_eq!(report.pruned_nonce_records, 2);
    assert!(!directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(!directory.0.join(linux_nonce_journal_name(2)).exists());
    let recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("recover after checkpoint retry");
    assert_eq!(recovery.durable.generation, 2);
    assert_eq!(recovery.durable.next_unreserved, Some(12));
}
