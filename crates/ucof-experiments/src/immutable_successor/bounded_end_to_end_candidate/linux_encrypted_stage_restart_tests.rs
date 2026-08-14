fn prepared_encrypted_restart_stage(
    label: &str,
    object_count: u64,
    cut: EncryptedStageManifestCommitCut,
) -> (
    super::TestDirectory,
    super::TestDirectory,
    [u8; 32],
    [u8; 4],
    LinuxEncryptedStageRestartLimits,
    Result<LinuxEncryptedStageManifest, LinuxEncryptedStageRestartError>,
) {
    let journal_directory = private_directory(&format!("{label}-journal"));
    let stage_directory = private_directory(&format!("{label}-stage"));
    let aes_key = [0xd3; 32];
    let nonce_prefix = [0x53; 4];
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let mut authority = journal.recover_authority(None).expect("initial authority");
    let lease_size = object_count.checked_mul(2).expect("restart lease size");
    let mut session = journal
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            [0x63; 16],
            lease_size,
            JournalCommitCut::Complete,
        )
        .expect("durable encrypted restart lease");
    let mut sources: Vec<_> = (1..=object_count)
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
    assert_eq!(session.remaining(), object_count);
    let limits = LinuxEncryptedStageRestartLimits::default();
    let persisted = persist_sorted_encrypted_spill_restart_stage(
        &journal,
        &stage_directory.0,
        &preflight,
        &session,
        limits,
        cut,
    );
    drop(preflight);
    drop(session);
    drop(journal);
    (
        journal_directory,
        stage_directory,
        aes_key,
        nonce_prefix,
        limits,
        persisted,
    )
}

#[test]
fn exact_durable_encrypted_stage_verifies_and_requires_fresh_lease() {
    const OBJECTS: u64 = 17;
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-restart-exact",
            OBJECTS,
            EncryptedStageManifestCommitCut::Complete,
        );
    let manifest = persisted.expect("persist exact encrypted stage");
    assert_eq!(manifest.generation, 1);
    assert_eq!(directory_entry_count(&stage_directory.0), 1);

    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let (disposition, report) = classify_encrypted_spill_restart(
        &journal,
        &stage_directory.0,
        &aes_key,
        1,
        None,
        limits,
    )
    .expect("classify exact encrypted stage");
    assert_eq!(
        disposition,
        EncryptedStageRestartDisposition::VerifiedExactNeedsFreshLease {
            object_count: usize::try_from(OBJECTS).unwrap(),
        }
    );
    let report = report.expect("exact inventory report");
    assert_eq!(
        report.observation,
        crate::private_cleanup_restart_inventory::InventoryObservation::ExactIdentity
    );
    assert_eq!(report.scanned_entries, 1);
    assert!(report.scanned_identity_bytes > 0);

    let mut recovered = journal
        .recover_authority(None)
        .expect("recover burned old lease");
    assert_eq!(recovered.durable.generation, 1);
    assert_eq!(recovered.next_unreserved(), Some(OBJECTS * 2));
    let fresh_session = journal
        .commit_descriptor_session(
            &mut recovered,
            aes_key,
            [0x64; 16],
            OBJECTS,
            JournalCommitCut::Complete,
        )
        .expect("fresh resume lease");
    assert_eq!(fresh_session.lease.first, OBJECTS * 2);
    assert_eq!(fresh_session.journal_generation, 2);
}

#[test]
fn renamed_durable_encrypted_stage_verifies_by_strong_identity() {
    const OBJECTS: u64 = 9;
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-restart-renamed",
            OBJECTS,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist renamed encrypted stage");
    let expected_name = encrypted_stage_file_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let renamed = OsString::from("renamed-encrypted-stage.bin");
    std::fs::rename(
        stage_directory.0.join(&expected_name),
        stage_directory.0.join(&renamed),
    )
    .expect("rename encrypted stage");
    let pinned = linux_nonce_open_private_directory(&stage_directory.0).expect("pin renamed stage directory");
    pinned.sync_all().expect("sync renamed stage directory");

    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let (disposition, report) = classify_encrypted_spill_restart(
        &journal,
        &stage_directory.0,
        &aes_key,
        1,
        None,
        limits,
    )
    .expect("classify renamed encrypted stage");
    assert_eq!(
        disposition,
        EncryptedStageRestartDisposition::VerifiedRenamedNeedsFreshLease {
            object_count: usize::try_from(OBJECTS).unwrap(),
            actual_name: renamed,
        }
    );
    assert_eq!(
        report.expect("renamed report").observation,
        crate::private_cleanup_restart_inventory::InventoryObservation::MissingMatchingIdentityElsewhere
    );
}

#[test]
fn complete_absence_burns_old_lease_and_restarts_work() {
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-restart-absent",
            7,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist absent encrypted stage");
    let expected_name = encrypted_stage_file_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    std::fs::remove_file(stage_directory.0.join(expected_name)).expect("remove encrypted stage");
    let pinned = linux_nonce_open_private_directory(&stage_directory.0).expect("pin absent stage directory");
    pinned.sync_all().expect("sync absent stage directory");

    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let (disposition, report) = classify_encrypted_spill_restart(
        &journal,
        &stage_directory.0,
        &aes_key,
        1,
        None,
        limits,
    )
    .expect("classify absent encrypted stage");
    assert_eq!(
        disposition,
        EncryptedStageRestartDisposition::StageAbsentRestartWork
    );
    assert_eq!(
        report.expect("absence report").observation,
        crate::private_cleanup_restart_inventory::InventoryObservation::MissingNoMatchingIdentityCompleteScan
    );
    let recovered = journal
        .recover_authority(None)
        .expect("recover burned absent-stage lease");
    assert_eq!(recovered.next_unreserved(), Some(14));
}

#[test]
fn expected_name_replacement_and_identity_budget_failure_are_indeterminate() {
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-restart-conflict",
            5,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist conflict encrypted stage");
    let expected_name = encrypted_stage_file_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let original = stage_directory.0.join(&expected_name);
    std::fs::rename(&original, stage_directory.0.join("moved-original.bin"))
        .expect("move original encrypted stage");
    std::fs::write(&original, b"replacement").expect("write replacement stage");
    let mut permissions = std::fs::metadata(&original)
        .expect("replacement metadata")
        .permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(&original, permissions).expect("replacement permissions");
    let pinned = linux_nonce_open_private_directory(&stage_directory.0).expect("pin conflict stage directory");
    pinned.sync_all().expect("sync conflict stage directory");

    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let (disposition, report) = classify_encrypted_spill_restart(
        &journal,
        &stage_directory.0,
        &aes_key,
        1,
        None,
        limits,
    )
    .expect("classify conflicting encrypted stage");
    assert_eq!(disposition, EncryptedStageRestartDisposition::RetainIndeterminate);
    assert_eq!(
        report.expect("conflict report").observation,
        crate::private_cleanup_restart_inventory::InventoryObservation::DifferentIdentity
    );

    std::fs::remove_file(&original).expect("remove replacement");
    std::fs::rename(stage_directory.0.join("moved-original.bin"), &original)
        .expect("restore original");
    pinned.sync_all().expect("sync restored stage directory");
    let mut tiny_identity = limits;
    tiny_identity.max_identity_bytes = 1;
    let (disposition, report) = classify_encrypted_spill_restart(
        &journal,
        &stage_directory.0,
        &aes_key,
        1,
        None,
        tiny_identity,
    )
    .expect("classify identity-budget encrypted stage");
    assert_eq!(disposition, EncryptedStageRestartDisposition::RetainIndeterminate);
    assert_eq!(
        report.expect("identity budget report").observation,
        crate::private_cleanup_restart_inventory::InventoryObservation::NameMetadataUnreadable
    );
}

#[test]
fn durable_stage_without_durable_manifest_is_never_resumed() {
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-restart-no-manifest",
            5,
            EncryptedStageManifestCommitCut::AfterStageSyncBeforeManifest,
        );
    assert_eq!(
        persisted.expect_err("manifest cut must fail"),
        LinuxEncryptedStageRestartError::InjectedCut(
            EncryptedStageManifestCommitCut::AfterStageSyncBeforeManifest
        )
    );
    assert_eq!(directory_entry_count(&stage_directory.0), 1);
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let (disposition, report) = classify_encrypted_spill_restart(
        &journal,
        &stage_directory.0,
        &aes_key,
        1,
        None,
        limits,
    )
    .expect("classify unmanifested encrypted stage");
    assert_eq!(
        disposition,
        EncryptedStageRestartDisposition::NoDurableManifestRestartWork
    );
    assert!(report.is_none());
    assert_eq!(directory_entry_count(&stage_directory.0), 1);
}

#[test]
fn manifest_tamper_fails_closed_before_stage_classification() {
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-restart-manifest-tamper",
            5,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist manifest-tamper encrypted stage");
    let manifest_name = encrypted_stage_manifest_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let path = journal_directory.0.join(manifest_name);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("open encrypted stage manifest");
    std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(88)).expect("seek manifest digest");
    let mut byte = [0u8; 1];
    file.read_exact(&mut byte).expect("read manifest byte");
    byte[0] ^= 0x80;
    std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(88)).expect("seek manifest digest");
    file.write_all(&byte).expect("tamper manifest");
    file.sync_all().expect("sync tampered manifest");

    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    assert_eq!(
        classify_encrypted_spill_restart(
            &journal,
            &stage_directory.0,
            &aes_key,
            1,
            None,
            limits,
        )
        .expect_err("tampered manifest must fail"),
        LinuxEncryptedStageRestartError::AuthenticationFailed
    );
}

#[test]
fn aead_verifier_rejects_ciphertext_even_if_identity_is_recomputed() {
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            "encrypted-restart-aead",
            5,
            EncryptedStageManifestCommitCut::Complete,
        );
    let original_manifest = persisted.expect("persist AEAD encrypted stage");
    let stage_name = encrypted_stage_file_name(1, EncryptedRestartStageRole::SortedDescriptorSpill);
    let stage_path = stage_directory.0.join(&stage_name);
    let mut stage = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&stage_path)
        .expect("open encrypted stage");
    std::io::Seek::seek(&mut stage, std::io::SeekFrom::Start(24)).expect("seek ciphertext");
    let mut byte = [0u8; 1];
    stage.read_exact(&mut byte).expect("read ciphertext byte");
    byte[0] ^= 0x80;
    std::io::Seek::seek(&mut stage, std::io::SeekFrom::Start(24)).expect("seek ciphertext");
    stage.write_all(&byte).expect("tamper ciphertext");
    stage.sync_all().expect("sync tampered ciphertext");

    let metadata = stage.metadata().expect("tampered stage metadata");
    let (digest, _) = encrypted_stage_file_digest(&stage, limits.max_identity_bytes)
        .expect("tampered stage digest");
    let recomputed_manifest = LinuxEncryptedStageManifest {
        stage_dev: metadata.dev(),
        stage_ino: metadata.ino(),
        stage_length: metadata.len(),
        stage_sha256: digest,
        ..original_manifest
    };
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let nonce_record = load_nonce_generation_record(&journal, 1).expect("nonce generation");
    assert_eq!(
        verify_persisted_sorted_encrypted_spill(
            &stage,
            recomputed_manifest,
            nonce_record,
            &aes_key,
            limits,
        )
        .expect_err("AEAD must reject modified ciphertext"),
        LinuxEncryptedStageRestartError::StageAuthenticationFailed
    );
}
