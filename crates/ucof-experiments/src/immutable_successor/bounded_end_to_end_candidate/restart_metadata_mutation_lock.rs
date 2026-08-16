struct RestartMetadataMutationGuard {
    directory: std::fs::File,
}

impl Drop for RestartMetadataMutationGuard {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.directory, rustix::fs::FlockOperation::Unlock);
    }
}

fn acquire_restart_metadata_mutation_lock(
    journal: &LinuxDurableNonceJournal,
) -> Result<RestartMetadataMutationGuard, LinuxNonceJournalError> {
    linux_nonce_verify_procfd_directory(&journal.directory)?;
    let expected = journal
        .directory
        .metadata()
        .map_err(|_| LinuxNonceJournalError::Io("mutation lock directory metadata"))?;
    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(LINUX_O_DIRECTORY | LINUX_O_CLOEXEC)
        .open(linux_nonce_procfd_directory(&journal.directory))
        .map_err(|_| LinuxNonceJournalError::Io("mutation lock directory open"))?;
    let observed = directory
        .metadata()
        .map_err(|_| LinuxNonceJournalError::Io("mutation lock descriptor metadata"))?;
    if !observed.file_type().is_dir()
        || observed.dev() != expected.dev()
        || observed.ino() != expected.ino()
    {
        return Err(LinuxNonceJournalError::Invalid(
            "mutation lock directory identity",
        ));
    }
    match rustix::fs::flock(
        &directory,
        rustix::fs::FlockOperation::NonBlockingLockExclusive,
    ) {
        Ok(()) => Ok(RestartMetadataMutationGuard { directory }),
        Err(error) if error == rustix::io::Errno::AGAIN => {
            Err(LinuxNonceJournalError::MutationLockBusy)
        }
        Err(_) => Err(LinuxNonceJournalError::Io("restart metadata mutation lock")),
    }
}
