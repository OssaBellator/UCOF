fn require_linux_nonce_journal_metadata_slots(
    journal: &LinuxDurableNonceJournal,
    additional_entries: usize,
    label: &'static str,
) -> super::CandidateResult<()> {
    if additional_entries == 0 {
        return Ok(());
    }
    linux_nonce_verify_procfd_directory(&journal.directory)
        .map_err(|error| error.to_string())?;
    let mut entries = 0usize;
    for entry in std::fs::read_dir(linux_nonce_procfd_directory(&journal.directory))
        .map_err(|error| error.to_string())?
    {
        entry.map_err(|error| error.to_string())?;
        entries = entries
            .checked_add(1)
            .ok_or_else(|| format!("{label} directory entries"))?;
        if entries > journal.limits.max_directory_entries {
            return Err(format!("{label} directory entry limit"));
        }
    }
    let required = entries
        .checked_add(additional_entries)
        .ok_or_else(|| format!("{label} directory headroom"))?;
    if required > journal.limits.max_directory_entries {
        return Err(format!("{label} directory headroom"));
    }
    Ok(())
}
