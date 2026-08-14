fn encrypted_retirement_inventory_name(name: &OsStr) -> bool {
    let bytes = name.as_bytes();
    bytes.starts_with(ENCRYPTED_RETIREMENT_PREFIX.as_bytes())
        && bytes.ends_with(ENCRYPTED_RETIREMENT_SUFFIX.as_bytes())
}

fn scan_encrypted_private_persistent_inventory(
    journal: &LinuxDurableNonceJournal,
) -> super::CandidateResult<EncryptedPrivatePersistentInventory> {
    let recovery = journal.scan(None).map_err(|error| error.to_string())?;
    let mut directory_entries = 0usize;
    let mut retirement_records = 0usize;
    for entry in std::fs::read_dir(linux_nonce_procfd_directory(&journal.directory))
        .map_err(|error| error.to_string())?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        directory_entries = directory_entries
            .checked_add(1)
            .ok_or_else(|| "encrypted private inventory directory entries".to_owned())?;
        if directory_entries > journal.limits.max_directory_entries {
            return Err("encrypted private inventory directory entry limit".into());
        }
        let name = entry.file_name();
        if !encrypted_retirement_inventory_name(&name) {
            continue;
        }
        let mut file = linux_nonce_open_relative_readonly(&journal.directory, &name)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "encrypted retirement inventory entry disappeared".to_owned())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file()
            || metadata.len()
                != u64::try_from(ENCRYPTED_RETIREMENT_BYTES).expect("retirement width fits u64")
        {
            return Err("encrypted retirement inventory file shape".into());
        }
        let mut sealed = [0u8; ENCRYPTED_RETIREMENT_BYTES];
        file.read_exact(&mut sealed)
            .map_err(|error| error.to_string())?;
        let mut trailing = [0u8; 1];
        if file.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
            return Err("encrypted retirement inventory exact end".into());
        }
        let record = open_encrypted_retirement_record(journal, &sealed)?;
        if record.key_id != journal.key_id || record.nonce_prefix != journal.nonce_prefix {
            return Err("encrypted retirement inventory context".into());
        }
        retirement_records = retirement_records
            .checked_add(1)
            .ok_or_else(|| "encrypted retirement inventory record count".to_owned())?;
    }
    Ok(EncryptedPrivatePersistentInventory {
        nonce_journal_records: recovery.generations,
        retirement_records,
    })
}

#[derive(Clone, Copy)]
struct EncryptedRestartPublicationQuotaSettings {
    continuation: EncryptedRestartContinuationSettings,
    spill_limits: super::BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
}

fn stage_and_publish_verified_encrypted_restart_with_private_quota<B, S>(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    backend: &mut B,
    sources: &mut [S],
    settings: EncryptedRestartPublicationQuotaSettings,
) -> super::CandidateResult<(EncryptedCrashResumeStoragePlan, EncryptedRestartPublicationOutcome)>
where
    B: super::PersistentStagingBackend,
    S: super::ImmutableStreamingPayloadSource,
{
    let inventory = scan_encrypted_private_persistent_inventory(journal)?;
    let output_bytes = super::expected_canonical_output_bytes(sources, settings.continuation.limits)?;
    let plan = encrypted_crash_resume_storage_plan(
        sources.len(),
        output_bytes,
        settings.spill_limits,
        inventory,
    )?;
    if plan.required_bytes > settings.max_private_storage_bytes {
        return Err("encrypted restart private storage limit".into());
    }
    let outcome = stage_and_publish_verified_encrypted_restart(
        journal,
        stage_directory_path,
        work_directory,
        backend,
        sources,
        settings.continuation,
    )?;
    Ok((plan, outcome))
}
