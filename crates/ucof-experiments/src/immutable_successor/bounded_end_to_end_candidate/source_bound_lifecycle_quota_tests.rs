#[test]
fn source_bound_persistent_inventory_counts_authenticated_source_set_records() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0xbd; 32];
    let (journal_directory, _stage_directory, aes_key, nonce_prefix, _limits, _) =
        prepared_source_bound_restart_stage(
            "source-bound-inventory",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let inventory = scan_source_bound_persistent_inventory(&journal)
        .expect("scan source-bound persistent inventory");
    assert_eq!(inventory.base.nonce_journal_records, 1);
    assert_eq!(inventory.base.retirement_records, 0);
    assert_eq!(inventory.source_set_records, 1);
    assert_eq!(
        source_set_record_storage_bytes(inventory.source_set_records)
            .expect("source-set inventory bytes"),
        u64::try_from(RESTART_SOURCE_SET_BYTES).expect("source-set width")
    );
}

#[test]
fn source_bound_lifecycle_prices_authority_record_on_encrypted_tree_spine() {
    let spill = super::spill_limits(17, 3);
    let inventory = SourceBoundPersistentInventory {
        base: EncryptedPrivatePersistentInventory {
            nonce_journal_records: 1,
            retirement_records: 0,
        },
        source_set_records: 1,
    };
    let object_count = 401;
    let output_bytes = 2_000_000;
    let source_bytes = u64::try_from(RESTART_SOURCE_SET_BYTES).expect("source-set width");

    let (plain_normal, tree) = consolidated_encrypted_tree_normal_lifecycle_plan(
        object_count,
        output_bytes,
        spill,
        inventory.base,
    )
    .expect("consolidated normal lifecycle");
    let normal = source_bound_normal_storage_plan(
        object_count,
        output_bytes,
        spill,
        inventory,
    )
    .expect("source-bound normal lifecycle");
    assert_eq!(normal.source_set_record_bytes, source_bytes);
    assert_eq!(
        normal.restart_transcode_window_bytes,
        plain_normal.restart_transcode_window_bytes + 2 * source_bytes
    );
    assert_eq!(
        normal.output_window_bytes,
        plain_normal.output_window_bytes + 2 * source_bytes
    );
    assert!(tree.required_post_preflight_bytes > 0);

    let (plain_restart, restart_tree) = consolidated_encrypted_tree_crash_resume_lifecycle_plan(
        object_count,
        output_bytes,
        spill,
        inventory.base,
    )
    .expect("consolidated restart lifecycle");
    let restart = source_bound_crash_resume_storage_plan(
        object_count,
        output_bytes,
        spill,
        inventory,
    )
    .expect("source-bound restart lifecycle");
    assert_eq!(tree, restart_tree);
    assert_eq!(restart.existing_source_set_bytes, source_bytes);
    assert_eq!(
        restart.restart_transcode_window_bytes,
        plain_restart.restart_transcode_window_bytes + source_bytes
    );
    assert_eq!(
        restart.output_window_bytes,
        plain_restart.output_window_bytes + source_bytes
    );
    assert_eq!(
        restart.retirement_prepared_window_bytes,
        plain_restart.retirement_prepared_window_bytes + source_bytes
    );
    assert_eq!(
        restart.retirement_terminal_window_bytes,
        plain_restart.retirement_terminal_window_bytes + source_bytes
    );
}

#[test]
fn source_bound_restart_quota_rejects_before_generation_two_and_exact_cap_succeeds() {
    const OBJECTS: u64 = 11;
    let source_set_id = [0xbe; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(
            "source-bound-quota",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    let work_directory = super::TestDirectory::new("source-bound-quota-work");
    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut sources = original.clone();
    let inventory = scan_source_bound_persistent_inventory(&journal)
        .expect("source-bound quota inventory");
    let output_bytes = super::expected_canonical_output_bytes(
        &sources,
        super::ImmutableLimits::default(),
    )
    .expect("source-bound output bytes");
    let spill = super::spill_limits(17, 3);
    let plan = source_bound_crash_resume_storage_plan(
        sources.len(),
        output_bytes,
        spill,
        inventory,
    )
    .expect("source-bound quota plan");
    let continuation = EncryptedRestartContinuationSettings {
        aes_key,
        crashed_generation: 1,
        trusted_floor: None,
        restart_limits,
        options: super::options(),
        limits: super::ImmutableLimits::default(),
        fresh_operation_id: [0xce; 16],
    };
    let mut output = Vec::new();
    let error = continue_source_bound_encrypted_tree_restart_with_private_quota(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut output,
        &mut sources,
        SourceBoundRestartQuotaSettings {
            continuation,
            spill_limits: spill,
            max_private_storage_bytes: plan.required_bytes - 1,
            source_set_id,
        },
    )
    .expect_err("one-byte-short source-bound quota must fail");
    assert!(error.contains("source-bound restart private storage limit"));
    assert!(output.is_empty());
    let rejected = journal
        .recover_authority(None)
        .expect("source-bound authority after quota rejection");
    assert_eq!(rejected.durable.generation, 1);
    assert_eq!(rejected.next_unreserved(), Some(OBJECTS * 2));
    work_directory.assert_empty();

    let mut exact_sources = original;
    let mut exact_output = Vec::new();
    let (actual_plan, evidence) = continue_source_bound_encrypted_tree_restart_with_private_quota(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut exact_output,
        &mut exact_sources,
        SourceBoundRestartQuotaSettings {
            continuation,
            spill_limits: spill,
            max_private_storage_bytes: plan.required_bytes,
            source_set_id,
        },
    )
    .expect("exact source-bound quota");
    assert_eq!(actual_plan, plan);
    assert_eq!(evidence.fresh_generation, 2);
    let accepted = journal
        .recover_authority(None)
        .expect("source-bound authority after exact quota");
    assert_eq!(accepted.durable.generation, 2);
    work_directory.assert_empty();
}

#[test]
fn stage_manifest_without_source_set_authority_is_a_non_authoritative_cut() {
    const OBJECTS: u64 = 5;
    let source_set_id = [0xbf; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, limits, persisted) =
        prepared_encrypted_restart_stage(
            "source-bound-manifest-cut",
            OBJECTS,
            EncryptedStageManifestCommitCut::Complete,
        );
    let manifest = persisted.expect("persist stage manifest before source-set cut");
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    assert!(load_restart_source_set_authority(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load absent source-set cut")
    .is_none());
    assert_eq!(manifest.generation, 1);
    let (disposition, _) = classify_encrypted_spill_restart(
        &journal,
        &stage_directory.0,
        &aes_key,
        1,
        None,
        limits,
    )
    .expect("legacy stage remains strongly verified");
    assert!(matches!(
        disposition,
        EncryptedStageRestartDisposition::VerifiedExactNeedsFreshLease { .. }
    ));
    assert!(verify_restart_source_set_authority(
        &journal,
        manifest,
        source_set_id,
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect_err("source-bound authority must be absent after cut")
    .contains("authority missing"));
    let recovered = journal
        .recover_authority(None)
        .expect("recover manifest-cut journal authority");
    assert_eq!(recovered.durable.generation, 1);
}
