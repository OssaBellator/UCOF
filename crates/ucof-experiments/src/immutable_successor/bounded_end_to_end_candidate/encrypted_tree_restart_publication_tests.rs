#[test]
fn encrypted_tree_restart_publication_is_canonical_and_retirement_compatible() {
    const OBJECTS: u64 = 11;
    let RestartPublicationFixture {
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        mut sources,
        baseline,
        baseline_report,
    } = restart_publication_fixture("encrypted-tree-restart-publication", OBJECTS);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut backend = RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
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
            fresh_operation_id: [0xa3; 16],
        },
    )
    .expect("durable encrypted-tree restart publication");

    let EncryptedTreeRestartPublicationOutcome::PublishedAndDurable(durable) = outcome else {
        panic!("parent-synced encrypted-tree publication must be durable");
    };
    assert_eq!(backend.destination.as_deref(), Some(baseline.as_slice()));
    assert_eq!(durable.durable.continuation.output.output, baseline_report);
    assert_eq!(
        durable.durable.output_length,
        u64::try_from(baseline.len()).expect("baseline length")
    );
    assert_eq!(
        durable.durable.output_sha256,
        <[u8; 32]>::from(Sha256::digest(&baseline))
    );
    assert_ne!(durable.tree_stage_ciphertext_sha256, [0u8; 32]);
    assert!(!durable.durable.cleanup_pending);

    let tree_nonces = consolidated_encrypted_tree_stage_record_count(
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect("tree nonce count");
    let fresh_size = OBJECTS.checked_add(tree_nonces).expect("fresh lease size");
    let authority = journal
        .recover_authority(None)
        .expect("durable encrypted-tree publication authority");
    assert_eq!(authority.durable.generation, 2);
    assert_eq!(authority.next_unreserved(), Some(OBJECTS * 2 + fresh_size));

    let prepared = prepare_encrypted_restart_retirement(
        &journal,
        &stage_directory.0,
        &durable.durable,
        restart_limits,
    )
    .expect("prepare encrypted-tree restart retirement");
    assert_eq!(prepared.crashed_generation, 1);
    assert_eq!(prepared.fresh_generation, 2);
    assert_eq!(prepared.output_sha256, durable.durable.output_sha256);
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &stage_directory.0,
            1,
            2,
            restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("retire encrypted-tree restart state"),
        EncryptedRetirementOutcome::Terminal
    );
    work_directory.assert_empty();
}

#[test]
fn encrypted_tree_restart_destination_exists_never_mints_retirement_authority() {
    const OBJECTS: u64 = 7;
    let RestartPublicationFixture {
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        mut sources,
        ..
    } = restart_publication_fixture("encrypted-tree-restart-exists", OBJECTS);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut backend = RestartPublicationTestBackend::new(
        super::PersistentPublicationLinkOutcome::DestinationExists,
    );
    backend.destination = Some(b"existing destination".to_vec());
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
            fresh_operation_id: [0xa4; 16],
        },
    )
    .expect("destination-exists encrypted-tree restart publication");
    assert!(matches!(
        outcome,
        EncryptedTreeRestartPublicationOutcome::NotPublishedDestinationExists
    ));
    assert_eq!(
        backend.destination.as_deref(),
        Some(b"existing destination".as_slice())
    );
    assert!(backend.private.is_empty());
    assert!(backend.aborted);
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Prepared,
    )
    .expect("load absent retirement authority")
    .is_none());
    work_directory.assert_empty();
}

#[test]
fn encrypted_tree_restart_parent_sync_failure_remains_indeterminate() {
    const OBJECTS: u64 = 5;
    let RestartPublicationFixture {
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        mut sources,
        baseline,
        ..
    } = restart_publication_fixture("encrypted-tree-restart-parent-sync", OBJECTS);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut backend = RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    backend.fail_sync_parent = true;
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
            fresh_operation_id: [0xa5; 16],
        },
    )
    .expect("parent-sync encrypted-tree restart publication");
    assert!(matches!(
        outcome,
        EncryptedTreeRestartPublicationOutcome::PublicationIndeterminate {
            stage: super::PersistentPublicationStage::SyncParent
        }
    ));
    assert_eq!(backend.destination.as_deref(), Some(baseline.as_slice()));
    assert_eq!(backend.private.as_slice(), baseline.as_slice());
    assert!(load_encrypted_retirement_record(
        &journal,
        1,
        2,
        EncryptedRetirementState::Prepared,
    )
    .expect("load absent indeterminate retirement authority")
    .is_none());
    work_directory.assert_empty();
}
