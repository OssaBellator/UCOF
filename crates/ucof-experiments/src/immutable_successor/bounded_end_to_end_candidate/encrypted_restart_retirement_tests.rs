struct EncryptedRetirementFixture {
    journal_directory: super::TestDirectory,
    stage_directory: super::TestDirectory,
    work_directory: super::TestDirectory,
    aes_key: [u8; 32],
    nonce_prefix: [u8; 4],
    restart_limits: LinuxEncryptedStageRestartLimits,
    durable: Box<DurableEncryptedRestartPublication>,
}

fn encrypted_retirement_fixture(label: &str, object_count: u64) -> EncryptedRetirementFixture {
    let RestartPublicationFixture {
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        mut sources,
        ..
    } = restart_publication_fixture(label, object_count);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut backend =
        RestartPublicationTestBackend::new(super::PersistentPublicationLinkOutcome::Linked);
    let outcome = stage_and_publish_verified_encrypted_restart(
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
    .expect("durable publication for retirement");
    let EncryptedRestartPublicationOutcome::PublishedAndDurable(durable) = outcome else {
        panic!("retirement fixture requires durable publication");
    };
    work_directory.assert_empty();
    EncryptedRetirementFixture {
        journal_directory,
        stage_directory,
        work_directory,
        aes_key,
        nonce_prefix,
        restart_limits,
        durable,
    }
}

fn retirement_file_exists(
    directory: &Path,
    crashed_generation: u64,
    fresh_generation: u64,
    state: EncryptedRetirementState,
) -> bool {
    directory
        .join(encrypted_retirement_name(
            crashed_generation,
            fresh_generation,
            state,
        ))
        .exists()
}

#[test]
fn retirement_without_durable_prepared_record_has_no_destructive_authority() {
    let fixture = encrypted_retirement_fixture("retirement-no-prepared", 7);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let stage_name =
        encrypted_stage_file_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let manifest_name =
        encrypted_stage_manifest_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    assert!(fixture.stage_directory.0.join(&stage_name).exists());
    assert!(fixture.journal_directory.0.join(&manifest_name).exists());

    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("no-prepared retirement disposition"),
        EncryptedRetirementOutcome::NoPreparedAuthority
    );
    assert!(fixture.stage_directory.0.join(stage_name).exists());
    assert!(fixture.journal_directory.0.join(manifest_name).exists());
    assert!(!retirement_file_exists(
        &fixture.journal_directory.0,
        1,
        2,
        EncryptedRetirementState::Terminal,
    ));
}

#[test]
fn durable_prepared_retirement_removes_exact_stage_and_manifest_then_commits_terminal() {
    let fixture = encrypted_retirement_fixture("retirement-terminal", 11);
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
    .expect("prepare encrypted retirement");
    assert_eq!(prepared.state, EncryptedRetirementState::Prepared);
    assert_eq!(prepared.output_length, fixture.durable.output_length);
    assert_eq!(prepared.output_sha256, fixture.durable.output_sha256);
    assert!(retirement_file_exists(
        &fixture.journal_directory.0,
        1,
        2,
        EncryptedRetirementState::Prepared,
    ));

    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("execute encrypted retirement"),
        EncryptedRetirementOutcome::Terminal
    );
    assert_eq!(directory_entry_count(&fixture.stage_directory.0), 0);
    assert!(!fixture
        .journal_directory
        .0
        .join(encrypted_stage_manifest_name(
            1,
            EncryptedRestartStageRole::SortedDescriptorSpill,
        ))
        .exists());
    assert!(retirement_file_exists(
        &fixture.journal_directory.0,
        1,
        2,
        EncryptedRetirementState::Terminal,
    ));
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("replayed terminal retirement"),
        EncryptedRetirementOutcome::AlreadyTerminal
    );
    fixture.work_directory.assert_empty();
}

#[test]
fn crash_after_prepared_before_unlink_retries_exact_cleanup() {
    let fixture = encrypted_retirement_fixture("retirement-cut-prepared", 5);
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
    .expect("prepare cut retirement");
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::AfterPreparedBeforeUnlink,
        )
        .expect("prepared cut"),
        EncryptedRetirementOutcome::Cut(EncryptedRetirementCut::AfterPreparedBeforeUnlink)
    );
    assert_eq!(directory_entry_count(&fixture.stage_directory.0), 1);
    assert!(fixture
        .journal_directory
        .0
        .join(encrypted_stage_manifest_name(
            1,
            EncryptedRestartStageRole::SortedDescriptorSpill,
        ))
        .exists());
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("retry prepared retirement"),
        EncryptedRetirementOutcome::Terminal
    );
}

#[test]
fn partial_unlink_and_synced_absence_restart_finalize_without_blind_redelete() {
    let fixture = encrypted_retirement_fixture("retirement-cut-unlink", 5);
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
    .expect("prepare partial retirement");
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::AfterStageUnlinkBeforeDirectorySync,
        )
        .expect("partial unlink cut"),
        EncryptedRetirementOutcome::Cut(
            EncryptedRetirementCut::AfterStageUnlinkBeforeDirectorySync
        )
    );
    assert_eq!(directory_entry_count(&fixture.stage_directory.0), 0);
    assert!(fixture
        .journal_directory
        .0
        .join(encrypted_stage_manifest_name(
            1,
            EncryptedRestartStageRole::SortedDescriptorSpill,
        ))
        .exists());
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::AfterDirectorySyncBeforeTerminal,
        )
        .expect("resume partial retirement"),
        EncryptedRetirementOutcome::Cut(
            EncryptedRetirementCut::AfterDirectorySyncBeforeTerminal
        )
    );
    assert!(!fixture
        .journal_directory
        .0
        .join(encrypted_stage_manifest_name(
            1,
            EncryptedRestartStageRole::SortedDescriptorSpill,
        ))
        .exists());
    assert!(!retirement_file_exists(
        &fixture.journal_directory.0,
        1,
        2,
        EncryptedRetirementState::Terminal,
    ));
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("finalize synced absence"),
        EncryptedRetirementOutcome::Terminal
    );
}

#[test]
fn renamed_stage_is_removed_by_strong_identity_after_preparation() {
    let fixture = encrypted_retirement_fixture("retirement-renamed", 7);
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
    .expect("prepare renamed retirement");
    let expected =
        encrypted_stage_file_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let renamed = OsString::from("retirement-renamed-stage.bin");
    std::fs::rename(
        fixture.stage_directory.0.join(expected),
        fixture.stage_directory.0.join(&renamed),
    )
    .expect("rename retirement stage");
    let pinned = linux_nonce_open_private_directory(&fixture.stage_directory.0)
        .expect("pin renamed retirement directory");
    pinned.sync_all().expect("sync renamed retirement directory");
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("execute renamed retirement"),
        EncryptedRetirementOutcome::Terminal
    );
    assert!(!fixture.stage_directory.0.join(renamed).exists());
}

#[test]
fn expected_name_replacement_blocks_both_stage_and_manifest_cleanup() {
    let fixture = encrypted_retirement_fixture("retirement-conflict", 7);
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
    .expect("prepare conflicting retirement");
    let expected =
        encrypted_stage_file_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let moved = OsString::from("retirement-original-stage.bin");
    std::fs::rename(
        fixture.stage_directory.0.join(&expected),
        fixture.stage_directory.0.join(&moved),
    )
    .expect("move original retirement stage");
    let replacement = fixture.stage_directory.0.join(&expected);
    std::fs::write(&replacement, b"replacement").expect("write retirement replacement");
    let mut permissions = std::fs::metadata(&replacement)
        .expect("retirement replacement metadata")
        .permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(&replacement, permissions)
        .expect("retirement replacement permissions");
    let pinned = linux_nonce_open_private_directory(&fixture.stage_directory.0)
        .expect("pin conflicting retirement directory");
    pinned.sync_all().expect("sync conflicting retirement directory");

    let manifest_path = fixture.journal_directory.0.join(encrypted_stage_manifest_name(
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    ));
    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("conflicting retirement disposition"),
        EncryptedRetirementOutcome::RetainIndeterminate
    );
    assert!(fixture.stage_directory.0.join(expected).exists());
    assert!(fixture.stage_directory.0.join(moved).exists());
    assert!(manifest_path.exists());
    assert!(!retirement_file_exists(
        &fixture.journal_directory.0,
        1,
        2,
        EncryptedRetirementState::Terminal,
    ));
}

#[test]
fn manifest_tamper_blocks_stage_cleanup_before_any_unlink() {
    let fixture = encrypted_retirement_fixture("retirement-manifest-tamper", 7);
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
    .expect("prepare manifest-tamper retirement");
    let stage_name =
        encrypted_stage_file_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let manifest_name =
        encrypted_stage_manifest_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let manifest_path = fixture.journal_directory.0.join(&manifest_name);
    let mut manifest = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&manifest_path)
        .expect("open retirement manifest");
    std::io::Seek::seek(&mut manifest, std::io::SeekFrom::Start(88))
        .expect("seek retirement manifest");
    let mut byte = [0u8; 1];
    manifest
        .read_exact(&mut byte)
        .expect("read retirement manifest byte");
    byte[0] ^= 0x80;
    std::io::Seek::seek(&mut manifest, std::io::SeekFrom::Start(88))
        .expect("seek retirement manifest");
    manifest
        .write_all(&byte)
        .expect("tamper retirement manifest");
    manifest.sync_all().expect("sync tampered retirement manifest");
    journal
        .directory
        .sync_all()
        .expect("sync retirement journal directory");

    assert_eq!(
        execute_encrypted_restart_retirement(
            &journal,
            &fixture.stage_directory.0,
            1,
            2,
            fixture.restart_limits,
            EncryptedRetirementCut::Complete,
        )
        .expect("manifest-tamper retirement disposition"),
        EncryptedRetirementOutcome::RetainIndeterminate
    );
    assert!(fixture.stage_directory.0.join(stage_name).exists());
    assert!(manifest_path.exists());
    assert!(!retirement_file_exists(
        &fixture.journal_directory.0,
        1,
        2,
        EncryptedRetirementState::Terminal,
    ));
}
