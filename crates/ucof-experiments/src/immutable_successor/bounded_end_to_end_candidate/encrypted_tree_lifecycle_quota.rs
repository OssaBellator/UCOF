#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConsolidatedEncryptedTreeStoragePlan {
    retained_descriptor_bytes: u64,
    encrypted_locator_bytes: u64,
    first_page_ref_bytes: u64,
    retained_descriptor_plus_locator_bytes: u64,
    locator_plus_leaf_ref_bytes: u64,
    max_adjacent_page_ref_bytes: u64,
    required_post_preflight_bytes: u64,
}

fn consolidated_encrypted_tree_storage_plan(
    object_count: usize,
) -> super::CandidateResult<ConsolidatedEncryptedTreeStoragePlan> {
    if object_count == 0 {
        return Err("encrypted tree storage object count".into());
    }
    let retained_descriptor_bytes =
        super::checked_stage_bytes(object_count, ENCRYPTED_DESCRIPTOR_STAGE_BYTES)?;
    let encrypted_locator_bytes =
        super::checked_stage_bytes(object_count, ENCRYPTED_LOCATOR_STAGE_BYTES)?;
    let leaf_ref_records =
        super::groups(object_count, super::LEAF_CAPACITY, super::LEAF_MIN_OCCUPANCY)?.len();
    let first_page_ref_bytes =
        super::checked_stage_bytes(leaf_ref_records, ENCRYPTED_PAGE_REF_STAGE_BYTES)?;

    let retained_descriptor_plus_locator_bytes = retained_descriptor_bytes
        .checked_add(encrypted_locator_bytes)
        .ok_or_else(|| "encrypted retained locator overlap overflow".to_owned())?;
    let locator_plus_leaf_ref_bytes = encrypted_locator_bytes
        .checked_add(first_page_ref_bytes)
        .ok_or_else(|| "encrypted locator leaf overlap overflow".to_owned())?;

    let mut max_adjacent_page_ref_bytes = first_page_ref_bytes;
    let mut current_records = leaf_ref_records;
    while current_records > 1 {
        let next_records = super::groups(
            current_records,
            super::INTERNAL_FANOUT,
            super::INTERNAL_MIN_OCCUPANCY,
        )?
        .len();
        let current_bytes =
            super::checked_stage_bytes(current_records, ENCRYPTED_PAGE_REF_STAGE_BYTES)?;
        let next_bytes =
            super::checked_stage_bytes(next_records, ENCRYPTED_PAGE_REF_STAGE_BYTES)?;
        max_adjacent_page_ref_bytes = max_adjacent_page_ref_bytes.max(
            current_bytes
                .checked_add(next_bytes)
                .ok_or_else(|| "encrypted page-ref overlap overflow".to_owned())?,
        );
        current_records = next_records;
    }

    let required_post_preflight_bytes = retained_descriptor_plus_locator_bytes
        .max(locator_plus_leaf_ref_bytes)
        .max(max_adjacent_page_ref_bytes);

    Ok(ConsolidatedEncryptedTreeStoragePlan {
        retained_descriptor_bytes,
        encrypted_locator_bytes,
        first_page_ref_bytes,
        retained_descriptor_plus_locator_bytes,
        locator_plus_leaf_ref_bytes,
        max_adjacent_page_ref_bytes,
        required_post_preflight_bytes,
    })
}

fn consolidated_encrypted_tree_normal_lifecycle_plan(
    object_count: usize,
    output_bytes: u64,
    spill_limits: super::BoundedSpillSortLimits,
    inventory_before_operation: EncryptedPrivatePersistentInventory,
) -> super::CandidateResult<(EncryptedNormalPublicationStoragePlan, ConsolidatedEncryptedTreeStoragePlan)> {
    let mut lifecycle = encrypted_normal_publication_storage_plan(
        object_count,
        output_bytes,
        spill_limits,
        inventory_before_operation,
    )?;
    let tree = consolidated_encrypted_tree_storage_plan(object_count)?;
    lifecycle.output_window_bytes = checked_private_storage_sum(
        "consolidated encrypted publication output storage overflow",
        &[
            lifecycle.persistent_after_lease_bytes,
            lifecycle.stage_manifest_bytes,
            lifecycle.durable_restart_stage_bytes,
            output_bytes,
            tree.required_post_preflight_bytes,
        ],
    )?;
    lifecycle.required_bytes = lifecycle
        .sort_window_bytes
        .max(lifecycle.restart_copy_window_bytes)
        .max(lifecycle.restart_manifest_window_bytes)
        .max(lifecycle.restart_transcode_window_bytes)
        .max(lifecycle.output_window_bytes);
    Ok((lifecycle, tree))
}

fn consolidated_encrypted_tree_crash_resume_lifecycle_plan(
    object_count: usize,
    output_bytes: u64,
    spill_limits: super::BoundedSpillSortLimits,
    inventory_at_restart: EncryptedPrivatePersistentInventory,
) -> super::CandidateResult<(EncryptedCrashResumeStoragePlan, ConsolidatedEncryptedTreeStoragePlan)> {
    let mut lifecycle = encrypted_crash_resume_storage_plan(
        object_count,
        output_bytes,
        spill_limits,
        inventory_at_restart,
    )?;
    let tree = consolidated_encrypted_tree_storage_plan(object_count)?;
    lifecycle.post_preflight_working_bytes = tree.required_post_preflight_bytes;
    lifecycle.output_window_bytes = checked_private_storage_sum(
        "consolidated encrypted crash-resume output storage overflow",
        &[
            lifecycle.persistent_after_fresh_lease_bytes,
            lifecycle.durable_restart_stage_bytes,
            lifecycle.stage_manifest_bytes,
            output_bytes,
            tree.required_post_preflight_bytes,
        ],
    )?;
    lifecycle.required_bytes = lifecycle
        .restart_transcode_window_bytes
        .max(lifecycle.output_window_bytes)
        .max(lifecycle.retirement_prepared_window_bytes)
        .max(lifecycle.retirement_terminal_window_bytes);
    Ok((lifecycle, tree))
}
