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
        if let Some(next_unreserved) = self.next_unreserved {
            bytes[48..56].copy_from_slice(&next_unreserved.to_le_bytes());
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
        } else if encoded_next == 0 {
            None
        } else {
            return Err("nonce compaction exhausted counter encoding".into());
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
    hmac::verify(&key, body, tag)
        .map_err(|_| "nonce compaction authentication".to_owned())?;
    let checkpoint = NonceCompactionCheckpoint::decode(body)?;
    if checkpoint.key_id != journal.key_id || checkpoint.nonce_prefix != journal.nonce_prefix {
        return Err("nonce compaction journal context".into());
    }
    Ok(checkpoint)
}

fn read_compaction_exact<const N: usize>(
    journal: &LinuxDurableNonceJournal,
    name: &OsStr,
    label: &'static str,
) -> super::CandidateResult<Option<[u8; N]>> {
    let Some(mut file) = linux_nonce_open_relative_readonly(&journal.directory, name)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file()
        || metadata.len() != u64::try_from(N).map_err(|_| format!("{label} width"))?
    {
        return Err(format!("{label} file shape"));
    }
    let mut bytes = [0u8; N];
    file.read_exact(&mut bytes).map_err(|error| error.to_string())?;
    let mut trailing = [0u8; 1];
    if file.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
        return Err(format!("{label} exact end"));
    }
    Ok(Some(bytes))
}

fn load_nonce_compaction_checkpoint(
    journal: &LinuxDurableNonceJournal,
    generation: u64,
) -> super::CandidateResult<Option<NonceCompactionCheckpoint>> {
    let name = nonce_compaction_name(generation);
    let Some(sealed) = read_compaction_exact::<NONCE_COMPACTION_BYTES>(
        journal,
        &name,
        "nonce compaction checkpoint",
    )?
    else {
        return Ok(None);
    };
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
        let mut journal_bytes_read = 0u64;

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
            if records.len() >= self.journal.limits.max_generations {
                return Err("compacted nonce generation limit".into());
            }
            let record = load_nonce_generation_record(self.journal, generation)
                .map_err(|error| error.to_string())?;
            journal_bytes_read = journal_bytes_read
                .checked_add(u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"))
                .ok_or_else(|| "compacted nonce journal bytes".to_owned())?;
            if journal_bytes_read > self.journal.limits.max_journal_bytes {
                return Err("compacted nonce byte limit".into());
            }
            bytes_read = bytes_read
                .checked_add(u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width"))
                .ok_or_else(|| "compacted nonce bytes".to_owned())?;
            records.push(record);
        }

        checkpoints.sort_unstable_by_key(|checkpoint| checkpoint.generation);
        for pair in checkpoints.windows(2) {
            let previous = pair[0];
            let next = pair[1];
            if !linux_nonce_at_least(previous.next_unreserved, next.next_unreserved) {
                return Err("nonce compaction checkpoint rollback".into());
            }
        }
        let checkpoint = checkpoints.last().copied();
        let mut durable = checkpoint
            .map(NonceCompactionCheckpoint::durable)
            .unwrap_or_else(DurableNonceState::initial);
        let checkpoint_generation = checkpoint.map(|checkpoint| checkpoint.generation);

        records.sort_unstable_by_key(|record| record.generation);
        if let Some(checkpoint) = checkpoint {
            for record in records
                .iter()
                .filter(|record| record.generation <= checkpoint.generation)
            {
                if !linux_nonce_at_least(record.next_unreserved, checkpoint.next_unreserved) {
                    return Err("nonce compaction checkpoint rollback".into());
                }
                if record.generation == checkpoint.generation
                    && record.next_unreserved != checkpoint.next_unreserved
                {
                    return Err("nonce compaction checkpoint generation mismatch".into());
                }
            }
        }

        let mut journal_records = 0usize;
        for record in records {
            if record.generation <= durable.generation {
                continue;
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
        Ok(DescriptorNonceAuthority {
            durable: self.scan(trusted_floor)?.durable,
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
        if self.scan(None)?.durable != authority.durable {
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
    preserved_nonce_records: usize,
    preserved_prepared_retirements: usize,
    preserved_source_set_records: usize,
}

#[derive(Default)]
struct CompactionMetadataInventory {
    terminal_pairs: std::collections::BTreeSet<(u64, u64)>,
    prepared_pairs: std::collections::BTreeSet<(u64, u64)>,
    live_manifests: std::collections::BTreeMap<u64, LinuxEncryptedStageManifest>,
    source_sets: Vec<(OsString, RestartSourceSetAuthority)>,
    retirement_files: Vec<(OsString, EncryptedRestartRetirementRecord)>,
}

fn parse_compaction_stage_manifest_name(
    name: &OsStr,
) -> Option<(u64, EncryptedRestartStageRole)> {
    let name = name.to_str()?;
    let body = name
        .strip_prefix(ENCRYPTED_STAGE_MANIFEST_PREFIX)?
        .strip_suffix(ENCRYPTED_STAGE_MANIFEST_SUFFIX)?;
    if body.len() < 22 || body.as_bytes().get(20) != Some(&b'-') {
        return None;
    }
    let generation_text = &body[..20];
    if !generation_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let generation = generation_text.parse::<u64>().ok()?;
    if generation == 0 {
        return None;
    }
    let role = match &body[21..] {
        "descriptor-spill" => EncryptedRestartStageRole::SortedDescriptorSpill,
        _ => return None,
    };
    if encrypted_stage_manifest_name(generation, role) != OsString::from(name) {
        return None;
    }
    Some((generation, role))
}

fn restart_manifest_object_count(
    manifest: LinuxEncryptedStageManifest,
) -> super::CandidateResult<u64> {
    let width = u64::try_from(ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES)
        .expect("encrypted spill width fits u64");
    if manifest.stage_length == 0 || manifest.stage_length % width != 0 {
        return Err("compaction manifest stage length".into());
    }
    Ok(manifest.stage_length / width)
}

fn same_retirement_payload(
    left: EncryptedRestartRetirementRecord,
    right: EncryptedRestartRetirementRecord,
) -> bool {
    left.key_id == right.key_id
        && left.nonce_prefix == right.nonce_prefix
        && left.crashed_generation == right.crashed_generation
        && left.fresh_generation == right.fresh_generation
        && left.stage_identity == right.stage_identity
        && left.manifest_identity == right.manifest_identity
        && left.output_length == right.output_length
        && left.output_sha256 == right.output_sha256
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
        if let Some((generation, role)) = parse_compaction_stage_manifest_name(&name) {
            let manifest = load_encrypted_stage_manifest(journal, generation, role)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "compaction stage manifest disappeared".to_owned())?;
            if inventory.live_manifests.insert(generation, manifest).is_some() {
                return Err("compaction duplicate live manifest generation".into());
            }
            continue;
        }
        let name_bytes = name.as_bytes();
        if name_bytes.starts_with(ENCRYPTED_RETIREMENT_PREFIX.as_bytes())
            && name_bytes.ends_with(ENCRYPTED_RETIREMENT_SUFFIX.as_bytes())
        {
            let Some(sealed) = read_compaction_exact::<ENCRYPTED_RETIREMENT_BYTES>(
                journal,
                &name,
                "compaction retirement",
            )?
            else {
                return Err("compaction retirement disappeared".into());
            };
            let record = open_encrypted_retirement_record(journal, &sealed)?;
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
            let Some(sealed) = read_compaction_exact::<RESTART_SOURCE_SET_BYTES>(
                journal,
                &name,
                "compaction source-set",
            )?
            else {
                return Err("compaction source-set disappeared".into());
            };
            let record = open_restart_source_set_authority(journal, &sealed)?;
            if restart_source_set_authority_name(record.generation, record.role) != name {
                return Err("compaction source-set canonical name".into());
            }
            inventory.source_sets.push((name, record));
        }
    }

    let mut fresh_by_crashed = std::collections::BTreeMap::new();
    for (crashed, fresh) in inventory
        .prepared_pairs
        .iter()
        .chain(inventory.terminal_pairs.iter())
        .copied()
    {
        if let Some(previous) = fresh_by_crashed.insert(crashed, fresh) {
            if previous != fresh {
                return Err("compaction competing retirement generations".into());
            }
        }
    }

    for pair in inventory
        .prepared_pairs
        .intersection(&inventory.terminal_pairs)
        .copied()
    {
        let prepared = inventory
            .retirement_files
            .iter()
            .find_map(|(_, record)| {
                (record.state == EncryptedRetirementState::Prepared
                    && (record.crashed_generation, record.fresh_generation) == pair)
                    .then_some(*record)
            })
            .ok_or_else(|| "compaction prepared retirement missing".to_owned())?;
        let terminal = inventory
            .retirement_files
            .iter()
            .find_map(|(_, record)| {
                (record.state == EncryptedRetirementState::Terminal
                    && (record.crashed_generation, record.fresh_generation) == pair)
                    .then_some(*record)
            })
            .ok_or_else(|| "compaction terminal retirement missing".to_owned())?;
        if !same_retirement_payload(prepared, terminal) {
            return Err("compaction retirement payload mismatch".into());
        }
    }

    let terminal_crashed: std::collections::BTreeSet<u64> = inventory
        .terminal_pairs
        .iter()
        .map(|(crashed, _)| *crashed)
        .collect();
    if inventory
        .live_manifests
        .keys()
        .any(|generation| terminal_crashed.contains(generation))
    {
        return Err("terminal retirement retains live stage manifest".into());
    }

    let prepared_crashed: std::collections::BTreeSet<u64> = inventory
        .prepared_pairs
        .iter()
        .map(|(crashed, _)| *crashed)
        .collect();
    for manifest in inventory.live_manifests.values().copied() {
        let nonce_record = load_nonce_generation_record(journal, manifest.generation)
            .map_err(|error| error.to_string())?;
        if nonce_record.key_id != manifest.key_id
            || nonce_record.nonce_prefix != manifest.nonce_prefix
            || nonce_record.operation_id != manifest.operation_id
        {
            return Err("compaction live manifest/nonce mismatch".into());
        }
    }
    for (_, source_set) in &inventory.source_sets {
        if let Some(manifest) = inventory.live_manifests.get(&source_set.generation) {
            if source_set.role != manifest.role
                || source_set.operation_id != manifest.operation_id
                || source_set.stage_identity != manifest.identity()
                || source_set.object_count != restart_manifest_object_count(*manifest)?
            {
                return Err("compaction source-set/live-manifest mismatch".into());
            }
            continue;
        }
        if !prepared_crashed.contains(&source_set.generation)
            && !terminal_crashed.contains(&source_set.generation)
        {
            return Err("source-set authority without live restart or cleanup".into());
        }
        let retirement = inventory
            .retirement_files
            .iter()
            .find_map(|(_, record)| {
                (record.crashed_generation == source_set.generation).then_some(*record)
            })
            .ok_or_else(|| "compaction source-set cleanup authority missing".to_owned())?;
        if source_set.stage_identity != retirement.stage_identity {
            return Err("compaction source-set/retirement mismatch".into());
        }
    }

    Ok(inventory)
}

fn persist_nonce_compaction_checkpoint(
    journal: &LinuxDurableNonceJournal,
    checkpoint: NonceCompactionCheckpoint,
    cut: RestartMetadataCompactionCut,
) -> super::CandidateResult<()> {
    if let Some(existing) = load_nonce_compaction_checkpoint(journal, checkpoint.generation)? {
        if existing != checkpoint {
            return Err("nonce compaction checkpoint conflict".into());
        }
        linux_nonce_verify_procfd_directory(&journal.directory)
            .map_err(|error| error.to_string())?;
        journal.directory.sync_all().map_err(|error| error.to_string())?;
        return Ok(());
    }
    let name = nonce_compaction_name(checkpoint.generation);
    let sealed = seal_nonce_compaction_checkpoint(journal, checkpoint)?;
    let path = linux_nonce_procfd_child(&journal.directory, &name)
        .map_err(|error| error.to_string())?;
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
    linux_nonce_verify_procfd_directory(&journal.directory)
        .map_err(|error| error.to_string())?;
    journal.directory.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

fn remove_verified_nonce_record(
    journal: &LinuxDurableNonceJournal,
    name: &OsStr,
    expected: LinuxNonceJournalRecord,
) -> super::CandidateResult<()> {
    if OsString::from(linux_nonce_journal_name(expected.generation)) != name {
        return Err("compaction nonce prune canonical name".into());
    }
    let current = load_nonce_generation_record(journal, expected.generation)
        .map_err(|error| error.to_string())?;
    if current != expected {
        return Err("compaction nonce prune identity changed".into());
    }
    let path = linux_nonce_procfd_child(&journal.directory, name)
        .map_err(|error| error.to_string())?;
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

fn remove_verified_checkpoint(
    journal: &LinuxDurableNonceJournal,
    name: &OsStr,
    expected: NonceCompactionCheckpoint,
) -> super::CandidateResult<()> {
    if nonce_compaction_name(expected.generation) != name {
        return Err("compaction checkpoint prune canonical name".into());
    }
    let current = load_nonce_compaction_checkpoint(journal, expected.generation)?
        .ok_or_else(|| "compaction checkpoint prune disappeared".to_owned())?;
    if current != expected {
        return Err("compaction checkpoint prune identity changed".into());
    }
    let path = linux_nonce_procfd_child(&journal.directory, name)
        .map_err(|error| error.to_string())?;
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

fn remove_verified_retirement(
    journal: &LinuxDurableNonceJournal,
    name: &OsStr,
    expected: EncryptedRestartRetirementRecord,
) -> super::CandidateResult<()> {
    if encrypted_retirement_name(
        expected.crashed_generation,
        expected.fresh_generation,
        expected.state,
    ) != name
    {
        return Err("compaction retirement prune canonical name".into());
    }
    let current = load_encrypted_retirement_record(
        journal,
        expected.crashed_generation,
        expected.fresh_generation,
        expected.state,
    )?
    .ok_or_else(|| "compaction retirement prune disappeared".to_owned())?;
    if current != expected {
        return Err("compaction retirement prune identity changed".into());
    }
    let path = linux_nonce_procfd_child(&journal.directory, name)
        .map_err(|error| error.to_string())?;
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

fn remove_verified_source_set(
    journal: &LinuxDurableNonceJournal,
    name: &OsStr,
    expected: RestartSourceSetAuthority,
) -> super::CandidateResult<()> {
    if restart_source_set_authority_name(expected.generation, expected.role) != name {
        return Err("compaction source-set prune canonical name".into());
    }
    let current = load_restart_source_set_authority(journal, expected.generation, expected.role)?
        .ok_or_else(|| "compaction source-set prune disappeared".to_owned())?;
    if current != expected {
        return Err("compaction source-set prune identity changed".into());
    }
    let path = linux_nonce_procfd_child(&journal.directory, name)
        .map_err(|error| error.to_string())?;
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

fn compaction_nonce_prune_inventory(
    journal: &LinuxDurableNonceJournal,
    checkpoint_generation: u64,
    protected_nonce_generations: &std::collections::BTreeSet<u64>,
) -> super::CandidateResult<(
    Vec<(OsString, LinuxNonceJournalRecord)>,
    Vec<(OsString, NonceCompactionCheckpoint)>,
    usize,
)> {
    let mut nonce_records = Vec::new();
    let mut old_checkpoints = Vec::new();
    let mut preserved_nonce_records = 0usize;
    let mut directory_entries = 0usize;
    for entry in std::fs::read_dir(linux_nonce_procfd_directory(&journal.directory))
        .map_err(|error| error.to_string())?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        directory_entries = directory_entries
            .checked_add(1)
            .ok_or_else(|| "compaction prune directory entries".to_owned())?;
        if directory_entries > journal.limits.max_directory_entries {
            return Err("compaction prune directory entry limit".into());
        }
        let name = entry.file_name();
        if let Some(generation) = linux_nonce_parse_generation_name(&name) {
            if generation <= checkpoint_generation {
                if protected_nonce_generations.contains(&generation) {
                    preserved_nonce_records = preserved_nonce_records
                        .checked_add(1)
                        .ok_or_else(|| "preserved nonce record count".to_owned())?;
                } else {
                    let record = load_nonce_generation_record(journal, generation)
                        .map_err(|error| error.to_string())?;
                    nonce_records.push((name, record));
                }
            }
            continue;
        }
        if let Some(generation) = parse_nonce_compaction_name(&name) {
            if generation < checkpoint_generation {
                let checkpoint = load_nonce_compaction_checkpoint(journal, generation)?
                    .ok_or_else(|| "compaction old checkpoint disappeared".to_owned())?;
                old_checkpoints.push((name, checkpoint));
            }
        }
    }
    Ok((nonce_records, old_checkpoints, preserved_nonce_records))
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
    for (crashed, fresh) in metadata
        .prepared_pairs
        .iter()
        .chain(metadata.terminal_pairs.iter())
        .copied()
    {
        if crashed > recovery.durable.generation || fresh > recovery.durable.generation {
            return Err("compaction retirement generation ahead of nonce authority".into());
        }
    }
    if metadata
        .live_manifests
        .keys()
        .any(|generation| *generation > recovery.durable.generation)
    {
        return Err("compaction live manifest ahead of nonce authority".into());
    }
    if metadata
        .source_sets
        .iter()
        .any(|(_, source_set)| source_set.generation > recovery.durable.generation)
    {
        return Err("compaction source-set ahead of nonce authority".into());
    }

    let terminal_crashed: std::collections::BTreeSet<u64> = metadata
        .terminal_pairs
        .iter()
        .map(|(crashed, _)| *crashed)
        .collect();
    let preserved_prepared_retirements = metadata
        .prepared_pairs
        .difference(&metadata.terminal_pairs)
        .count();
    let protected_nonce_generations: std::collections::BTreeSet<u64> =
        metadata.live_manifests.keys().copied().collect();

    let checkpoint = NonceCompactionCheckpoint::from_durable(journal, recovery.durable)?;
    persist_nonce_compaction_checkpoint(journal, checkpoint, cut)?;
    let (nonce_records, old_checkpoints, preserved_nonce_records) =
        compaction_nonce_prune_inventory(
            journal,
            checkpoint.generation,
            &protected_nonce_generations,
        )?;
    if cut == RestartMetadataCompactionCut::AfterCheckpointDirectorySyncBeforePrune {
        return Ok(RestartMetadataCompactionReport {
            checkpoint_generation: checkpoint.generation,
            pruned_nonce_records: 0,
            pruned_retirement_records: 0,
            pruned_source_set_records: 0,
            pruned_old_checkpoints: 0,
            preserved_nonce_records,
            preserved_prepared_retirements,
            preserved_source_set_records: metadata.source_sets.len(),
        });
    }

    let mut pruned_nonce_records = 0usize;
    let mut pruned_retirement_records = 0usize;
    let mut pruned_source_set_records = 0usize;
    let mut pruned_old_checkpoints = 0usize;
    let mut preserved_source_set_records = 0usize;

    for (name, record) in nonce_records {
        remove_verified_nonce_record(journal, &name, record)?;
        pruned_nonce_records = pruned_nonce_records
            .checked_add(1)
            .ok_or_else(|| "pruned nonce record count".to_owned())?;
    }
    for (name, record) in &metadata.retirement_files {
        let pair = (record.crashed_generation, record.fresh_generation);
        if metadata.terminal_pairs.contains(&pair) {
            remove_verified_retirement(journal, name, *record)?;
            pruned_retirement_records = pruned_retirement_records
                .checked_add(1)
                .ok_or_else(|| "pruned retirement record count".to_owned())?;
        }
    }
    for (name, record) in &metadata.source_sets {
        if terminal_crashed.contains(&record.generation) {
            remove_verified_source_set(journal, name, *record)?;
            pruned_source_set_records = pruned_source_set_records
                .checked_add(1)
                .ok_or_else(|| "pruned source-set record count".to_owned())?;
        } else {
            preserved_source_set_records = preserved_source_set_records
                .checked_add(1)
                .ok_or_else(|| "preserved source-set record count".to_owned())?;
        }
    }
    for (name, old_checkpoint) in old_checkpoints {
        remove_verified_checkpoint(journal, &name, old_checkpoint)?;
        pruned_old_checkpoints = pruned_old_checkpoints
            .checked_add(1)
            .ok_or_else(|| "pruned checkpoint count".to_owned())?;
    }

    if cut == RestartMetadataCompactionCut::AfterPruneBeforeDirectorySync {
        return Ok(RestartMetadataCompactionReport {
            checkpoint_generation: checkpoint.generation,
            pruned_nonce_records,
            pruned_retirement_records,
            pruned_source_set_records,
            pruned_old_checkpoints,
            preserved_nonce_records,
            preserved_prepared_retirements,
            preserved_source_set_records,
        });
    }
    linux_nonce_verify_procfd_directory(&journal.directory)
        .map_err(|error| error.to_string())?;
    journal.directory.sync_all().map_err(|error| error.to_string())?;
    Ok(RestartMetadataCompactionReport {
        checkpoint_generation: checkpoint.generation,
        pruned_nonce_records,
        pruned_retirement_records,
        pruned_source_set_records,
        pruned_old_checkpoints,
        preserved_nonce_records,
        preserved_prepared_retirements,
        preserved_source_set_records,
    })
}
