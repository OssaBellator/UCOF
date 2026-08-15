#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompactedPersistentInventory {
    nonce_records: usize,
    checkpoint_records: usize,
    retirement_records: usize,
    source_set_records: usize,
    total_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompactionStoragePlan {
    existing_persistent_bytes: u64,
    new_checkpoint_bytes: u64,
    required_before_prune: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompactedCrashResumeStoragePlan {
    base_without_existing_inventory: EncryptedCrashResumeStoragePlan,
    persistent_bytes: u64,
    required_bytes: u64,
}

fn scan_compacted_persistent_inventory(
    journal: &LinuxDurableNonceJournal,
) -> super::CandidateResult<CompactedPersistentInventory> {
    let mut nonce_records = 0usize;
    let mut checkpoint_records = 0usize;
    let mut retirement_records = 0usize;
    let mut source_set_records = 0usize;
    let mut total_bytes = 0u64;
    let mut directory_entries = 0usize;
    let mut saw_authenticated_checkpoint = false;
    let mut saw_unrecognized_entry = false;
    let directory_scan_ceiling = compacted_directory_scan_ceiling(journal)?;

    for entry in std::fs::read_dir(linux_nonce_procfd_directory(&journal.directory))
        .map_err(|error| error.to_string())?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        directory_entries = directory_entries
            .checked_add(1)
            .ok_or_else(|| "compacted inventory directory entries".to_owned())?;
        if directory_entries > directory_scan_ceiling {
            return Err("compacted inventory directory entry limit".into());
        }
        let name = entry.file_name();

        if let Some(generation) = linux_nonce_parse_generation_name(&name) {
            let file = linux_nonce_open_relative_readonly(&journal.directory, &name)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "compacted inventory nonce disappeared".to_owned())?;
            let metadata = file.metadata().map_err(|error| error.to_string())?;
            if !metadata.file_type().is_file()
                || metadata.len()
                    != u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("nonce record width")
            {
                return Err("compacted inventory nonce file shape".into());
            }
            let sealed = linux_nonce_read_exact_file(file).map_err(|error| error.to_string())?;
            let record = journal.open_record(&sealed).map_err(|error| error.to_string())?;
            if record.generation != generation
                || record.key_id != journal.key_id
                || record.nonce_prefix != journal.nonce_prefix
            {
                return Err("compacted inventory nonce context".into());
            }
            nonce_records = nonce_records
                .checked_add(1)
                .ok_or_else(|| "compacted inventory nonce count".to_owned())?;
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| "compacted inventory byte overflow".to_owned())?;
            continue;
        }

        if let Some(generation) = parse_nonce_compaction_name(&name) {
            let checkpoint = load_nonce_compaction_checkpoint(journal, generation)?
                .ok_or_else(|| "compacted inventory checkpoint disappeared".to_owned())?;
            if checkpoint.generation != generation {
                return Err("compacted inventory checkpoint generation".into());
            }
            saw_authenticated_checkpoint = true;
            checkpoint_records = checkpoint_records
                .checked_add(1)
                .ok_or_else(|| "compacted inventory checkpoint count".to_owned())?;
            total_bytes = total_bytes
                .checked_add(u64::try_from(NONCE_COMPACTION_BYTES).expect("checkpoint width"))
                .ok_or_else(|| "compacted inventory byte overflow".to_owned())?;
            continue;
        }

        if let Some((generation, role)) = parse_compaction_stage_manifest_name(&name) {
            let manifest = load_encrypted_stage_manifest(journal, generation, role)
                .map_err(|error| match error {
                    LinuxEncryptedStageRestartError::ForeignKey
                    | LinuxEncryptedStageRestartError::ForeignNoncePrefix
                    | LinuxEncryptedStageRestartError::ForeignGeneration => {
                        "compacted inventory stage manifest context".to_owned()
                    }
                    _ => error.to_string(),
                })?
                .ok_or_else(|| "compacted inventory stage manifest disappeared".to_owned())?;
            if manifest.generation != generation
                || manifest.role != role
                || manifest.key_id != journal.key_id
                || manifest.nonce_prefix != journal.nonce_prefix
            {
                return Err("compacted inventory stage manifest context".into());
            }
            continue;
        }

        let name_bytes = name.as_bytes();
        if name_bytes.starts_with(ENCRYPTED_RETIREMENT_PREFIX.as_bytes())
            && name_bytes.ends_with(ENCRYPTED_RETIREMENT_SUFFIX.as_bytes())
        {
            let mut file = linux_nonce_open_relative_readonly(&journal.directory, &name)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "compacted inventory retirement disappeared".to_owned())?;
            let metadata = file.metadata().map_err(|error| error.to_string())?;
            if !metadata.file_type().is_file()
                || metadata.len()
                    != u64::try_from(ENCRYPTED_RETIREMENT_BYTES).expect("retirement width")
            {
                return Err("compacted inventory retirement file shape".into());
            }
            let mut sealed = [0u8; ENCRYPTED_RETIREMENT_BYTES];
            file.read_exact(&mut sealed).map_err(|error| error.to_string())?;
            let mut trailing = [0u8; 1];
            if file.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
                return Err("compacted inventory retirement exact end".into());
            }
            let record = open_encrypted_retirement_record(journal, &sealed)?;
            if record.key_id != journal.key_id
                || record.nonce_prefix != journal.nonce_prefix
                || encrypted_retirement_name(
                    record.crashed_generation,
                    record.fresh_generation,
                    record.state,
                ) != name
            {
                return Err("compacted inventory retirement context".into());
            }
            retirement_records = retirement_records
                .checked_add(1)
                .ok_or_else(|| "compacted inventory retirement count".to_owned())?;
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| "compacted inventory byte overflow".to_owned())?;
            continue;
        }

        if name_bytes.starts_with(RESTART_SOURCE_SET_PREFIX.as_bytes())
            && name_bytes.ends_with(RESTART_SOURCE_SET_SUFFIX.as_bytes())
        {
            let mut file = linux_nonce_open_relative_readonly(&journal.directory, &name)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "compacted inventory source-set disappeared".to_owned())?;
            let metadata = file.metadata().map_err(|error| error.to_string())?;
            if !metadata.file_type().is_file()
                || metadata.len()
                    != u64::try_from(RESTART_SOURCE_SET_BYTES).expect("source-set width")
            {
                return Err("compacted inventory source-set file shape".into());
            }
            let mut sealed = [0u8; RESTART_SOURCE_SET_BYTES];
            file.read_exact(&mut sealed).map_err(|error| error.to_string())?;
            let mut trailing = [0u8; 1];
            if file.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
                return Err("compacted inventory source-set exact end".into());
            }
            let record = open_restart_source_set_authority(journal, &sealed)?;
            if record.key_id != journal.key_id
                || record.nonce_prefix != journal.nonce_prefix
                || restart_source_set_authority_name(record.generation, record.role) != name
            {
                return Err("compacted inventory source-set context".into());
            }
            source_set_records = source_set_records
                .checked_add(1)
                .ok_or_else(|| "compacted inventory source-set count".to_owned())?;
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| "compacted inventory byte overflow".to_owned())?;
            continue;
        }

        saw_unrecognized_entry = true;
    }
    if saw_unrecognized_entry {
        return Err("compacted inventory unrecognized entry".into());
    }
    validate_compacted_directory_entry_count(
        journal,
        directory_entries,
        saw_authenticated_checkpoint,
        false,
        "compacted inventory",
    )?;

    Ok(CompactedPersistentInventory {
        nonce_records,
        checkpoint_records,
        retirement_records,
        source_set_records,
        total_bytes,
    })
}

fn compaction_storage_plan(
    journal: &LinuxDurableNonceJournal,
) -> super::CandidateResult<CompactionStoragePlan> {
    let inventory = scan_compacted_persistent_inventory(journal)?;
    let recovery = CompactedNonceJournal::new(journal).scan(None)?;
    if recovery.durable.generation == 0 {
        return Err("compaction storage plan requires durable generation".into());
    }
    let checkpoint_exists = load_nonce_compaction_checkpoint(journal, recovery.durable.generation)?
        .is_some();
    let new_checkpoint_bytes = if checkpoint_exists {
        0
    } else {
        u64::try_from(NONCE_COMPACTION_BYTES).expect("checkpoint width")
    };
    let required_before_prune = inventory
        .total_bytes
        .checked_add(new_checkpoint_bytes)
        .ok_or_else(|| "compaction storage plan overflow".to_owned())?;
    Ok(CompactionStoragePlan {
        existing_persistent_bytes: inventory.total_bytes,
        new_checkpoint_bytes,
        required_before_prune,
    })
}

fn compact_restart_metadata_with_private_quota(
    journal: &LinuxDurableNonceJournal,
    trusted_floor: Option<TrustedNonceFloor>,
    max_private_metadata_bytes: u64,
    cut: RestartMetadataCompactionCut,
) -> super::CandidateResult<(CompactionStoragePlan, RestartMetadataCompactionReport)> {
    let plan = compaction_storage_plan(journal)?;
    if plan.required_before_prune > max_private_metadata_bytes {
        return Err("restart metadata compaction private storage limit".into());
    }
    let report = compact_restart_metadata(journal, trusted_floor, cut)?;
    Ok((plan, report))
}

fn compacted_crash_resume_storage_plan(
    object_count: usize,
    output_bytes: u64,
    spill_limits: super::BoundedSpillSortLimits,
    inventory: CompactedPersistentInventory,
) -> super::CandidateResult<CompactedCrashResumeStoragePlan> {
    let zero_inventory = EncryptedPrivatePersistentInventory {
        nonce_journal_records: 0,
        retirement_records: 0,
    };
    let (base, _) = consolidated_encrypted_tree_crash_resume_lifecycle_plan(
        object_count,
        output_bytes,
        spill_limits,
        zero_inventory,
    )?;
    let required_bytes = base
        .required_bytes
        .checked_add(inventory.total_bytes)
        .ok_or_else(|| "compacted crash-resume storage overflow".to_owned())?;
    Ok(CompactedCrashResumeStoragePlan {
        base_without_existing_inventory: base,
        persistent_bytes: inventory.total_bytes,
        required_bytes,
    })
}

#[derive(Clone, Copy)]
struct CompactedSourceBoundRestartQuotaSettings {
    continuation: EncryptedRestartContinuationSettings,
    spill_limits: super::BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
    source_set_id: [u8; 32],
}

fn enforce_compacted_source_bound_restart_private_quota<S>(
    journal: &LinuxDurableNonceJournal,
    sources: &mut [S],
    settings: CompactedSourceBoundRestartQuotaSettings,
) -> super::CandidateResult<CompactedCrashResumeStoragePlan>
where
    S: super::ImmutableStreamingPayloadSource,
{
    if settings.source_set_id == [0; 32] {
        return Err("compacted restart source-set identity".into());
    }
    let inventory = scan_compacted_persistent_inventory(journal)?;
    let output_bytes = super::expected_canonical_output_bytes(sources, settings.continuation.limits)?;
    let plan = compacted_crash_resume_storage_plan(
        sources.len(),
        output_bytes,
        settings.spill_limits,
        inventory,
    )?;
    if plan.required_bytes > settings.max_private_storage_bytes {
        return Err("compacted source-bound restart private storage limit".into());
    }
    Ok(plan)
}

fn continue_compacted_source_bound_restart_with_private_quota<W, S>(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    writer: &mut W,
    sources: &mut [S],
    settings: CompactedSourceBoundRestartQuotaSettings,
) -> super::CandidateResult<(
    CompactedCrashResumeStoragePlan,
    EncryptedTreeRestartContinuationEvidence,
)>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
{
    let plan = enforce_compacted_source_bound_restart_private_quota(journal, sources, settings)?;
    let evidence = continue_compacted_source_bound_encrypted_tree_restart(
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

fn stage_and_publish_compacted_source_bound_restart_with_private_quota<B, S>(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    backend: &mut B,
    sources: &mut [S],
    settings: CompactedSourceBoundRestartQuotaSettings,
) -> super::CandidateResult<(
    CompactedCrashResumeStoragePlan,
    EncryptedTreeRestartPublicationOutcome,
)>
where
    B: super::PersistentStagingBackend,
    S: super::ImmutableStreamingPayloadSource,
{
    let plan = enforce_compacted_source_bound_restart_private_quota(journal, sources, settings)?;
    let outcome = stage_and_publish_compacted_source_bound_encrypted_tree_restart(
        journal,
        stage_directory_path,
        work_directory,
        backend,
        sources,
        settings.source_set_id,
        settings.continuation,
    )?;
    Ok((plan, outcome))
}
