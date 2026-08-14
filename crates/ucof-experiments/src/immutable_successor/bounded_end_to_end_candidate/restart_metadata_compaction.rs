const NONCE_COMPACTION_MAGIC: &[u8; 8] = b"UCOFCP01";
const NONCE_COMPACTION_VERSION: u8 = 1;
const NONCE_COMPACTION_BODY_BYTES: usize = 80;
const NONCE_COMPACTION_TAG_BYTES: usize = 32;
const NONCE_COMPACTION_BYTES: usize = NONCE_COMPACTION_BODY_BYTES + NONCE_COMPACTION_TAG_BYTES;
const NONCE_COMPACTION_PREFIX: &str = ".ucof-nonce-checkpoint-v1-";
const NONCE_COMPACTION_SUFFIX: &str = ".bin";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NonceCompactionCheckpoint {
    key_id: [u8; 16],
    nonce_prefix: [u8; 4],
    generation: u64,
    next_unreserved: Option<u64>,
}

impl NonceCompactionCheckpoint {
    fn from_durable(
        journal: &LinuxDurableNonceJournal,
        durable: DurableNonceState,
    ) -> super::CandidateResult<Self> {
        if durable.generation == 0 {
            return Err("nonce compaction initial state".into());
        }
        Ok(Self {
            key_id: journal.key_id,
            nonce_prefix: journal.nonce_prefix,
            generation: durable.generation,
            next_unreserved: durable.next_unreserved,
        })
    }

    fn durable(self) -> DurableNonceState {
        DurableNonceState {
            generation: self.generation,
            next_unreserved: self.next_unreserved,
        }
    }

    fn encode(&self) -> super::CandidateResult<[u8; NONCE_COMPACTION_BODY_BYTES]> {
        if self.key_id == [0; 16] || self.generation == 0 {
            return Err("nonce compaction checkpoint fields".into());
        }
        let mut bytes = [0u8; NONCE_COMPACTION_BODY_BYTES];
        bytes[..8].copy_from_slice(NONCE_COMPACTION_MAGIC);
        bytes[8] = NONCE_COMPACTION_VERSION;
        bytes[9] = u8::from(self.next_unreserved.is_some());
        bytes[16..32].copy_from_slice(&self.key_id);
        bytes[32..36].copy_from_slice(&self.nonce_prefix);
        bytes[40..48].copy_from_slice(&self.generation.to_le_bytes());
        if let Some(next) = self.next_unreserved {
            bytes[48..56].copy_from_slice(&next.to_le_bytes());
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> super::CandidateResult<Self> {
        if bytes.len() != NONCE_COMPACTION_BODY_BYTES {
            return Err("nonce compaction checkpoint length".into());
        }
        if &bytes[..8] != NONCE_COMPACTION_MAGIC || bytes[8] != NONCE_COMPACTION_VERSION {
            return Err("nonce compaction checkpoint header".into());
        }
        if bytes[9] > 1
            || bytes[10..16].iter().any(|byte| *byte != 0)
            || bytes[36..40].iter().any(|byte| *byte != 0)
            || bytes[56..80].iter().any(|byte| *byte != 0)
        {
            return Err("nonce compaction checkpoint reserved bytes".into());
        }
        let encoded_next = u64::from_le_bytes(
            bytes[48..56]
                .try_into()
                .expect("nonce compaction next counter"),
        );
        let next_unreserved = if bytes[9] == 1 {
            Some(encoded_next)
        } else {
            if encoded_next != 0 {
                return Err("nonce compaction exhausted counter encoding".into());
            }
            None
        };
        let checkpoint = Self {
            key_id: bytes[16..32].try_into().expect("nonce compaction key id"),
            nonce_prefix: bytes[32..36]
                .try_into()
                .expect("nonce compaction prefix"),
            generation: u64::from_le_bytes(
                bytes[40..48]
                    .try_into()
                    .expect("nonce compaction generation"),
            ),
            next_unreserved,
        };
        checkpoint.encode()?;
        Ok(checkpoint)
    }
}

fn nonce_compaction_name(generation: u64) -> OsString {
    OsString::from(format!(
        "{NONCE_COMPACTION_PREFIX}{generation:020}{NONCE_COMPACTION_SUFFIX}"
    ))
}

fn parse_nonce_compaction_name(name: &OsStr) -> Option<u64> {
    let name = name.to_str()?;
    let digits = name
        .strip_prefix(NONCE_COMPACTION_PREFIX)?
        .strip_suffix(NONCE_COMPACTION_SUFFIX)?;
    if digits.len() != 20 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let generation = digits.parse::<u64>().ok()?;
    if generation == 0 || nonce_compaction_name(generation) != OsString::from(name) {
        return None;
    }
    Some(generation)
}

fn seal_nonce_compaction_checkpoint(
    journal: &LinuxDurableNonceJournal,
    checkpoint: NonceCompactionCheckpoint,
) -> super::CandidateResult<[u8; NONCE_COMPACTION_BYTES]> {
    let body = checkpoint.encode()?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, &journal.journal_auth_key);
    let tag = hmac::sign(&key, &body);
    if tag.as_ref().len() != NONCE_COMPACTION_TAG_BYTES {
        return Err("nonce compaction HMAC width".into());
    }
    let mut sealed = [0u8; NONCE_COMPACTION_BYTES];
    sealed[..NONCE_COMPACTION_BODY_BYTES].copy_from_slice(&body);
    sealed[NONCE_COMPACTION_BODY_BYTES..].copy_from_slice(tag.as_ref());
    Ok(sealed)
}

fn open_nonce_compaction_checkpoint(
    journal: &LinuxDurableNonceJournal,
    sealed: &[u8; NONCE_COMPACTION_BYTES],
) -> super::CandidateResult<NonceCompactionCheckpoint> {
    let (body, tag) = sealed.split_at(NONCE_COMPACTION_BODY_BYTES);
    let key = hmac::Key::new(hmac::HMAC_SHA256, &journal.journal_auth_key);
    hmac::verify(&key, body, tag).map_err(|_| "nonce compaction authentication".to_owned())?;
    let checkpoint = NonceCompactionCheckpoint::decode(body)?;
    if checkpoint.key_id != journal.key_id || checkpoint.nonce_prefix != journal.nonce_prefix {
        return Err("nonce compaction journal context".into());
    }
    Ok(checkpoint)
}

fn load_nonce_compaction_checkpoint(
    journal: &LinuxDurableNonceJournal,
    generation: u64,
) -> super::CandidateResult<Option<NonceCompactionCheckpoint>> {
    let name = nonce_compaction_name(generation);
    let Some(mut file) = linux_nonce_open_relative_readonly(&journal.directory, &name)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file()
        || metadata.len() != u64::try_from(NONCE_COMPACTION_BYTES).expect("checkpoint width")
    {
        return Err("nonce compaction file shape".into());
    }
    let mut sealed = [0u8; NONCE_COMPACTION_BYTES];
    file.read_exact(&mut sealed).map_err(|error| error.to_string())?;
    let mut trailing = [0u8; 1];
    if file.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
        return Err("nonce compaction exact end".into());
    }
    let checkpoint = open_nonce_compaction_checkpoint(journal, &sealed)?;
    if checkpoint.generation != generation {
        return Err("nonce compaction filename generation".into());
    }
    Ok(Some(checkpoint))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompactedNonceRecovery {
    durable: DurableNonceState,
    checkpoint_generation: Option<u64>,
    journal_records: usize,
    bytes_read: u64,
}

struct CompactedNonceJournal<'a> {
    journal: &'a LinuxDurableNonceJournal,
}

impl<'a> CompactedNonceJournal<'a> {
    fn new(journal: &'a LinuxDurableNonceJournal) -> Self {
        Self { journal }
    }

    fn scan(
        &self,
        trusted_floor: Option<TrustedNonceFloor>,
    ) -> super::CandidateResult<CompactedNonceRecovery> {
        linux_nonce_verify_procfd_directory(&self.journal.directory)
            .map_err(|error| error.to_string())?;
        let mut checkpoints = Vec::new();
        let mut records = Vec::new();
        let mut directory_entries = 0usize;
        let mut bytes_read = 0u64;
        for entry in std::fs::read_dir(linux_nonce_procfd_directory(&self.journal.directory))
            .map_err(|error| error.to_string())?
        {
            let entry = entry.map_err(|error| error.to_string())?;
            directory_entries = directory_entries
                .checked_add(1)
                .ok_or_else(|| "compacted nonce directory entries".to_owned())?;
            if directory_entries > self.journal.limits.max_directory_entries {
                return Err("compacted nonce directory entry limit".into());
            }
            let name = entry.file_name();
            if let Some(generation) = parse_nonce_compaction_name(&name) {
                let checkpoint = load_nonce_compaction_checkpoint(self.journal, generation)?
                    .ok_or_else(|| "nonce compaction checkpoint disappeared".to_owned())?;
                bytes_read = bytes_read
                    .checked_add(u64::try_from(NONCE_COMPACTION_BYTES).expect("checkpoint width"))
                    .ok_or_else(|| "compacted nonce bytes".to_owned())?;
                checkpoints.push(checkpoint);
                continue;
            }
            let Some(generation) = linux_nonce_parse_generation_name(&name) else {
                continue;
            };
            let file = linux_nonce_open_relative_readonly(&self.journal.directory, &name)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "compacted nonce journal disappeared".to_owned())?;
            let metadata = file.metadata().map_err(|error| error.to_string())?;
            if !metadata.file_type().is_file()
                || metadata.len()
                    != u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width")
            {
                return Err("compacted nonce journal file shape".into());
            }
            bytes_read = bytes_read
                .checked_add(metadata.len())
                .ok_or_else(|| "compacted nonce bytes".to_owned())?;
            let sealed = linux_nonce_read_exact_file(file).map_err(|error| error.to_string())?;
            let record = self
                .journal
                .open_record(&sealed)
                .map_err(|error| error.to_string())?;
            if record.generation != generation {
                return Err("compacted nonce filename generation".into());
            }
            records.push(record);
        }

        checkpoints.sort_unstable_by_key(|checkpoint| checkpoint.generation);
        let checkpoint = checkpoints.last().copied();
        let mut durable = checkpoint
            .map(NonceCompactionCheckpoint::durable)
            .unwrap_or_else(DurableNonceState::initial);
        let checkpoint_generation = checkpoint.map(|checkpoint| checkpoint.generation);
        records.sort_unstable_by_key(|record| record.generation);
        let mut journal_records = 0usize;
        for record in records {
            if record.generation <= durable.generation {
                continue;
            }
            if record.key_id != self.journal.key_id {
                return Err("compacted nonce foreign key".into());
            }
            if record.nonce_prefix != self.journal.nonce_prefix {
                return Err("compacted nonce foreign prefix".into());
            }
            if durable.generation.checked_add(1) != Some(record.generation)
                || durable.next_unreserved != Some(record.lease_first)
            {
                return Err("compacted nonce generation/lease gap".into());
            }
            durable = DurableNonceState {
                generation: record.generation,
                next_unreserved: record.next_unreserved,
            };
            journal_records = journal_records
                .checked_add(1)
                .ok_or_else(|| "compacted nonce journal record count".to_owned())?;
        }
        if let Some(floor) = trusted_floor {
            if durable.generation < floor.generation
                || !linux_nonce_at_least(floor.next_unreserved, durable.next_unreserved)
            {
                return Err("compacted nonce below trusted floor".into());
            }
        }
        Ok(CompactedNonceRecovery {
            durable,
            checkpoint_generation,
            journal_records,
            bytes_read,
        })
    }

    fn recover_authority(
        &self,
        trusted_floor: Option<TrustedNonceFloor>,
    ) -> super::CandidateResult<DescriptorNonceAuthority> {
        let recovery = self.scan(trusted_floor)?;
        Ok(DescriptorNonceAuthority {
            durable: recovery.durable,
        })
    }

    fn commit_descriptor_session(
        &self,
        authority: &mut DescriptorNonceAuthority,
        aes_key: [u8; 32],
        operation_id: [u8; 16],
        lease_size: u64,
        cut: JournalCommitCut,
    ) -> super::CandidateResult<DescriptorEncryptionSession> {
        if linux_nonce_key_id(&aes_key) != self.journal.key_id {
            return Err("compacted nonce foreign key".into());
        }
        let observed = self.scan(None)?.durable;
        if observed != authority.durable {
            return Err("compacted nonce stale authority".into());
        }
        let pending = reserve_nonce_lease(
            authority.durable,
            lease_size,
            self.journal.limits.max_lease_size,
        )
        .map_err(|error| format!("compacted nonce lease: {error:?}"))?;
        let record = LinuxNonceJournalRecord::from_pending(
            self.journal.key_id,
            self.journal.nonce_prefix,
            operation_id,
            pending,
        )
        .map_err(|error| error.to_string())?;
        self.journal
            .persist_record(record, cut)
            .map_err(|error| error.to_string())?;
        let (durable, lease) = activate_nonce_lease(authority.durable, pending, true)
            .map_err(|error| format!("compacted nonce activate: {error:?}"))?;
        authority.durable = durable;
        Ok(DescriptorEncryptionSession {
            key: aes_key,
            nonce_prefix: self.journal.nonce_prefix,
            operation_id,
            journal_generation: durable.generation,
            lease,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestartMetadataCompactionCut {
    AfterCheckpointFileSyncBeforeDirectorySync,
    AfterCheckpointDirectorySyncBeforePrune,
    AfterPruneBeforeDirectorySync,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestartMetadataCompactionReport {
    checkpoint_generation: u64,
    pruned_nonce_records: usize,
    pruned_retirement_records: usize,
    pruned_source_set_records: usize,
    pruned_old_checkpoints: usize,
    preserved_prepared_retirements: usize,
    preserved_source_set_records: usize,
}

#[derive(Default)]
struct CompactionMetadataInventory {
    terminal_pairs: std::collections::BTreeSet<(u64, u64)>,
    prepared_pairs: std::collections::BTreeSet<(u64, u64)>,
    source_sets: Vec<(OsString, RestartSourceSetAuthority)>,
    retirement_files: Vec<(OsString, EncryptedRestartRetirementRecord)>,
}

fn scan_compaction_metadata(
    journal: &LinuxDurableNonceJournal,
) -> super::CandidateResult<CompactionMetadataInventory> {
    let mut inventory = CompactionMetadataInventory::default();
    let mut directory_entries = 0usize;
    for entry in std::fs::read_dir(linux_nonce_procfd_directory(&journal.directory))
        .map_err(|error| error.to_string())?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        directory_entries = directory_entries
            .checked_add(1)
            .ok_or_else(|| "compaction metadata directory entries".to_owned())?;
        if directory_entries > journal.limits.max_directory_entries {
            return Err("compaction metadata directory entry limit".into());
        }
        let name = entry.file_name();
        let name_bytes = name.as_bytes();
        if name_bytes.starts_with(ENCRYPTED_RETIREMENT_PREFIX.as_bytes())
            && name_bytes.ends_with(ENCRYPTED_RETIREMENT_SUFFIX.as_bytes())
        {
            let mut file = linux_nonce_open_relative_readonly(&journal.directory, &name)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "compaction retirement disappeared".to_owned())?;
            let metadata = file.metadata().map_err(|error| error.to_string())?;
            if !metadata.file_type().is_file()
                || metadata.len()
                    != u64::try_from(ENCRYPTED_RETIREMENT_BYTES).expect("retirement width")
            {
                return Err("compaction retirement file shape".into());
            }
            let mut sealed = [0u8; ENCRYPTED_RETIREMENT_BYTES];
            file.read_exact(&mut sealed).map_err(|error| error.to_string())?;
            let mut trailing = [0u8; 1];
            if file.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
                return Err("compaction retirement exact end".into());
            }
            let record = open_encrypted_retirement_record(journal, &sealed)?;
            if record.key_id != journal.key_id || record.nonce_prefix != journal.nonce_prefix {
                return Err("compaction retirement context".into());
            }
            if encrypted_retirement_name(
                record.crashed_generation,
                record.fresh_generation,
                record.state,
            ) != name
            {
                return Err("compaction retirement canonical name".into());
            }
            let pair = (record.crashed_generation, record.fresh_generation);
            match record.state {
                EncryptedRetirementState::Prepared => {
                    inventory.prepared_pairs.insert(pair);
                }
                EncryptedRetirementState::Terminal => {
                    inventory.terminal_pairs.insert(pair);
                }
            }
            inventory.retirement_files.push((name, record));
            continue;
        }
        if name_bytes.starts_with(RESTART_SOURCE_SET_PREFIX.as_bytes())
            && name_bytes.ends_with(RESTART_SOURCE_SET_SUFFIX.as_bytes())
        {
            let mut file = linux_nonce_open_relative_readonly(&journal.directory, &name)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "compaction source-set disappeared".to_owned())?;
            let metadata = file.metadata().map_err(|error| error.to_string())?;
            if !metadata.file_type().is_file()
                || metadata.len()
                    != u64::try_from(RESTART_SOURCE_SET_BYTES).expect("source-set width")
            {
                return Err("compaction source-set file shape".into());
            }
            let mut sealed = [0u8; RESTART_SOURCE_SET_BYTES];
            file.read_exact(&mut sealed).map_err(|error| error.to_string())?;
            let mut trailing = [0u8; 1];
            if file.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
                return Err("compaction source-set exact end".into());
            }
            let record = open_restart_source_set_authority(journal, &sealed)?;
            if record.key_id != journal.key_id || record.nonce_prefix != journal.nonce_prefix {
                return Err("compaction source-set context".into());
            }
            if restart_source_set_authority_name(record.generation, record.role) != name {
                return Err("compaction source-set canonical name".into());
            }
            inventory.source_sets.push((name, record));
        }
    }
    Ok(inventory)
}

fn persist_nonce_compaction_checkpoint(
    journal: &LinuxDurableNonceJournal,
    checkpoint: NonceCompactionCheckpoint,
    cut: RestartMetadataCompactionCut,
) -> super::CandidateResult<()> {
    let name = nonce_compaction_name(checkpoint.generation);
    if let Some(existing) = load_nonce_compaction_checkpoint(journal, checkpoint.generation)? {
        if existing != checkpoint {
            return Err("nonce compaction checkpoint conflict".into());
        }
        return Ok(());
    }
    let sealed = seal_nonce_compaction_checkpoint(journal, checkpoint)?;
    let path = linux_nonce_procfd_child(&journal.directory, &name).map_err(|error| error.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .map_err(|error| format!("nonce compaction checkpoint create: {error}"))?;
    file.write_all(&sealed).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    if cut == RestartMetadataCompactionCut::AfterCheckpointFileSyncBeforeDirectorySync {
        return Err("injected compaction cut after checkpoint file sync".into());
    }
    linux_nonce_verify_procfd_directory(&journal.directory).map_err(|error| error.to_string())?;
    journal.directory.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

fn remove_compaction_file(
    journal: &LinuxDurableNonceJournal,
    name: &OsStr,
) -> super::CandidateResult<()> {
    let path = linux_nonce_procfd_child(&journal.directory, name).map_err(|error| error.to_string())?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn compact_restart_metadata(
    journal: &LinuxDurableNonceJournal,
    trusted_floor: Option<TrustedNonceFloor>,
    cut: RestartMetadataCompactionCut,
) -> super::CandidateResult<RestartMetadataCompactionReport> {
    let compacted = CompactedNonceJournal::new(journal);
    let recovery = compacted.scan(trusted_floor)?;
    if recovery.durable.generation == 0 {
        return Err("restart metadata compaction requires durable generation".into());
    }
    let metadata = scan_compaction_metadata(journal)?;
    let checkpoint = NonceCompactionCheckpoint::from_durable(journal, recovery.durable)?;
    persist_nonce_compaction_checkpoint(journal, checkpoint, cut)?;
    if cut == RestartMetadataCompactionCut::AfterCheckpointDirectorySyncBeforePrune {
        return Ok(RestartMetadataCompactionReport {
            checkpoint_generation: checkpoint.generation,
            pruned_nonce_records: 0,
            pruned_retirement_records: 0,
            pruned_source_set_records: 0,
            pruned_old_checkpoints: 0,
            preserved_prepared_retirements: metadata
                .prepared_pairs
                .difference(&metadata.terminal_pairs)
                .count(),
            preserved_source_set_records: metadata.source_sets.len(),
        });
    }

    let terminal_crashed: std::collections::BTreeSet<u64> = metadata
        .terminal_pairs
        .iter()
        .map(|(crashed, _)| *crashed)
        .collect();
    let mut pruned_nonce_records = 0usize;
    let mut pruned_retirement_records = 0usize;
    let mut pruned_source_set_records = 0usize;
    let mut pruned_old_checkpoints = 0usize;
    let mut preserved_source_set_records = 0usize;

    for generation in 1..=checkpoint.generation {
        let name = OsString::from(linux_nonce_journal_name(generation));
        if linux_nonce_open_relative_readonly(&journal.directory, &name)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            remove_compaction_file(journal, &name)?;
            pruned_nonce_records = pruned_nonce_records
                .checked_add(1)
                .ok_or_else(|| "pruned nonce record count".to_owned())?;
        }
    }

    for (name, record) in metadata.retirement_files {
        let pair = (record.crashed_generation, record.fresh_generation);
        if metadata.terminal_pairs.contains(&pair) {
            remove_compaction_file(journal, &name)?;
            pruned_retirement_records = pruned_retirement_records
                .checked_add(1)
                .ok_or_else(|| "pruned retirement record count".to_owned())?;
        }
    }

    for (name, record) in metadata.source_sets {
        if terminal_crashed.contains(&record.generation) {
            remove_compaction_file(journal, &name)?;
            pruned_source_set_records = pruned_source_set_records
                .checked_add(1)
                .ok_or_else(|| "pruned source-set record count".to_owned())?;
        } else {
            preserved_source_set_records = preserved_source_set_records
                .checked_add(1)
                .ok_or_else(|| "preserved source-set record count".to_owned())?;
        }
    }

    for entry in std::fs::read_dir(linux_nonce_procfd_directory(&journal.directory))
        .map_err(|error| error.to_string())?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let Some(generation) = parse_nonce_compaction_name(&name) else {
            continue;
        };
        if generation < checkpoint.generation {
            remove_compaction_file(journal, &name)?;
            pruned_old_checkpoints = pruned_old_checkpoints
                .checked_add(1)
                .ok_or_else(|| "pruned checkpoint count".to_owned())?;
        }
    }

    if cut == RestartMetadataCompactionCut::AfterPruneBeforeDirectorySync {
        return Ok(RestartMetadataCompactionReport {
            checkpoint_generation: checkpoint.generation,
            pruned_nonce_records,
            pruned_retirement_records,
            pruned_source_set_records,
            pruned_old_checkpoints,
            preserved_prepared_retirements: metadata
                .prepared_pairs
                .difference(&metadata.terminal_pairs)
                .count(),
            preserved_source_set_records,
        });
    }
    linux_nonce_verify_procfd_directory(&journal.directory).map_err(|error| error.to_string())?;
    journal.directory.sync_all().map_err(|error| error.to_string())?;
    Ok(RestartMetadataCompactionReport {
        checkpoint_generation: checkpoint.generation,
        pruned_nonce_records,
        pruned_retirement_records,
        pruned_source_set_records,
        pruned_old_checkpoints,
        preserved_prepared_retirements: metadata
            .prepared_pairs
            .difference(&metadata.terminal_pairs)
            .count(),
        preserved_source_set_records,
    })
}
