#[test]
fn repeated_compaction_preserves_generation_and_nonce_monotonicity() {
    let directory = super::TestDirectory::new("nonce-compaction-monotonicity");
    let key = [0x91; 32];
    let prefix = [0x92; 4];
    let journal = open_journal(&directory.0, &key, prefix);
    let compacted = CompactedNonceJournal::new(&journal);
    let mut authority = DescriptorNonceAuthority::initial();
    let mut expected_next = 0u64;

    for generation in 1u64..=32 {
        let lease_size = (generation % 7) + 1;
        let operation_id = [
            u8::try_from(generation).expect("campaign generation fits operation byte");
            16
        ];
        let session = compacted
            .commit_descriptor_session(
                &mut authority,
                key,
                operation_id,
                lease_size,
                JournalCommitCut::Complete,
            )
            .expect("commit campaign nonce generation");
        assert_eq!(session.journal_generation, generation);
        assert_eq!(session.lease.first, expected_next);
        let expected_last = expected_next
            .checked_add(lease_size - 1)
            .expect("campaign lease last");
        assert_eq!(session.lease.last, expected_last);
        expected_next = expected_last
            .checked_add(1)
            .expect("campaign next nonce");

        let recovery = compacted.scan(None).expect("scan campaign generation");
        assert_eq!(recovery.durable.generation, generation);
        assert_eq!(recovery.durable.next_unreserved, Some(expected_next));

        if generation % 3 == 0 {
            let report = compact_restart_metadata(
                &journal,
                None,
                RestartMetadataCompactionCut::Complete,
            )
            .expect("compact campaign generation");
            assert_eq!(report.checkpoint_generation, generation);
            assert_eq!(report.preserved_nonce_records, 0);
            let after = compacted.scan(None).expect("scan campaign checkpoint");
            assert_eq!(after.durable.generation, generation);
            assert_eq!(after.durable.next_unreserved, Some(expected_next));
            assert_eq!(after.checkpoint_generation, Some(generation));
            assert_eq!(after.journal_records, 0);
        }
    }

    let final_recovery = compacted.scan(None).expect("final campaign recovery");
    assert_eq!(final_recovery.durable.generation, 32);
    assert_eq!(final_recovery.durable.next_unreserved, Some(expected_next));
    let final_plan = compaction_storage_plan(&journal).expect("final campaign compaction plan");
    assert!(final_plan.required_before_prune >= final_plan.existing_persistent_bytes);
}
