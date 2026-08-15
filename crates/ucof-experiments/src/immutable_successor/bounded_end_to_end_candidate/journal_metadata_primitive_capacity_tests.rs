fn open_journal_with_entry_capacity(
    directory: &Path,
    aes_key: &[u8; 32],
    prefix: [u8; 4],
    max_directory_entries: usize,
) -> LinuxDurableNonceJournal {
    LinuxDurableNonceJournal::open(
        directory,
        aes_key,
        prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open entry-capacity journal")
}

#[test]
fn ordinary_stage_manifest_rejects_full_journal_before_durable_stage_creation() {
    const OBJECTS: u64 = 7;
    let journal_directory = private_directory("stage-manifest-entry-cap-journal");
    let stage_directory = private_directory("stage-manifest-entry-cap-stage");
    let aes_key = [0xd6; 32];
    let nonce_prefix = [0x56; 4];
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut authority = journal.recover_authority(None).expect("initial authority");
    let lease_size = OBJECTS.checked_mul(2).expect("restart lease size");
    let mut session = journal
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x66; 16],
            lease_size,
            JournalCommitCut::Complete,
        )
        .expect("durable encrypted restart lease");
    let mut sources: Vec<_> = (1..=OBJECTS)
        .rev()
        .map(super::TinySource::new)
        .collect();
    let preflight = prepare_encrypted_spill_preflight(
        &stage_directory.0,
        &mut sources,
        super::options(),
        super::ImmutableLimits::default(),
        super::spill_limits(5, 2),
        &mut session,
    )
    .expect("encrypted spill preflight");
    let journal_entries_before = directory_entry_count(&journal_directory.0);
    let stage_entries_before = directory_entry_count(&stage_directory.0);
    assert_eq!(journal_entries_before, 1);
    let durable_stage_name = encrypted_stage_file_name(
        session.journal_generation,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    );
    assert!(!stage_directory.0.join(&durable_stage_name).exists());
    drop(journal);

    let constrained = open_journal_with_entry_capacity(
        &journal_directory.0,
        &aes_key,
        nonce_prefix,
        journal_entries_before,
    );
    let error = persist_sorted_encrypted_spill_restart_stage(
        &constrained,
        &stage_directory.0,
        &preflight,
        &session,
        LinuxEncryptedStageRestartLimits::default(),
        EncryptedStageManifestCommitCut::Complete,
    )
    .expect_err("full journal must reject stage-manifest persistence before durable stage creation");
    assert!(matches!(
        error,
        LinuxEncryptedStageRestartError::Journal(ref message)
            if message.contains("encrypted stage manifest directory headroom")
    ));
    assert_eq!(
        directory_entry_count(&journal_directory.0),
        journal_entries_before
    );
    assert_eq!(
        directory_entry_count(&stage_directory.0),
        stage_entries_before
    );
    assert!(!stage_directory.0.join(&durable_stage_name).exists());
    drop(constrained);

    let exact_capacity = journal_entries_before
        .checked_add(1)
        .expect("one manifest journal slot");
    let exact = open_journal_with_entry_capacity(
        &journal_directory.0,
        &aes_key,
        nonce_prefix,
        exact_capacity,
    );
    let manifest = persist_sorted_encrypted_spill_restart_stage(
        &exact,
        &stage_directory.0,
        &preflight,
        &session,
        LinuxEncryptedStageRestartLimits::default(),
        EncryptedStageManifestCommitCut::Complete,
    )
    .expect("one free journal slot must permit exact stage-manifest persistence");
    assert_eq!(manifest.generation, session.journal_generation);
    assert_eq!(directory_entry_count(&journal_directory.0), exact_capacity);
    assert!(stage_directory.0.join(&durable_stage_name).exists());
}

#[test]
fn ordinary_prepared_retirement_respects_configured_journal_entry_capacity() {
    let fixture = encrypted_retirement_fixture("retirement-entry-cap", 7);
    let journal_entries_before = directory_entry_count(&fixture.journal_directory.0);
    let constrained = open_journal_with_entry_capacity(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
        journal_entries_before,
    );
    let error = prepare_encrypted_restart_retirement(
        &constrained,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect_err("full journal must reject Prepared retirement authority");
    assert!(error.contains("encrypted retirement directory headroom"));
    assert_eq!(
        directory_entry_count(&fixture.journal_directory.0),
        journal_entries_before
    );
    assert!(!retirement_file_exists(
        &fixture.journal_directory.0,
        fixture.durable.continuation.crashed_generation,
        fixture.durable.continuation.fresh_generation,
        EncryptedRetirementState::Prepared,
    ));
    fixture.work_directory.assert_empty();
    drop(constrained);

    let exact_capacity = journal_entries_before
        .checked_add(1)
        .expect("one retirement journal slot");
    let exact = open_journal_with_entry_capacity(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
        exact_capacity,
    );
    let prepared = prepare_encrypted_restart_retirement(
        &exact,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect("one free journal slot must permit Prepared retirement authority");
    assert_eq!(prepared.state, EncryptedRetirementState::Prepared);
    assert_eq!(
        directory_entry_count(&fixture.journal_directory.0),
        exact_capacity
    );
    assert!(retirement_file_exists(
        &fixture.journal_directory.0,
        fixture.durable.continuation.crashed_generation,
        fixture.durable.continuation.fresh_generation,
        EncryptedRetirementState::Prepared,
    ));

    assert_eq!(
        execute_encrypted_restart_retirement(
            &exact,
            &fixture.stage_directory.0,
            fixture.durable.continuation.crashed_generation,
            fixture.durable.continuation.fresh_generation,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("manifest reclamation must free the slot needed by Terminal authority"),
        EncryptedRetirementOutcome::Terminal
    );
    assert!(retirement_file_exists(
        &fixture.journal_directory.0,
        fixture.durable.continuation.crashed_generation,
        fixture.durable.continuation.fresh_generation,
        EncryptedRetirementState::Terminal,
    ));
    assert_eq!(
        directory_entry_count(&fixture.journal_directory.0),
        exact_capacity
    );
    fixture.work_directory.assert_empty();
}
