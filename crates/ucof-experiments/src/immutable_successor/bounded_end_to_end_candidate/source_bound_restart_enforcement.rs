#[derive(Clone, Copy)]
struct SourceBoundRestartQuotaSettings {
    continuation: EncryptedRestartContinuationSettings,
    spill_limits: super::BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
    source_set_id: [u8; 32],
}

fn continue_source_bound_encrypted_tree_restart_with_private_quota<W, S>(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    writer: &mut W,
    sources: &mut [S],
    settings: SourceBoundRestartQuotaSettings,
) -> super::CandidateResult<(SourceBoundCrashResumeStoragePlan, EncryptedTreeRestartContinuationEvidence)>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
{
    if settings.source_set_id == [0; 32] {
        return Err("restart source-set identity".into());
    }
    let inventory = scan_source_bound_persistent_inventory(journal)?;
    let output_bytes = super::expected_canonical_output_bytes(
        sources,
        settings.continuation.limits,
    )?;
    let plan = source_bound_crash_resume_storage_plan(
        sources.len(),
        output_bytes,
        settings.spill_limits,
        inventory,
    )?;
    if plan.required_bytes > settings.max_private_storage_bytes {
        return Err("source-bound restart private storage limit".into());
    }
    let evidence = continue_source_bound_encrypted_tree_restart(
        journal,
        stage_directory_path,
        work_directory,
        writer,
        sources,
        settings.source_set_id,
        settings.continuation,
    )?;
    Ok((plan, evidence))
}
