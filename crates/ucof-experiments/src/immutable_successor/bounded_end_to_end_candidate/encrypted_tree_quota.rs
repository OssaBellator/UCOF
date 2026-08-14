#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EncryptedTreePrivateStoragePlan {
    encrypted_sorter_frame_bytes: u64,
    retained_descriptor_bytes: u64,
    encrypted_locator_bytes: u64,
    first_page_ref_bytes: u64,
    sorter_plus_retained_descriptor_bytes: u64,
    retained_descriptor_plus_locator_bytes: u64,
    locator_plus_leaf_ref_bytes: u64,
    max_adjacent_page_ref_bytes: u64,
    required_bytes: u64,
}

fn encrypted_tree_private_storage_plan(
    object_count: usize,
    spill_limits: BoundedSpillSortLimits,
) -> CandidateResult<EncryptedTreePrivateStoragePlan> {
    let sorter = encrypted_sorter_private_storage_plan(object_count, spill_limits)?;
    let encrypted_locator_bytes = checked_stage_bytes(object_count, ENCRYPTED_LOCATOR_STAGE_BYTES)?;
    let leaf_ref_records = groups(object_count, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY)?.len();
    let first_page_ref_bytes =
        checked_stage_bytes(leaf_ref_records, ENCRYPTED_PAGE_REF_STAGE_BYTES)?;

    let retained_descriptor_plus_locator_bytes = sorter
        .retained_descriptor_bytes
        .checked_add(encrypted_locator_bytes)
        .ok_or_else(|| "encrypted retained locator overlap overflow".to_owned())?;
    let locator_plus_leaf_ref_bytes = encrypted_locator_bytes
        .checked_add(first_page_ref_bytes)
        .ok_or_else(|| "encrypted locator leaf overlap overflow".to_owned())?;

    let mut max_adjacent_page_ref_bytes = first_page_ref_bytes;
    let mut current_records = leaf_ref_records;
    while current_records > 1 {
        let next_records = groups(
            current_records,
            INTERNAL_FANOUT,
            INTERNAL_MIN_OCCUPANCY,
        )?
        .len();
        let current_bytes =
            checked_stage_bytes(current_records, ENCRYPTED_PAGE_REF_STAGE_BYTES)?;
        let next_bytes = checked_stage_bytes(next_records, ENCRYPTED_PAGE_REF_STAGE_BYTES)?;
        max_adjacent_page_ref_bytes = max_adjacent_page_ref_bytes.max(
            current_bytes
                .checked_add(next_bytes)
                .ok_or_else(|| "encrypted page-ref overlap overflow".to_owned())?,
        );
        current_records = next_records;
    }

    let required_bytes = sorter
        .sorter_plus_retained_descriptor_bytes
        .max(retained_descriptor_plus_locator_bytes)
        .max(locator_plus_leaf_ref_bytes)
        .max(max_adjacent_page_ref_bytes);

    Ok(EncryptedTreePrivateStoragePlan {
        encrypted_sorter_frame_bytes: sorter.encrypted_sorter_frame_bytes,
        retained_descriptor_bytes: sorter.retained_descriptor_bytes,
        encrypted_locator_bytes,
        first_page_ref_bytes,
        sorter_plus_retained_descriptor_bytes: sorter.sorter_plus_retained_descriptor_bytes,
        retained_descriptor_plus_locator_bytes,
        locator_plus_leaf_ref_bytes,
        max_adjacent_page_ref_bytes,
        required_bytes,
    })
}

fn enforce_encrypted_tree_private_storage_limit(
    object_count: usize,
    spill_limits: BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
) -> CandidateResult<EncryptedTreePrivateStoragePlan> {
    let plan = encrypted_tree_private_storage_plan(object_count, spill_limits)?;
    if plan.required_bytes > max_private_storage_bytes {
        return Err("private storage limit".into());
    }
    Ok(plan)
}

#[derive(Clone, Copy)]
struct EncryptedTreeWriterSettings {
    pipeline: EncryptedSorterPipelineSettings,
    max_private_storage_bytes: u64,
}

fn write_genesis_sources_with_encrypted_tree_private_quota_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    settings: EncryptedTreeWriterSettings,
    spill_session: &mut DescriptorEncryptionSession,
    retained_session: &mut DescriptorEncryptionSession,
    tree_session: &mut DescriptorEncryptionSession,
) -> CandidateResult<(EncryptedTreePrivateStoragePlan, EncryptedTreeEndToEndEvidence)>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
    let plan = enforce_encrypted_tree_private_storage_limit(
        sources.len(),
        settings.pipeline.spill_limits,
        settings.max_private_storage_bytes,
    )?;
    let evidence = write_genesis_sources_end_to_end_encrypted_tree_candidate(
        writer,
        sources,
        directory,
        settings.pipeline,
        spill_session,
        retained_session,
        tree_session,
    )?;
    Ok((plan, evidence))
}
