#[test]
fn orphan_source_set_authority_is_not_compaction_authority() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xa1; 32];
    let (journal_directory, _stage_directory, aes_key, nonce_prefix, _restart_limits, _) =
        prepared_source_bound_restart_stage(
            "orphan-source-set-compaction",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let manifest_name = encrypted_stage_manifest_name(
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    );
    std::fs::remove_file(journal_directory.0.join(manifest_name))
        .expect("remove live manifest to orphan source-set authority");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("orphan source-set authority must fail closed");
    assert!(error.contains("source-set authority without live restart or cleanup"));
    assert!(!journal_directory.0.join(nonce_compaction_name(1)).exists());
    assert!(journal_directory.0.join(linux_nonce_journal_name(1)).exists());
}

#[test]
fn authenticated_checkpoint_chain_rejects_counter_rollback() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-checkpoint-rollback",
        &[5, 7, 3],
    );
    let journal = open_journal(&directory.0, &key, prefix);
    let checkpoint_two = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 2,
        next_unreserved: Some(12),
    };
    let checkpoint_three = NonceCompactionCheckpoint {
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        generation: 3,
        next_unreserved: Some(11),
    };
    persist_nonce_compaction_checkpoint(
        &journal,
        checkpoint_two,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist valid generation-two checkpoint");
    persist_nonce_compaction_checkpoint(
        &journal,
        checkpoint_three,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist authenticated rollback checkpoint");

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("newer authenticated checkpoint must not roll nonce floor backward");
    assert!(error.contains("nonce compaction checkpoint rollback"));
}

#[test]
fn competing_authenticated_retirement_generations_fail_before_checkpoint_creation() {
    let fixture = encrypted_retirement_fixture("compaction-competing-retirement", 7);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let prepared = prepare_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect("prepare first retirement pair");
    let competing = EncryptedRestartRetirementRecord {
        fresh_generation: 3,
        ..prepared
    };
    persist_encrypted_retirement_record(&journal, competing)
        .expect("persist authenticated competing retirement pair");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("competing retirement generations must fail closed");
    assert!(error.contains("compaction competing retirement generations"));
    assert!(!fixture.journal_directory.0.join(nonce_compaction_name(2)).exists());
}

#[test]
fn prepared_and_terminal_payloads_must_match_before_reclamation() {
    let fixture = encrypted_retirement_fixture("compaction-retirement-payload-mismatch", 7);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let prepared = prepare_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect("prepare retirement payload");
    std::fs::remove_file(
        fixture
            .journal_directory
            .0
            .join(encrypted_stage_manifest_name(
                1,
                EncryptedRestartStageRole::SortedDescriptorSpill,
            )),
    )
    .expect("remove live manifest before forged terminal");
    let mut forged_digest = prepared.output_sha256;
    forged_digest[0] ^= 0x80;
    let forged_terminal = EncryptedRestartRetirementRecord {
        state: EncryptedRetirementState::Terminal,
        output_sha256: forged_digest,
        ..prepared
    };
    persist_encrypted_retirement_record(&journal, forged_terminal)
        .expect("persist authenticated mismatched terminal");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("mismatched retirement payloads must fail closed");
    assert!(error.contains("compaction retirement payload mismatch"));
    assert!(!fixture.journal_directory.0.join(nonce_compaction_name(2)).exists());
}

#[test]
fn source_set_must_match_live_manifest_identity_and_operation() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xa2; 32];
    let (journal_directory, _stage_directory, aes_key, nonce_prefix, _restart_limits, _) =
        prepared_source_bound_restart_stage(
            "source-set-live-manifest-mismatch",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let original = load_restart_source_set_authority(&journal, 1, role)
        .expect("load original source-set authority")
        .expect("original source-set authority");
    let name = restart_source_set_authority_name(1, role);
    std::fs::remove_file(journal_directory.0.join(&name))
        .expect("remove original source-set authority");
    let mut forged_identity = original.stage_identity;
    forged_identity[0] ^= 0x40;
    let forged = RestartSourceSetAuthority {
        stage_identity: forged_identity,
        ..original
    };
    let sealed = seal_restart_source_set_authority(&journal, forged)
        .expect("seal forged source-set authority");
    let path = linux_nonce_procfd_child(&journal.directory, &name)
        .expect("source-set procfd path");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .expect("create forged source-set authority");
    file.write_all(&sealed).expect("write forged source-set authority");
    file.flush().expect("flush forged source-set authority");
    file.sync_all().expect("sync forged source-set authority");
    journal.directory.sync_all().expect("sync forged source-set directory entry");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("source-set/live-manifest mismatch must fail closed");
    assert!(error.contains("compaction source-set/live-manifest mismatch"));
    assert!(!journal_directory.0.join(nonce_compaction_name(1)).exists());
}
