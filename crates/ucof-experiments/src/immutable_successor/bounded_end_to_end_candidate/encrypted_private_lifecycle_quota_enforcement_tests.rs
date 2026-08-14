#[test]
fn restart_private_quota_rejects_before_fresh_nonce_journal_or_publication() {
    let RestartPublicationFixture {
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        mut sources,
        ..
    } = restart_publication_fixture("restart-private-quota", 23);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let before = journal.scan(None).expect("journal before quota rejection");
    assert_eq!(before.generations, 1);
    let inventory = scan_encrypted_private_persistent_inventory(&journal)
        .expect("persistent inventory before quota rejection");
    assert_eq!(inventory.nonce_journal_records, 1);
    assert_eq!(inventory.retirement_records, 0);

    let spill = super::spill_limits(17, 3);
    let output_bytes = super::expected_canonical_output_bytes(
        &sources,
        super::ImmutableLimits::default(),
    )
    .expect("restart output bytes");
    let plan = encrypted_crash_resume_storage_plan(
        sources.len(),
        output_bytes,
        spill,
        inventory,
    )
    .expect("restart storage plan");
    let continuation = EncryptedRestartContinuationSettings {
        aes_key,
        crashed_generation: 1,
        trusted_floor: None,
        restart_limits,
        options: super::options(),
        limits: super::ImmutableLimits::default(),
        fresh_operation_id: [0xa7; 16],
    };
    let mut backend =
        RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    let error = stage_and_publish_verified_encrypted_restart_with_private_quota(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut backend,
        &mut sources,
        EncryptedRestartPublicationQuotaSettings {
            continuation,
            spill_limits: spill,
            max_private_storage_bytes: plan.required_bytes - 1,
        },
    )
    .expect_err("one-byte-short restart lifecycle quota must fail");
    assert!(error.contains("encrypted restart private storage limit"));
    let after_rejection = journal.scan(None).expect("journal after quota rejection");
    assert_eq!(after_rejection, before);
    work_directory.assert_empty();

    let (actual_plan, outcome) = stage_and_publish_verified_encrypted_restart_with_private_quota(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut backend,
        &mut sources,
        EncryptedRestartPublicationQuotaSettings {
            continuation,
            spill_limits: spill,
            max_private_storage_bytes: plan.required_bytes,
        },
    )
    .expect("exact restart lifecycle quota");
    assert_eq!(actual_plan, plan);
    assert!(matches!(
        outcome,
        EncryptedRestartPublicationOutcome::PublishedAndDurable(_)
    ));
    let after_success = journal.scan(None).expect("journal after exact quota success");
    assert_eq!(after_success.generations, before.generations + 1);
    work_directory.assert_empty();
}

#[test]
fn persistent_inventory_counts_authenticated_retirement_records() {
    let fixture = encrypted_retirement_fixture("quota-inventory-retirement", 7);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let before = scan_encrypted_private_persistent_inventory(&journal)
        .expect("inventory before retirement preparation");
    assert_eq!(before.nonce_journal_records, 2);
    assert_eq!(before.retirement_records, 0);

    prepare_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect("prepare retirement record");
    let prepared = scan_encrypted_private_persistent_inventory(&journal)
        .expect("inventory after prepared retirement");
    assert_eq!(prepared.nonce_journal_records, 2);
    assert_eq!(prepared.retirement_records, 1);

    execute_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        1,
        2,
        fixture.restart_limits,
        EncryptedRetirementCut::Complete,
    )
    .expect("complete retirement");
    let terminal = scan_encrypted_private_persistent_inventory(&journal)
        .expect("inventory after terminal retirement");
    assert_eq!(terminal.nonce_journal_records, 2);
    assert_eq!(terminal.retirement_records, 2);
}
