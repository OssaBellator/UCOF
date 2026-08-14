#[test]
fn compaction_checkpoint_quota_rejects_before_checkpoint_or_prune_and_exact_cap_succeeds() {
    let (directory, key, prefix) = nonce_compaction_fixture("compaction-quota", &[5, 7]);
    let journal = open_journal(&directory.0, &key, prefix);
    let plan = compaction_storage_plan(&journal).expect("compaction quota plan");
    assert_eq!(
        plan.new_checkpoint_bytes,
        u64::try_from(NONCE_COMPACTION_BYTES).expect("checkpoint width")
    );
    assert_eq!(
        plan.required_before_prune,
        plan.existing_persistent_bytes + plan.new_checkpoint_bytes
    );

    let error = compact_restart_metadata_with_private_quota(
        &journal,
        None,
        plan.required_before_prune - 1,
        RestartMetadataCompactionCut::Complete,
    )
    .expect_err("one-byte-short compaction quota must fail");
    assert!(error.contains("compaction private storage limit"));
    assert!(!directory.0.join(nonce_compaction_name(2)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(1)).exists());
    assert!(directory.0.join(linux_nonce_journal_name(2)).exists());
    assert_eq!(
        journal.scan(None).expect("legacy authority after quota rejection").durable.generation,
        2
    );

    let (actual_plan, report) = compact_restart_metadata_with_private_quota(
        &journal,
        None,
        plan.required_before_prune,
        RestartMetadataCompactionCut::Complete,
    )
    .expect("exact compaction metadata quota");
    assert_eq!(actual_plan, plan);
    assert_eq!(report.checkpoint_generation, 2);
    assert_eq!(report.pruned_nonce_records, 2);
    assert_eq!(
        CompactedNonceJournal::new(&journal)
            .scan(None)
            .expect("authority after exact compaction quota")
            .durable
            .generation,
        2
    );
}

#[test]
fn compacted_inventory_prices_checkpoint_and_all_preserved_authenticated_metadata() {
    const OBJECTS: u64 = 7;
    let source_set_id = [0x71; 32];
    let fixture = encrypted_retirement_fixture("compacted-inventory", OBJECTS);
    let journal = open_journal(
        &fixture.journal_directory.0,
        &fixture.aes_key,
        fixture.nonce_prefix,
    );
    let manifest = load_encrypted_stage_manifest(
        &journal,
        1,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    )
    .expect("load compacted inventory manifest")
    .expect("compacted inventory manifest");
    persist_restart_source_set_authority(
        &journal,
        manifest,
        source_set_id,
        usize::try_from(OBJECTS).expect("object count"),
    )
    .expect("persist compacted inventory source authority");
    prepare_encrypted_restart_retirement(
        &journal,
        &fixture.stage_directory.0,
        &fixture.publication,
        fixture.restart_limits,
    )
    .expect("prepare compacted inventory retirement");
    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("compact inventory with live authority");

    let inventory = scan_compacted_persistent_inventory(&journal)
        .expect("scan checkpointed persistent inventory");
    assert_eq!(inventory.nonce_records, 1);
    assert_eq!(inventory.checkpoint_records, 1);
    assert_eq!(inventory.retirement_records, 1);
    assert_eq!(inventory.source_set_records, 1);
    let expected_bytes = u64::try_from(LINUX_NONCE_JOURNAL_BYTES).unwrap()
        + u64::try_from(NONCE_COMPACTION_BYTES).unwrap()
        + u64::try_from(ENCRYPTED_RETIREMENT_BYTES).unwrap()
        + u64::try_from(RESTART_SOURCE_SET_BYTES).unwrap();
    assert_eq!(inventory.total_bytes, expected_bytes);

    let base = compacted_crash_resume_storage_plan(
        usize::try_from(OBJECTS).expect("object count"),
        1_000_000,
        super::spill_limits(17, 3),
        inventory,
    )
    .expect("checkpointed crash-resume plan");
    assert_eq!(base.persistent_bytes, expected_bytes);
    assert_eq!(
        base.required_bytes,
        base.base_without_existing_inventory.required_bytes + expected_bytes
    );
}

#[test]
fn checkpointed_source_bound_restart_quota_preserves_pre_side_effect_rejection() {
    const OBJECTS: u64 = 11;
    let source_set_id = [0x72; 32];
    let (journal_directory, stage_directory, aes_key, nonce_prefix, restart_limits, _) =
        prepared_source_bound_restart_stage(
            "checkpointed-source-bound-quota",
            OBJECTS,
            source_set_id,
        );
    let journal = open_journal(&journal_directory.0, &aes_key, nonce_prefix);
    compact_restart_metadata(&journal, None, RestartMetadataCompactionCut::Complete)
        .expect("checkpoint live source-bound state");
    let work_directory = super::TestDirectory::new("checkpointed-source-bound-quota-work");
    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut sources = original.clone();
    let inventory = scan_compacted_persistent_inventory(&journal)
        .expect("checkpointed source-bound inventory");
    assert_eq!(inventory.checkpoint_records, 1);
    assert_eq!(inventory.nonce_records, 1);
    assert_eq!(inventory.source_set_records, 1);
    let output_bytes = super::expected_canonical_output_bytes(
        &sources,
        super::ImmutableLimits::default(),
    )
    .expect("checkpointed source-bound output size");
    let spill = super::spill_limits(17, 3);
    let plan = compacted_crash_resume_storage_plan(
        sources.len(),
        output_bytes,
        spill,
        inventory,
    )
    .expect("checkpointed source-bound quota plan");
    let continuation = EncryptedRestartContinuationSettings {
        aes_key,
        crashed_generation: 1,
        trusted_floor: None,
        restart_limits,
        options: super::options(),
        limits: super::ImmutableLimits::default(),
        fresh_operation_id: [0x73; 16],
    };
    let mut output = Vec::new();
    let error = continue_compacted_source_bound_restart_with_private_quota(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut output,
        &mut sources,
        CompactedSourceBoundRestartQuotaSettings {
            continuation,
            spill_limits: spill,
            max_private_storage_bytes: plan.required_bytes - 1,
            source_set_id,
        },
    )
    .expect_err("one-byte-short checkpointed restart quota must fail");
    assert!(error.contains("compacted source-bound restart private storage limit"));
    assert!(output.is_empty());
    assert_eq!(
        CompactedNonceJournal::new(&journal)
            .scan(None)
            .expect("checkpointed authority after quota rejection")
            .durable
            .generation,
        1
    );
    work_directory.assert_empty();

    let mut exact_sources = original;
    let mut exact_output = Vec::new();
    let (actual_plan, evidence) = continue_compacted_source_bound_restart_with_private_quota(
        &journal,
        &stage_directory.0,
        &work_directory.0,
        &mut exact_output,
        &mut exact_sources,
        CompactedSourceBoundRestartQuotaSettings {
            continuation,
            spill_limits: spill,
            max_private_storage_bytes: plan.required_bytes,
            source_set_id,
        },
    )
    .expect("exact checkpointed restart quota");
    assert_eq!(actual_plan, plan);
    assert_eq!(evidence.fresh_generation, 2);
    assert_eq!(
        CompactedNonceJournal::new(&journal)
            .scan(None)
            .expect("checkpointed authority after exact quota")
            .durable
            .generation,
        2
    );
    work_directory.assert_empty();
}
