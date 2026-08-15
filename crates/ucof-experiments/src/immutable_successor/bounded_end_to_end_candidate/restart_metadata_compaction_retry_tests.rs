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

#[test]
fn retry_after_terminal_source_set_prune_before_retirement_prune_completes() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xb1; 32];
    let fixture = encrypted_retirement_fixture("terminal-source-prune-retry", OBJECTS);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let manifest = load_encrypted_stage_manifest(&journal, 1, role)
        .expect("load terminal-source retry manifest")
        .expect("terminal-source retry manifest");
    let source_set = RestartSourceSetAuthority {
        role,
        key_id: manifest.key_id,
        nonce_prefix: manifest.nonce_prefix,
        operation_id: manifest.operation_id,
        generation: manifest.generation,
        stage_identity: manifest.identity(),
        source_set_id,
        object_count: OBJECTS,
    };
    let source_set_name = restart_source_set_authority_name(1, role);
    let source_set_sealed = seal_restart_source_set_authority(&journal, source_set)
        .expect("seal terminal-source retry authority");
    let source_set_path = linux_nonce_procfd_child(&journal.directory, &source_set_name)
        .expect("terminal-source retry source-set path");
    let mut source_set_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(source_set_path)
        .expect("create terminal-source retry authority");
    source_set_file
        .write_all(&source_set_sealed)
        .expect("write terminal-source retry authority");
    source_set_file
        .flush()
        .expect("flush terminal-source retry authority");
    source_set_file
        .sync_all()
        .expect("sync terminal-source retry authority");
    journal
        .directory
        .sync_all()
        .expect("sync terminal-source retry authority directory");

    prepare_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect("prepare terminal-source retry retirement");
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("terminalize terminal-source retry retirement"),
        EncryptedRetirementOutcome::Terminal
    );

    let cut_report = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::AfterSourceSetPruneBeforeRetirementPrune,
    )
    .expect("cut after terminal source-set prune");
    assert_eq!(cut_report.checkpoint_generation, 2);
    assert_eq!(cut_report.pruned_nonce_records, 2);
    assert_eq!(cut_report.pruned_source_set_records, 1);
    assert_eq!(cut_report.pruned_retirement_records, 0);
    assert!(load_restart_source_set_authority(&journal, 1, role)
        .expect("load pruned terminal source-set")
        .is_none());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Prepared,
    )
    .expect("load retained Prepared after source cut")
    .is_some());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Terminal,
    )
    .expect("load retained Terminal after source cut")
    .is_some());

    let retry = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("retry after terminal source-set prune cut");
    assert_eq!(retry.checkpoint_generation, 2);
    assert_eq!(retry.pruned_nonce_records, 0);
    assert_eq!(retry.pruned_source_set_records, 0);
    assert_eq!(retry.pruned_retirement_records, 2);
    assert_eq!(retry.pruned_old_checkpoints, 0);
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Prepared,
    )
    .expect("load reclaimed Prepared after retry")
    .is_none());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Terminal,
    )
    .expect("load reclaimed Terminal after retry")
    .is_none());
    let recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("recover terminal-source retry authority");
    assert_eq!(recovery.durable.generation, 2);
    assert_eq!(recovery.checkpoint_generation, Some(2));
    assert_eq!(recovery.journal_records, 0);
}

#[test]
fn retry_after_prepared_retirement_prune_keeps_terminal_authority() {
    let fixture = encrypted_retirement_fixture("terminal-last-prune-retry", 7);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    prepare_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect("prepare terminal-last retry retirement");
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("terminalize terminal-last retry retirement"),
        EncryptedRetirementOutcome::Terminal
    );

    let cut_report = compact_restart_metadata(
        &journal,
        None,
        RestartMetadataCompactionCut::AfterPreparedRetirementPruneBeforeTerminalPrune,
    )
    .expect("cut after Prepared retirement prune");
    assert_eq!(cut_report.checkpoint_generation, 2);
    assert_eq!(cut_report.pruned_nonce_records, 2);
    assert_eq!(cut_report.pruned_retirement_records, 1);
    assert_eq!(cut_report.pruned_source_set_records, 0);
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Prepared,
    )
    .expect("load pruned Prepared after retirement cut")
    .is_none());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Terminal,
    )
    .expect("load retained Terminal after retirement cut")
    .is_some());

    let retry = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("retry with Terminal-only completion authority");
    assert_eq!(retry.checkpoint_generation, 2);
    assert_eq!(retry.pruned_nonce_records, 0);
    assert_eq!(retry.pruned_retirement_records, 1);
    assert_eq!(retry.pruned_source_set_records, 0);
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Terminal,
    )
    .expect("load reclaimed Terminal after retry")
    .is_none());
    let recovery = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect("recover terminal-last retry authority");
    assert_eq!(recovery.durable.generation, 2);
    assert_eq!(recovery.checkpoint_generation, Some(2));
    assert_eq!(recovery.journal_records, 0);
}
