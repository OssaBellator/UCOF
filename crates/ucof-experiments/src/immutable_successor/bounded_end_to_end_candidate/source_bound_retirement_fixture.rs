fn source_bound_retirement_fixture(
    label: &str,
    object_count: u64,
    source_set_id: [u8; 32],
) -> EncryptedRetirementFixture {
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(label, object_count, source_set_id);
    let work_directory = super::TestDirectory::new(&format!("{label}-work"));
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut sources: Vec<_> = (1..=object_count).rev().map(super::TinySource::new).collect();
    let mut backend =
        RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    let outcome = stage_and_publish_verified_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut backend,
        &mut sources,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0x93; 16],
        },
    )
    .expect("durable source-bound publication for retirement fixture");
    let EncryptedTreeRestartPublicationOutcome::PublishedAndDurable(durable) = outcome else {
        panic!("source-bound retirement fixture requires durable publication");
    };
    work_directory.assert_empty();
    EncryptedRetirementFixture {
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        durable: Box::new(durable.durable),
    }
}

#[test]
fn source_set_persistence_retry_is_idempotent_but_cannot_rebind_history() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xd8; 32];
    let fixture = source_bound_retirement_fixture(
        "source-set-idempotent-retry",
        OBJECTS,
        source_set_id,
    );
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    assert_eq!(
        journal
            .recover_authority(None)
            .expect("recover advanced source-set authority")
            .durable
            .generation,
        2
    );
    let manifest = load_encrypted_stage_manifest(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load idempotent source-set manifest")
    .expect("idempotent source-set manifest");

    let existing = persist_restart_source_set_authority(
        &journal,
        manifest,
        source_set_id,
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect("exact persisted source-set retry remains idempotent after generation advance");
    assert_eq!(existing.source_set_id, source_set_id);
    assert_eq!(existing.generation, 1);

    let error = persist_restart_source_set_authority(
        &journal,
        manifest,
        [0xd9; 32],
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect_err("historical source-set authority cannot be rebound after generation advance");
    assert!(error.contains("restart source-set authority conflict"));
}
