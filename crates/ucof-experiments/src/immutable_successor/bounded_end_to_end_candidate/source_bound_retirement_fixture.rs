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
