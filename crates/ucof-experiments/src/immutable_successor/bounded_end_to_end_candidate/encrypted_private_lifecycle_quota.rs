#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EncryptedPrivatePersistentInventory {
    nonce_journal_records: usize,
    retirement_records: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EncryptedNormalPublicationStoragePlan {
    working: EncryptedSpillPrivateStoragePlan,
    output_bytes: u64,
    persistent_after_lease_bytes: u64,
    durable_restart_stage_bytes: u64,
    stage_manifest_bytes: u64,
    sort_window_bytes: u64,
    restart_copy_window_bytes: u64,
    restart_manifest_window_bytes: u64,
    restart_transcode_window_bytes: u64,
    output_window_bytes: u64,
    required_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EncryptedCrashResumeStoragePlan {
    output_bytes: u64,
    persistent_before_fresh_lease_bytes: u64,
    persistent_after_fresh_lease_bytes: u64,
    durable_restart_stage_bytes: u64,
    stage_manifest_bytes: u64,
    retained_encrypted_descriptor_bytes: u64,
    post_preflight_working_bytes: u64,
    restart_transcode_window_bytes: u64,
    output_window_bytes: u64,
    retirement_prepared_window_bytes: u64,
    retirement_terminal_window_bytes: u64,
    required_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UnifiedEncryptedPrivateStoragePlan {
    normal: EncryptedNormalPublicationStoragePlan,
    crash_resume: EncryptedCrashResumeStoragePlan,
    required_bytes: u64,
}

fn checked_private_storage_sum(label: &'static str, values: &[u64]) -> super::CandidateResult<u64> {
    values.iter().try_fold(0u64, |sum, value| {
        sum.checked_add(*value).ok_or_else(|| label.to_owned())
    })
}

fn encrypted_persistent_inventory_bytes(
    inventory: EncryptedPrivatePersistentInventory,
) -> super::CandidateResult<u64> {
    let nonce_bytes = super::checked_stage_bytes(
        inventory.nonce_journal_records,
        LINUX_NONCE_JOURNAL_BYTES,
    )?;
    let retirement_bytes =
        super::checked_stage_bytes(inventory.retirement_records, ENCRYPTED_RETIREMENT_BYTES)?;
    checked_private_storage_sum(
        "encrypted persistent inventory overflow",
        &[nonce_bytes, retirement_bytes],
    )
}

fn encrypted_post_preflight_working_bytes(
    working: EncryptedSpillPrivateStoragePlan,
) -> u64 {
    working
        .retained_plus_locator_bytes
        .max(working.locator_plus_leaf_ref_bytes)
        .max(working.max_adjacent_page_ref_bytes)
}

fn encrypted_normal_publication_storage_plan(
    object_count: usize,
    output_bytes: u64,
    spill_limits: super::BoundedSpillSortLimits,
    inventory_before_operation: EncryptedPrivatePersistentInventory,
) -> super::CandidateResult<EncryptedNormalPublicationStoragePlan> {
    if output_bytes == 0 {
        return Err("encrypted publication output bytes".into());
    }
    let working = encrypted_spill_private_storage_plan(object_count, spill_limits)?;
    let nonce_journal_records = inventory_before_operation
        .nonce_journal_records
        .checked_add(1)
        .ok_or_else(|| "encrypted nonce journal record count overflow".to_owned())?;
    let persistent_after_lease_bytes = encrypted_persistent_inventory_bytes(
        EncryptedPrivatePersistentInventory {
            nonce_journal_records,
            retirement_records: inventory_before_operation.retirement_records,
        },
    )?;
    let durable_restart_stage_bytes = working.encrypted_spill_descriptor_bytes;
    let stage_manifest_bytes =
        u64::try_from(ENCRYPTED_STAGE_MANIFEST_BYTES).expect("stage manifest width fits u64");

    let sort_window_bytes = checked_private_storage_sum(
        "encrypted sort storage overflow",
        &[persistent_after_lease_bytes, working.sorter_plus_encrypted_spill_bytes],
    )?;
    let restart_copy_window_bytes = checked_private_storage_sum(
        "encrypted restart copy storage overflow",
        &[
            persistent_after_lease_bytes,
            working.encrypted_spill_descriptor_bytes,
            durable_restart_stage_bytes,
        ],
    )?;
    let restart_manifest_window_bytes = restart_copy_window_bytes
        .checked_add(stage_manifest_bytes)
        .ok_or_else(|| "encrypted restart manifest storage overflow".to_owned())?;
    let restart_transcode_window_bytes = checked_private_storage_sum(
        "encrypted restart transcode storage overflow",
        &[
            persistent_after_lease_bytes,
            stage_manifest_bytes,
            durable_restart_stage_bytes,
            working.encrypted_spill_plus_retained_bytes,
        ],
    )?;
    let output_window_bytes = checked_private_storage_sum(
        "encrypted publication output storage overflow",
        &[
            persistent_after_lease_bytes,
            stage_manifest_bytes,
            durable_restart_stage_bytes,
            output_bytes,
            encrypted_post_preflight_working_bytes(working),
        ],
    )?;
    let required_bytes = sort_window_bytes
        .max(restart_copy_window_bytes)
        .max(restart_manifest_window_bytes)
        .max(restart_transcode_window_bytes)
        .max(output_window_bytes);

    Ok(EncryptedNormalPublicationStoragePlan {
        working,
        output_bytes,
        persistent_after_lease_bytes,
        durable_restart_stage_bytes,
        stage_manifest_bytes,
        sort_window_bytes,
        restart_copy_window_bytes,
        restart_manifest_window_bytes,
        restart_transcode_window_bytes,
        output_window_bytes,
        required_bytes,
    })
}

fn encrypted_crash_resume_storage_plan(
    object_count: usize,
    output_bytes: u64,
    spill_limits: super::BoundedSpillSortLimits,
    inventory_at_restart: EncryptedPrivatePersistentInventory,
) -> super::CandidateResult<EncryptedCrashResumeStoragePlan> {
    if output_bytes == 0 {
        return Err("encrypted restart output bytes".into());
    }
    let working = encrypted_spill_private_storage_plan(object_count, spill_limits)?;
    let persistent_before_fresh_lease_bytes =
        encrypted_persistent_inventory_bytes(inventory_at_restart)?;
    let fresh_nonce_journal_records = inventory_at_restart
        .nonce_journal_records
        .checked_add(1)
        .ok_or_else(|| "encrypted fresh nonce journal record count overflow".to_owned())?;
    let persistent_after_fresh_lease_bytes = encrypted_persistent_inventory_bytes(
        EncryptedPrivatePersistentInventory {
            nonce_journal_records: fresh_nonce_journal_records,
            retirement_records: inventory_at_restart.retirement_records,
        },
    )?;
    let durable_restart_stage_bytes = working.encrypted_spill_descriptor_bytes;
    let stage_manifest_bytes =
        u64::try_from(ENCRYPTED_STAGE_MANIFEST_BYTES).expect("stage manifest width fits u64");
    let retained_encrypted_descriptor_bytes = working.retained_encrypted_descriptor_bytes;
    let post_preflight_working_bytes = encrypted_post_preflight_working_bytes(working);

    let restart_transcode_window_bytes = checked_private_storage_sum(
        "encrypted crash-resume transcode storage overflow",
        &[
            persistent_after_fresh_lease_bytes,
            durable_restart_stage_bytes,
            stage_manifest_bytes,
            retained_encrypted_descriptor_bytes,
        ],
    )?;
    let output_window_bytes = checked_private_storage_sum(
        "encrypted crash-resume output storage overflow",
        &[
            persistent_after_fresh_lease_bytes,
            durable_restart_stage_bytes,
            stage_manifest_bytes,
            output_bytes,
            post_preflight_working_bytes,
        ],
    )?;

    let prepared_retirement_records = inventory_at_restart
        .retirement_records
        .checked_add(1)
        .ok_or_else(|| "encrypted prepared retirement record count overflow".to_owned())?;
    let prepared_persistent_bytes = encrypted_persistent_inventory_bytes(
        EncryptedPrivatePersistentInventory {
            nonce_journal_records: fresh_nonce_journal_records,
            retirement_records: prepared_retirement_records,
        },
    )?;
    let retirement_prepared_window_bytes = checked_private_storage_sum(
        "encrypted retirement prepared storage overflow",
        &[
            prepared_persistent_bytes,
            durable_restart_stage_bytes,
            stage_manifest_bytes,
            output_bytes,
        ],
    )?;

    let terminal_retirement_records = inventory_at_restart
        .retirement_records
        .checked_add(2)
        .ok_or_else(|| "encrypted terminal retirement record count overflow".to_owned())?;
    let terminal_persistent_bytes = encrypted_persistent_inventory_bytes(
        EncryptedPrivatePersistentInventory {
            nonce_journal_records: fresh_nonce_journal_records,
            retirement_records: terminal_retirement_records,
        },
    )?;
    let retirement_terminal_window_bytes = checked_private_storage_sum(
        "encrypted retirement terminal storage overflow",
        &[terminal_persistent_bytes, output_bytes],
    )?;

    let required_bytes = restart_transcode_window_bytes
        .max(output_window_bytes)
        .max(retirement_prepared_window_bytes)
        .max(retirement_terminal_window_bytes);

    Ok(EncryptedCrashResumeStoragePlan {
        output_bytes,
        persistent_before_fresh_lease_bytes,
        persistent_after_fresh_lease_bytes,
        durable_restart_stage_bytes,
        stage_manifest_bytes,
        retained_encrypted_descriptor_bytes,
        post_preflight_working_bytes,
        restart_transcode_window_bytes,
        output_window_bytes,
        retirement_prepared_window_bytes,
        retirement_terminal_window_bytes,
        required_bytes,
    })
}

fn unified_encrypted_private_storage_plan(
    object_count: usize,
    output_bytes: u64,
    spill_limits: super::BoundedSpillSortLimits,
    inventory_before_operation: EncryptedPrivatePersistentInventory,
) -> super::CandidateResult<UnifiedEncryptedPrivateStoragePlan> {
    let normal = encrypted_normal_publication_storage_plan(
        object_count,
        output_bytes,
        spill_limits,
        inventory_before_operation,
    )?;
    let restart_inventory = EncryptedPrivatePersistentInventory {
        nonce_journal_records: inventory_before_operation
            .nonce_journal_records
            .checked_add(1)
            .ok_or_else(|| "encrypted restart inventory nonce count overflow".to_owned())?,
        retirement_records: inventory_before_operation.retirement_records,
    };
    let crash_resume = encrypted_crash_resume_storage_plan(
        object_count,
        output_bytes,
        spill_limits,
        restart_inventory,
    )?;
    let required_bytes = normal.required_bytes.max(crash_resume.required_bytes);
    Ok(UnifiedEncryptedPrivateStoragePlan {
        normal,
        crash_resume,
        required_bytes,
    })
}

fn enforce_unified_encrypted_private_storage_limit(
    object_count: usize,
    output_bytes: u64,
    spill_limits: super::BoundedSpillSortLimits,
    inventory_before_operation: EncryptedPrivatePersistentInventory,
    max_private_storage_bytes: u64,
) -> super::CandidateResult<UnifiedEncryptedPrivateStoragePlan> {
    let plan = unified_encrypted_private_storage_plan(
        object_count,
        output_bytes,
        spill_limits,
        inventory_before_operation,
    )?;
    if plan.required_bytes > max_private_storage_bytes {
        return Err("unified encrypted private storage limit".into());
    }
    Ok(plan)
}
