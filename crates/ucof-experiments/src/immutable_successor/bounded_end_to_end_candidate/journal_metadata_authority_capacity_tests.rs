#[test]
fn source_set_authority_respects_configured_journal_entry_capacity() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xd2; 32];
    let fixture = encrypted_retirement_fixture("source-set-authority-capacity", OBJECTS);
    let initial = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let manifest = load_encrypted_stage_manifest(
        &initial,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load source-set capacity manifest")
    .expect("source-set capacity manifest");
    drop(initial);

    let occupied = directory_entry_count(&fixture.journal_directory.0);
    let full = LinuxDurableNonceJournal::open(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: occupied,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open full source-set journal");
    let error = persist_restart_source_set_authority(
        &full,
        manifest,
        source_set_id,
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect_err("source-set create must reject a full journal directory");
    assert!(error.contains("restart source-set authority directory headroom"));
    assert!(load_restart_source_set_authority(
        &full,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load rejected source-set")
    .is_none());

    let one_free = LinuxDurableNonceJournal::open(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: occupied + 1,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open one-free-slot source-set journal");
    persist_restart_source_set_authority(
        &one_free,
        manifest,
        source_set_id,
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect("one free journal entry permits source-set authority");
    assert_eq!(directory_entry_count(&fixture.journal_directory.0), occupied + 1);
}

#[test]
fn retirement_authority_respects_configured_journal_entry_capacity() {
    const OBJECTS: u64 = 7;
    let fixture = encrypted_retirement_fixture("retirement-authority-capacity", OBJECTS);
    let occupied = directory_entry_count(&fixture.journal_directory.0);

    let full = LinuxDurableNonceJournal::open(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: occupied,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open full retirement journal");
    let error = prepare_encrypted_restart_retirement(
        &full,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect_err("Prepared authority must reject a full journal directory");
    assert!(error.contains("encrypted restart retirement directory headroom"));
    assert!(load_encrypted_retirement_record(
        &full,
        1,
        fixture.durable.continuation.fresh_generation,
        EncryptedRetirementState::Prepared,
    )
    .expect("load rejected Prepared authority")
    .is_none());

    let one_free = LinuxDurableNonceJournal::open(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
        [0x5a; 32],
        LinuxNonceJournalLimits {
            max_directory_entries: occupied + 1,
            ..LinuxNonceJournalLimits::default()
        },
    )
    .expect("open one-free-slot retirement journal");
    let prepared = prepare_encrypted_restart_retirement(
        &one_free,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect("one free journal entry permits Prepared authority");
    assert_eq!(prepared.state, EncryptedRetirementState::Prepared);
    assert_eq!(directory_entry_count(&fixture.journal_directory.0), occupied + 1);
}
