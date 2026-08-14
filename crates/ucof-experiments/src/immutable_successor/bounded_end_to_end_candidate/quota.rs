#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrivateStoragePlan {
    descriptor_bytes: u64,
    locator_bytes: u64,
    leaf_ref_bytes: u64,
    sorter_plus_descriptor_bytes: u64,
    descriptor_plus_locator_bytes: u64,
    locator_plus_leaf_ref_bytes: u64,
    max_adjacent_page_ref_bytes: u64,
    required_bytes: u64,
}

fn checked_stage_bytes(records: usize, record_bytes: usize) -> CandidateResult<u64> {
    let records = u64::try_from(records).map_err(|_| "private stage record count".to_owned())?;
    let record_bytes =
        u64::try_from(record_bytes).map_err(|_| "private stage record width".to_owned())?;
    records
        .checked_mul(record_bytes)
        .ok_or_else(|| "private stage byte overflow".to_owned())
}

fn private_storage_plan(
    object_count: usize,
    spill_limits: BoundedSpillSortLimits,
) -> CandidateResult<PrivateStoragePlan> {
    if object_count == 0 {
        return Err("private storage object count".into());
    }

    let descriptor_bytes = checked_stage_bytes(object_count, DESCRIPTOR_STAGE_BYTES)?;
    let locator_bytes = checked_stage_bytes(object_count, LOCATOR_STAGE_BYTES)?;
    let leaf_ref_records = groups(object_count, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY)?.len();
    let leaf_ref_bytes = checked_stage_bytes(leaf_ref_records, PAGE_REF_STAGE_BYTES)?;

    let sorter_plus_descriptor_bytes = spill_limits
        .max_live_spill_bytes
        .checked_add(descriptor_bytes)
        .ok_or_else(|| "private sorter overlap overflow".to_owned())?;
    let descriptor_plus_locator_bytes = descriptor_bytes
        .checked_add(locator_bytes)
        .ok_or_else(|| "private descriptor locator overlap overflow".to_owned())?;
    let locator_plus_leaf_ref_bytes = locator_bytes
        .checked_add(leaf_ref_bytes)
        .ok_or_else(|| "private locator leaf overlap overflow".to_owned())?;

    let mut max_adjacent_page_ref_bytes = leaf_ref_bytes;
    let mut current_records = leaf_ref_records;
    while current_records > 1 {
        let next_records = groups(
            current_records,
            INTERNAL_FANOUT,
            INTERNAL_MIN_OCCUPANCY,
        )?
        .len();
        let current_bytes = checked_stage_bytes(current_records, PAGE_REF_STAGE_BYTES)?;
        let next_bytes = checked_stage_bytes(next_records, PAGE_REF_STAGE_BYTES)?;
        max_adjacent_page_ref_bytes = max_adjacent_page_ref_bytes.max(
            current_bytes
                .checked_add(next_bytes)
                .ok_or_else(|| "private page-ref overlap overflow".to_owned())?,
        );
        current_records = next_records;
    }

    let required_bytes = sorter_plus_descriptor_bytes
        .max(descriptor_plus_locator_bytes)
        .max(locator_plus_leaf_ref_bytes)
        .max(max_adjacent_page_ref_bytes);

    Ok(PrivateStoragePlan {
        descriptor_bytes,
        locator_bytes,
        leaf_ref_bytes,
        sorter_plus_descriptor_bytes,
        descriptor_plus_locator_bytes,
        locator_plus_leaf_ref_bytes,
        max_adjacent_page_ref_bytes,
        required_bytes,
    })
}

fn enforce_private_storage_limit(
    object_count: usize,
    spill_limits: BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
) -> CandidateResult<PrivateStoragePlan> {
    let plan = private_storage_plan(object_count, spill_limits)?;
    if plan.required_bytes > max_private_storage_bytes {
        return Err("private storage limit".into());
    }
    Ok(plan)
}

fn write_genesis_sources_with_private_quota_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    spill_limits: BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
) -> CandidateResult<(PrivateStoragePlan, EndToEndEvidence)>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
    let plan = enforce_private_storage_limit(
        sources.len(),
        spill_limits,
        max_private_storage_bytes,
    )?;
    let evidence = write_genesis_sources_end_to_end_bounded_candidate(
        writer,
        sources,
        directory,
        options,
        limits,
        spill_limits,
    )?;
    Ok((plan, evidence))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EncryptedPrivateStoragePlan {
    base: PrivateStoragePlan,
    encrypted_descriptor_bytes: u64,
    plaintext_plus_encrypted_descriptor_bytes: u64,
    encrypted_descriptor_plus_locator_bytes: u64,
    required_bytes: u64,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn encrypted_private_storage_plan(
    object_count: usize,
    spill_limits: BoundedSpillSortLimits,
) -> CandidateResult<EncryptedPrivateStoragePlan> {
    let base = private_storage_plan(object_count, spill_limits)?;
    let encrypted_descriptor_bytes =
        checked_stage_bytes(object_count, ENCRYPTED_DESCRIPTOR_STAGE_BYTES)?;
    let plaintext_plus_encrypted_descriptor_bytes = base
        .descriptor_bytes
        .checked_add(encrypted_descriptor_bytes)
        .ok_or_else(|| "private descriptor encryption overlap overflow".to_owned())?;
    let encrypted_descriptor_plus_locator_bytes = encrypted_descriptor_bytes
        .checked_add(base.locator_bytes)
        .ok_or_else(|| "private encrypted descriptor locator overlap overflow".to_owned())?;
    let required_bytes = base
        .required_bytes
        .max(plaintext_plus_encrypted_descriptor_bytes)
        .max(encrypted_descriptor_plus_locator_bytes);
    Ok(EncryptedPrivateStoragePlan {
        base,
        encrypted_descriptor_bytes,
        plaintext_plus_encrypted_descriptor_bytes,
        encrypted_descriptor_plus_locator_bytes,
        required_bytes,
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn enforce_encrypted_private_storage_limit(
    object_count: usize,
    spill_limits: BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
) -> CandidateResult<EncryptedPrivateStoragePlan> {
    let plan = encrypted_private_storage_plan(object_count, spill_limits)?;
    if plan.required_bytes > max_private_storage_bytes {
        return Err("private storage limit".into());
    }
    Ok(plan)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn write_genesis_sources_with_encrypted_descriptor_private_quota_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    spill_limits: BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
    session: &mut DescriptorEncryptionSession,
) -> CandidateResult<(EncryptedPrivateStoragePlan, EndToEndEvidence)>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
    let plan = enforce_encrypted_private_storage_limit(
        sources.len(),
        spill_limits,
        max_private_storage_bytes,
    )?;
    let evidence = write_genesis_sources_end_to_end_encrypted_descriptor_candidate(
        writer,
        sources,
        directory,
        options,
        limits,
        spill_limits,
        session,
    )?;
    Ok((plan, evidence))
}
