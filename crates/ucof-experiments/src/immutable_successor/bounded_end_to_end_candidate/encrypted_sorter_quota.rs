#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EncryptedSorterPrivateStoragePlan {
    encrypted_sorter_frame_bytes: u64,
    retained_descriptor_bytes: u64,
    locator_bytes: u64,
    sorter_plus_retained_descriptor_bytes: u64,
    retained_descriptor_plus_locator_bytes: u64,
    locator_plus_leaf_ref_bytes: u64,
    max_adjacent_page_ref_bytes: u64,
    required_bytes: u64,
}

fn encrypted_sorter_private_storage_plan(
    object_count: usize,
    spill_limits: BoundedSpillSortLimits,
) -> CandidateResult<EncryptedSorterPrivateStoragePlan> {
    let base = private_storage_plan(object_count, spill_limits)?;
    let encrypted_limits = encrypted_sorter_limits(spill_limits)?;
    let encrypted_sorter_frame_bytes =
        u64::try_from(ENCRYPTED_SORTER_FRAME_BYTES).expect("encrypted sorter frame fits u64");
    let retained_descriptor_bytes =
        checked_stage_bytes(object_count, ENCRYPTED_DESCRIPTOR_STAGE_BYTES)?;
    let sorter_plus_retained_descriptor_bytes = encrypted_limits
        .max_live_spill_bytes
        .checked_add(retained_descriptor_bytes)
        .ok_or_else(|| "encrypted sorter retained overlap overflow".to_owned())?;
    let retained_descriptor_plus_locator_bytes = retained_descriptor_bytes
        .checked_add(base.locator_bytes)
        .ok_or_else(|| "encrypted retained locator overlap overflow".to_owned())?;
    let required_bytes = sorter_plus_retained_descriptor_bytes
        .max(retained_descriptor_plus_locator_bytes)
        .max(base.locator_plus_leaf_ref_bytes)
        .max(base.max_adjacent_page_ref_bytes);
    Ok(EncryptedSorterPrivateStoragePlan {
        encrypted_sorter_frame_bytes,
        retained_descriptor_bytes,
        locator_bytes: base.locator_bytes,
        sorter_plus_retained_descriptor_bytes,
        retained_descriptor_plus_locator_bytes,
        locator_plus_leaf_ref_bytes: base.locator_plus_leaf_ref_bytes,
        max_adjacent_page_ref_bytes: base.max_adjacent_page_ref_bytes,
        required_bytes,
    })
}

fn enforce_encrypted_sorter_private_storage_limit(
    object_count: usize,
    spill_limits: BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
) -> CandidateResult<EncryptedSorterPrivateStoragePlan> {
    let plan = encrypted_sorter_private_storage_plan(object_count, spill_limits)?;
    if plan.required_bytes > max_private_storage_bytes {
        return Err("private storage limit".into());
    }
    Ok(plan)
}

#[derive(Clone, Copy)]
struct EncryptedSorterWriterSettings {
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    max_private_storage_bytes: u64,
}

fn write_genesis_sources_with_encrypted_sorter_private_quota_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    spill_limits: BoundedSpillSortLimits,
    settings: EncryptedSorterWriterSettings,
    spill_session: &mut DescriptorEncryptionSession,
    retained_session: &mut DescriptorEncryptionSession,
) -> CandidateResult<(EncryptedSorterPrivateStoragePlan, EndToEndEvidence)>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
    let plan = enforce_encrypted_sorter_private_storage_limit(
        sources.len(),
        spill_limits,
        settings.max_private_storage_bytes,
    )?;
    let evidence = write_genesis_sources_end_to_end_encrypted_sorter_candidate(
        writer,
        sources,
        directory,
        settings.options,
        settings.limits,
        spill_limits,
        spill_session,
        retained_session,
    )?;
    Ok((plan, evidence))
}
