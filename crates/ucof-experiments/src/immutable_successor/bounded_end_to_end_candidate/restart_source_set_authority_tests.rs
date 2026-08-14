fn prepared_source_bound_restart_stage(
    label: &str,
    object_count: u64,
    source_set_id: [u8; 32],
) -> (
    super::TestDirectory,
    super::TestDirectory,
    [u8; 32],
    [u8; 4],
    LinuxEncryptedStageRestartLimits,
    RestartSourceSetAuthority,
) {
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            label,
            object_count,
            EncryptedStageManifestCommitCut::Complete,
        );
    let manifest = persisted.expect("persist source-bound encrypted stage");
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let authority = persist_restart_source_set_authority(
        &journal,
        manifest,
        source_set_id,
        usize::try_from(object_count).expect("object count"),
    )
    .expect("persist restart source-set authority");
    (
        journal_directory,
        stage_directory,
        aes_key,
        nonce_prefix,
        limits,
        authority,
    )
}

#[test]
fn restart_source_set_authority_round_trips_and_binds_stage_identity() {
    const OBJECTS: u64 = 13;
    let source_set_id = [0xb7; 32];
    let (journal_directory, _stage_directory, aes_key, nonce_prefix, _limits, expected) =
        prepared_source_bound_restart_stage(
            "source-set-authority-round-trip",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let loaded = load_restart_source_set_authority(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load restart source-set authority")
    .expect("source-set authority exists");
    assert_eq!(loaded, expected);
    assert_eq!(loaded.source_set_id, source_set_id);
    assert_eq!(loaded.object_count, OBJECTS);
    let manifest = load_encrypted_stage_manifest(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load source-set manifest")
    .expect("source-set manifest exists");
    assert_eq!(loaded.stage_identity, manifest.identity());
    assert_eq!(loaded.operation_id, manifest.operation_id);
    assert_eq!(loaded.key_id, manifest.key_id);
    assert_eq!(loaded.nonce_prefix, manifest.nonce_prefix);
}

#[test]
fn missing_source_set_authority_blocks_fresh_generation() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xb8; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            "source-set-missing",
            OBJECTS,
            EncryptedStageManifestCommitCut::Complete,
        );
    persisted.expect("persist manifest without source-set authority");
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let work_directory = super::TestDirectory::new("source-set-missing-work");
    let mut sources: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut output = Vec::new();
    let error = continue_source_bound_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut output,
        &mut sources,
        source_set_id,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits: limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0xc8; 16],
        },
    )
    .expect_err("missing source-set authority must fail");
    assert!(error.contains("source-set authority missing"));
    assert!(output.is_empty());
    let recovered = journal
        .recover_authority(None)
        .expect("recover authority after missing source-set failure");
    assert_eq!(recovered.durable.generation, 1);
    assert_eq!(recovered.next_unreserved(), Some(OBJECTS * 2));
    work_directory.assert_empty();
}

#[test]
fn wrong_source_set_identity_blocks_fresh_generation() {
    const OBJECTS: u64 = 9;
    let correct_source_set_id = [0xb9; 32];
    let wrong_source_set_id = [0xba; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, _) =
        prepared_source_bound_restart_stage(
            "source-set-wrong",
            OBJECTS,
            correct_source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let work_directory = super::TestDirectory::new("source-set-wrong-work");
    let mut sources: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut output = Vec::new();
    let error = continue_source_bound_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut output,
        &mut sources,
        wrong_source_set_id,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits: limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0xc9; 16],
        },
    )
    .expect_err("wrong source-set identity must fail");
    assert!(error.contains("source-set identity mismatch"));
    assert!(output.is_empty());
    let recovered = journal
        .recover_authority(None)
        .expect("recover authority after wrong source-set failure");
    assert_eq!(recovered.durable.generation, 1);
    assert_eq!(recovered.next_unreserved(), Some(OBJECTS * 2));
    work_directory.assert_empty();
}

#[test]
fn tampered_source_set_authority_blocks_fresh_generation() {
    const OBJECTS: u64 = 5;
    let source_set_id = [0xbb; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, _) =
        prepared_source_bound_restart_stage(
            "source-set-tamper",
            OBJECTS,
            source_set_id,
        );
    let name = restart_source_set_authority_name(
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    );
    let path = journal_directory.0.join(name);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .expect("open source-set record for tamper");
    file.seek(SeekFrom::Start(
        u64::try_from(RESTART_SOURCE_SET_BYTES - 1).expect("source-set tamper offset"),
    ))
    .expect("seek source-set tag");
    let mut byte = [0u8; 1];
    file.read_exact(&mut byte).expect("read source-set tag byte");
    byte[0] ^= 0x80;
    file.seek(SeekFrom::Start(
        u64::try_from(RESTART_SOURCE_SET_BYTES - 1).expect("source-set tamper offset"),
    ))
    .expect("reseek source-set tag");
    file.write_all(&byte).expect("tamper source-set tag");
    file.flush().expect("flush source-set tamper");
    file.sync_all().expect("sync source-set tamper");
    drop(file);

    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let work_directory = super::TestDirectory::new("source-set-tamper-work");
    let mut sources: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut output = Vec::new();
    let error = continue_source_bound_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut output,
        &mut sources,
        source_set_id,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits: limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0xca; 16],
        },
    )
    .expect_err("tampered source-set authority must fail");
    assert!(error.contains("source-set authentication"));
    assert!(output.is_empty());
    let recovered = journal
        .recover_authority(None)
        .expect("recover authority after source-set tamper");
    assert_eq!(recovered.durable.generation, 1);
    assert_eq!(recovered.next_unreserved(), Some(OBJECTS * 2));
    work_directory.assert_empty();
}

#[test]
fn correct_source_set_identity_continues_canonical_encrypted_tree_restart() {
    const OBJECTS: u64 = 17;
    let source_set_id = [0xbc; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, _) =
        prepared_source_bound_restart_stage(
            "source-set-canonical",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let work_directory = super::TestDirectory::new("source-set-canonical-work");
    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        super::ImmutableLimits::default(),
    )
    .expect("source-set baseline writer");

    let mut resumed_sources = original;
    let mut resumed = Vec::new();
    let evidence = continue_source_bound_encrypted_tree_restart(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut resumed,
        &mut resumed_sources,
        source_set_id,
        EncryptedRestartContinuationSettings {
            aes_key,
            crashed_generation: 1,
            trusted_floor: None,
            restart_limits: limits,
            options: super::options(),
            limits: super::ImmutableLimits::default(),
            fresh_operation_id: [0xcb; 16],
        },
    )
    .expect("source-bound encrypted-tree continuation");
    assert_eq!(resumed, baseline);
    assert_eq!(evidence.output.output, baseline_report);
    assert_eq!(evidence.crashed_generation, 1);
    assert_eq!(evidence.fresh_generation, 2);
    let recovered = journal
        .recover_authority(None)
        .expect("recover source-bound fresh authority");
    assert_eq!(recovered.durable.generation, 2);
    work_directory.assert_empty();
}
