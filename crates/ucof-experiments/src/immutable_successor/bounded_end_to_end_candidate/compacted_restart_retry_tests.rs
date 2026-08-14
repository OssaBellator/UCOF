#[test]
fn compacted_source_bound_restart_retries_after_burned_generation() {
    const OBJECTS: u64 = 11;
    let source_set_id = [0x81; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(
            "compacted-retry-after-burn",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("checkpoint generation one before burned retry");

    let object_count = usize::try_from(OBJECTS).expect("object count");
    let burned_size = u64::try_from(object_count)
        .expect("object nonce count")
        .checked_add(
            consolidated_encrypted_tree_stage_record_count(object_count)
                .expect("tree nonce count"),
        )
        .expect("burned retry lease size");
    let compacted = CompactedNonceJournal::new(&journal);
    let mut authority = compacted
        .recover_authority(None)
        .expect("recover generation one before burn");
    let burned = compacted
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x82; 16],
            burned_size,
            JournalCommitCut::Complete,
        )
        .expect("burn generation two");
    assert_eq!(burned.journal_generation, 2);
    drop(burned);
    assert_eq!(
        compacted
            .scan(None)
            .expect("scan burned generation")
            .durable
            .generation,
        2
    );

    let work_directory = super::TestDirectory::new("compacted-retry-after-burn-work");
    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        super::ImmutableLimits::default(),
    )
    .expect("retry baseline writer");

    let mut sources = original;
    let mut output = Vec::new();
    let evidence = continue_compacted_source_bound_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut output,
        &mut sources,
        source_set_id,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0x83; 16],
        },
    )
    .expect("retry from generation-one stage over burned generation two");
    assert_eq!(output, baseline);
    assert_eq!(evidence.output.output, baseline_report);
    assert_eq!(evidence.crashed_generation, 1);
    assert_eq!(evidence.fresh_generation, 3);
    assert!(evidence.fresh_lease_first > evidence.crashed_lease_last);
    assert_eq!(
        compacted
            .scan(None)
            .expect("scan successful retry generation")
            .durable
            .generation,
        3
    );
    work_directory.assert_empty();
}
