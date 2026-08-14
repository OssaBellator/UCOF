#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceBoundPersistentInventory {
    base: EncryptedPrivatePersistentInventory,
    source_set_records: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceBoundNormalStoragePlan {
    base: EncryptedNormalPublicationStoragePlan,
    source_set_record_bytes: u64,
    source_authority_window_bytes: u64,
    restart_transcode_window_bytes: u64,
    output_window_bytes: u64,
    required_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceBoundCrashResumeStoragePlan {
    base: EncryptedCrashResumeStoragePlan,
    existing_source_set_bytes: u64,
    restart_transcode_window_bytes: u64,
    output_window_bytes: u64,
    retirement_prepared_window_bytes: u64,
    retirement_terminal_window_bytes: u64,
    required_bytes: u64,
}

fn source_set_record_storage_bytes(record_count: usize) -> super::CandidateResult<u64> {
    super::checked_stage_bytes(record_count, RESTART_SOURCE_SET_BYTES)
}

fn scan_source_bound_persistent_inventory(
    journal: &LinuxDurableNonceJournal,
) -> super::CandidateResult<SourceBoundPersistentInventory> {
    let base = scan_encrypted_private_persistent_inventory(journal)?;
    let mut directory_entries = 0usize;
    let mut source_set_records = 0usize;
    for entry in std::fs::read_dir(linux_nonce_procfd_directory(&journal.directory))
        .map_err(|error| error.to_string())?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        directory_entries = directory_entries
            .checked_add(1)
            .ok_or_else(|| "source-bound inventory directory entries".to_owned())?;
        if directory_entries > journal.limits.max_directory_entries {
            return Err("source-bound inventory directory entry limit".into());
        }
        let name = entry.file_name();
        let bytes = name.as_bytes();
        if !bytes.starts_with(RESTART_SOURCE_SET_PREFIX.as_bytes())
            || !bytes.ends_with(RESTART_SOURCE_SET_SUFFIX.as_bytes())
        {
            continue;
        }
        let mut file = linux_nonce_open_relative_readonly(&journal.directory, &name)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "source-set inventory entry disappeared".to_owned())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file()
            || metadata.len()
                != u64::try_from(RESTART_SOURCE_SET_BYTES).expect("source-set width fits u64")
        {
            return Err("source-set inventory file shape".into());
        }
        let mut sealed = [0u8; RESTART_SOURCE_SET_BYTES];
        file.read_exact(&mut sealed).map_err(|error| error.to_string())?;
        let mut trailing = [0u8; 1];
        if file.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
            return Err("source-set inventory exact end".into());
        }
        let record = open_restart_source_set_authority(journal, &sealed)?;
        if record.key_id != journal.key_id || record.nonce_prefix != journal.nonce_prefix {
            return Err("source-set inventory journal context".into());
        }
        if restart_source_set_authority_name(record.generation, record.role) != name {
            return Err("source-set inventory canonical name".into());
        }
        source_set_records = source_set_records
            .checked_add(1)
            .ok_or_else(|| "source-set inventory record count".to_owned())?;
    }
    Ok(SourceBoundPersistentInventory {
        base,
        source_set_records,
    })
}

fn source_bound_normal_storage_plan(
    object_count: usize,
    output_bytes: u64,
    spill_limits: super::BoundedSpillSortLimits,
    inventory_before_operation: SourceBoundPersistentInventory,
) -> super::CandidateResult<SourceBoundNormalStoragePlan> {
    let mut base = encrypted_normal_publication_storage_plan(
        object_count,
        output_bytes,
        spill_limits,
        inventory_before_operation.base,
    )?;
    let existing_source_set_bytes =
        source_set_record_storage_bytes(inventory_before_operation.source_set_records)?;
    let source_set_record_bytes =
        u64::try_from(RESTART_SOURCE_SET_BYTES).expect("source-set width fits u64");
    let source_authority_window_bytes = checked_private_storage_sum(
        "source-bound authority storage overflow",
        &[
            base.restart_manifest_window_bytes,
            existing_source_set_bytes,
            source_set_record_bytes,
        ],
    )?;
    let restart_transcode_window_bytes = checked_private_storage_sum(
        "source-bound transcode storage overflow",
        &[
            base.restart_transcode_window_bytes,
            existing_source_set_bytes,
            source_set_record_bytes,
        ],
    )?;
    let output_window_bytes = checked_private_storage_sum(
        "source-bound output storage overflow",
        &[
            base.output_window_bytes,
            existing_source_set_bytes,
            source_set_record_bytes,
        ],
    )?;
    base.required_bytes = base
        .required_bytes
        .max(source_authority_window_bytes)
        .max(restart_transcode_window_bytes)
        .max(output_window_bytes);
    Ok(SourceBoundNormalStoragePlan {
        base,
        source_set_record_bytes,
        source_authority_window_bytes,
        restart_transcode_window_bytes,
        output_window_bytes,
        required_bytes: base.required_bytes,
    })
}

fn source_bound_crash_resume_storage_plan(
    object_count: usize,
    output_bytes: u64,
    spill_limits: super::BoundedSpillSortLimits,
    inventory_at_restart: SourceBoundPersistentInventory,
) -> super::CandidateResult<SourceBoundCrashResumeStoragePlan> {
    let mut base = encrypted_crash_resume_storage_plan(
        object_count,
        output_bytes,
        spill_limits,
        inventory_at_restart.base,
    )?;
    let existing_source_set_bytes =
        source_set_record_storage_bytes(inventory_at_restart.source_set_records)?;
    let restart_transcode_window_bytes = base
        .restart_transcode_window_bytes
        .checked_add(existing_source_set_bytes)
        .ok_or_else(|| "source-bound crash transcode storage overflow".to_owned())?;
    let output_window_bytes = base
        .output_window_bytes
        .checked_add(existing_source_set_bytes)
        .ok_or_else(|| "source-bound crash output storage overflow".to_owned())?;
    let retirement_prepared_window_bytes = base
        .retirement_prepared_window_bytes
        .checked_add(existing_source_set_bytes)
        .ok_or_else(|| "source-bound prepared retirement storage overflow".to_owned())?;
    let retirement_terminal_window_bytes = base
        .retirement_terminal_window_bytes
        .checked_add(existing_source_set_bytes)
        .ok_or_else(|| "source-bound terminal retirement storage overflow".to_owned())?;
    base.required_bytes = base
        .required_bytes
        .max(restart_transcode_window_bytes)
        .max(output_window_bytes)
        .max(retirement_prepared_window_bytes)
        .max(retirement_terminal_window_bytes);
    Ok(SourceBoundCrashResumeStoragePlan {
        base,
        existing_source_set_bytes,
        restart_transcode_window_bytes,
        output_window_bytes,
        retirement_prepared_window_bytes,
        retirement_terminal_window_bytes,
        required_bytes: base.required_bytes,
    })
}
