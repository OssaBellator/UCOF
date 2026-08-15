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

#[test]
fn live_manifest_must_match_original_nonce_operation_context() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xa3; 32];
    let (journal_directory, _stage_directory, aes_key, nonce_prefix, _restart_limits, _) =
        prepared_source_bound_restart_stage(
            "manifest-nonce-operation-mismatch",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let original = load_nonce_generation_record(&journal, 1).expect("load original nonce record");
    let name = OsString::from(linux_nonce_journal_name(1));
    std::fs::remove_file(journal_directory.0.join(&name))
        .expect("remove original nonce record");
    let forged = LinuxNonceJournalRecord {
        operation_id: [0xe3; 16],
        ..original
    };
    let sealed = journal.seal_record(forged).expect("seal forged nonce record");
    let path = linux_nonce_procfd_child(&journal.directory, &name)
        .expect("forged nonce procfd path");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .expect("create forged nonce record");
    file.write_all(&sealed).expect("write forged nonce record");
    file.flush().expect("flush forged nonce record");
    file.sync_all().expect("sync forged nonce record");
    journal.directory.sync_all().expect("sync forged nonce directory entry");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("live manifest/nonce operation mismatch must fail closed");
    assert!(error.contains("compaction live manifest/nonce mismatch"));
    assert!(!journal_directory.0.join(nonce_compaction_name(1)).exists());
}

#[test]
fn source_set_cleanup_identity_must_match_retirement_lineage() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xa4; 32];
    let fixture = encrypted_retirement_fixture("source-set-retirement-mismatch", OBJECTS);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let manifest = load_encrypted_stage_manifest(&journal, 1, role)
        .expect("load cleanup manifest")
        .expect("cleanup manifest");
    persist_restart_source_set_authority(
        &journal,
        manifest,
        source_set_id,
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect("persist cleanup source-set authority");
    prepare_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        &fixture.durable,
        fixture.restart_limits,
    )
    .expect("prepare cleanup retirement");
    std::fs::remove_file(
        fixture
            .journal_directory
            .0
            .join(encrypted_stage_manifest_name(1, role)),
    )
    .expect("remove live manifest after Prepared authority");

    let original = load_restart_source_set_authority(&journal, 1, role)
        .expect("load cleanup source-set")
        .expect("cleanup source-set");
    let name = restart_source_set_authority_name(1, role);
    std::fs::remove_file(fixture.journal_directory.0.join(&name))
        .expect("remove original cleanup source-set");
    let mut forged_identity = original.stage_identity;
    forged_identity[1] ^= 0x20;
    let forged = RestartSourceSetAuthority {
        stage_identity: forged_identity,
        ..original
    };
    let sealed = seal_restart_source_set_authority(&journal, forged)
        .expect("seal forged cleanup source-set");
    let path = linux_nonce_procfd_child(&journal.directory, &name)
        .expect("cleanup source-set procfd path");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .expect("create forged cleanup source-set");
    file.write_all(&sealed).expect("write forged cleanup source-set");
    file.flush().expect("flush forged cleanup source-set");
    file.sync_all().expect("sync forged cleanup source-set");
    journal.directory.sync_all().expect("sync cleanup source-set directory entry");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("source-set/retirement mismatch must fail closed");
    assert!(error.contains("compaction source-set/retirement mismatch"));
    assert!(!fixture.journal_directory.0.join(nonce_compaction_name(2)).exists());
}

#[test]
fn retirement_generation_ahead_of_global_nonce_authority_fails_before_checkpoint() {
    let fixture = encrypted_retirement_fixture("retirement-generation-ahead", 7);
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
    .expect("prepare generation-two retirement");
    std::fs::remove_file(
        fixture
            .journal_directory
            .0
            .join(encrypted_retirement_name(
                1,
                2,
                EncryptedRetirementState::Prepared,
            )),
    )
    .expect("remove generation-two prepared authority");
    let forged = EncryptedRestartRetirementRecord {
        fresh_generation: 3,
        ..prepared
    };
    persist_encrypted_retirement_record(&journal, forged)
        .expect("persist future retirement authority");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("future retirement generation must fail closed");
    assert!(error.contains("compaction retirement generation ahead of nonce authority"));
    assert!(!fixture.journal_directory.0.join(nonce_compaction_name(2)).exists());
}

#[test]
fn compacted_scan_rejects_authenticated_record_replayed_under_wrong_generation_name() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-filename-replay",
        &[5, 7],
    );
    let journal = open_journal(&directory.0, &key, prefix);
    let generation_one = load_nonce_generation_record(&journal, 1)
        .expect("load generation-one record for replay");
    let generation_two_name = OsString::from(linux_nonce_journal_name(2));
    std::fs::remove_file(directory.0.join(&generation_two_name))
        .expect("remove original generation-two record");
    let replay = journal
        .seal_record(generation_one)
        .expect("seal authenticated replay record");
    let path = linux_nonce_procfd_child(&journal.directory, &generation_two_name)
        .expect("replayed nonce procfd path");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .expect("create replayed generation-two filename");
    file.write_all(&replay).expect("write replayed nonce record");
    file.flush().expect("flush replayed nonce record");
    file.sync_all().expect("sync replayed nonce record");
    journal.directory.sync_all().expect("sync replayed nonce directory entry");

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("authenticated nonce record under wrong filename must fail closed");
    assert!(error.contains("compacted nonce filename generation"));
    assert!(!directory.0.join(nonce_compaction_name(2)).exists());
}

#[test]
fn compaction_rejects_authenticated_retirement_from_foreign_journal_context() {
    let fixture = encrypted_retirement_fixture("compaction-foreign-retirement-context", 7);
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
    .expect("prepare retirement before foreign-context replacement");
    let name = encrypted_retirement_name(1, 2, EncryptedRetirementState::Prepared);
    std::fs::remove_file(fixture.journal_directory.0.join(&name))
        .expect("remove original prepared retirement");
    let forged = EncryptedRestartRetirementRecord {
        key_id: [0xf1; 16],
        ..prepared
    };
    let sealed = seal_encrypted_retirement_record(&journal, forged)
        .expect("seal foreign-context retirement with local auth key");
    let path = linux_nonce_procfd_child(&journal.directory, &name)
        .expect("foreign retirement procfd path");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .expect("create foreign-context retirement");
    file.write_all(&sealed).expect("write foreign-context retirement");
    file.flush().expect("flush foreign-context retirement");
    file.sync_all().expect("sync foreign-context retirement");
    journal.directory.sync_all().expect("sync foreign retirement directory entry");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("foreign retirement context must fail before checkpoint creation");
    assert!(error.contains("compaction retirement context"));
    assert!(!fixture.journal_directory.0.join(nonce_compaction_name(2)).exists());
}

#[test]
fn compaction_rejects_authenticated_source_set_from_foreign_journal_context() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xa5; 32];
    let (journal_directory, _stage_directory, aes_key, nonce_prefix, _restart_limits, _) =
        prepared_source_bound_restart_stage(
            "compaction-foreign-source-context",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let original = load_restart_source_set_authority(&journal, 1, role)
        .expect("load original source-set context")
        .expect("original source-set context");
    let name = restart_source_set_authority_name(1, role);
    std::fs::remove_file(journal_directory.0.join(&name))
        .expect("remove original source-set context");
    let forged = RestartSourceSetAuthority {
        nonce_prefix: [0xf2; 4],
        ..original
    };
    let sealed = seal_restart_source_set_authority(&journal, forged)
        .expect("seal foreign-context source-set with local auth key");
    let path = linux_nonce_procfd_child(&journal.directory, &name)
        .expect("foreign source-set procfd path");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .expect("create foreign-context source-set");
    file.write_all(&sealed).expect("write foreign-context source-set");
    file.flush().expect("flush foreign-context source-set");
    file.sync_all().expect("sync foreign-context source-set");
    journal.directory.sync_all().expect("sync foreign source-set directory entry");

    let error = compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect_err("foreign source-set context must fail before checkpoint creation");
    assert!(error.contains("compaction source-set context"));
    assert!(!journal_directory.0.join(nonce_compaction_name(1)).exists());
}

#[test]
fn older_checkpoint_cannot_be_masked_by_newer_checkpoint_when_below_surviving_record() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-masked-historical-rollback",
        &[5, 7, 3],
    );
    let journal = open_journal(&directory.0, &key, prefix);
    persist_nonce_compaction_checkpoint(
        &journal,
        NonceCompactionCheckpoint {
            key_id: journal.key_id,
            nonce_prefix: journal.nonce_prefix,
            generation: 1,
            next_unreserved: Some(4),
        },
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist older checkpoint below surviving generation-one record");
    persist_nonce_compaction_checkpoint(
        &journal,
        NonceCompactionCheckpoint {
            key_id: journal.key_id,
            nonce_prefix: journal.nonce_prefix,
            generation: 3,
            next_unreserved: Some(15),
        },
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist newer checkpoint that would otherwise mask older contradiction");

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("older checkpoint contradiction must not be masked by latest checkpoint");
    assert!(error.contains("nonce compaction checkpoint rollback"));
}

#[test]
fn older_same_generation_checkpoint_mismatch_cannot_be_masked_by_newer_checkpoint() {
    let (directory, key, prefix) = nonce_compaction_fixture(
        "nonce-compaction-masked-generation-mismatch",
        &[5, 7, 3],
    );
    let journal = open_journal(&directory.0, &key, prefix);
    persist_nonce_compaction_checkpoint(
        &journal,
        NonceCompactionCheckpoint {
            key_id: journal.key_id,
            nonce_prefix: journal.nonce_prefix,
            generation: 2,
            next_unreserved: Some(13),
        },
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist generation-two checkpoint disagreeing with surviving record");
    persist_nonce_compaction_checkpoint(
        &journal,
        NonceCompactionCheckpoint {
            key_id: journal.key_id,
            nonce_prefix: journal.nonce_prefix,
            generation: 3,
            next_unreserved: Some(15),
        },
        RestartMetadataCompactionCut::Complete,
    )
    .expect("persist newer checkpoint that would otherwise mask generation-two mismatch");

    let error = CompactedNonceJournal::new(&journal)
        .scan(None)
        .expect_err("same-generation checkpoint mismatch must remain visible across newer checkpoint");
    assert!(error.contains("nonce compaction checkpoint generation mismatch"));
}
