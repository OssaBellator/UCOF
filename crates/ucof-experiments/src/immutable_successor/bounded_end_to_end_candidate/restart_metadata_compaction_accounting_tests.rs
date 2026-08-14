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
