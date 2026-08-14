const ENCRYPTED_RETIREMENT_MAGIC: &[u8; 8] = b"UCOFRT05";
const ENCRYPTED_RETIREMENT_VERSION: u8 = 1;
const ENCRYPTED_RETIREMENT_BODY_BYTES: usize = 176;
const ENCRYPTED_RETIREMENT_TAG_BYTES: usize = 32;
const ENCRYPTED_RETIREMENT_BYTES: usize =
    ENCRYPTED_RETIREMENT_BODY_BYTES + ENCRYPTED_RETIREMENT_TAG_BYTES;
const ENCRYPTED_RETIREMENT_PREFIX: &str = ".ucof-encrypted-retirement-v1-";
const ENCRYPTED_RETIREMENT_SUFFIX: &str = ".bin";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncryptedRetirementState {
    Prepared = 1,
    Terminal = 2,
}

impl EncryptedRetirementState {
    fn from_byte(value: u8) -> super::CandidateResult<Self> {
        match value {
            1 => Ok(Self::Prepared),
            2 => Ok(Self::Terminal),
            _ => Err("encrypted retirement state".into()),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EncryptedRestartRetirementRecord {
    state: EncryptedRetirementState,
    key_id: [u8; 16],
    nonce_prefix: [u8; 4],
    crashed_generation: u64,
    fresh_generation: u64,
    stage_identity: [u8; 32],
    manifest_identity: [u8; 32],
    output_length: u64,
    output_sha256: [u8; 32],
}

impl EncryptedRestartRetirementRecord {
    fn with_state(self, state: EncryptedRetirementState) -> Self {
        Self { state, ..self }
    }

    fn encode(&self) -> super::CandidateResult<[u8; ENCRYPTED_RETIREMENT_BODY_BYTES]> {
        if self.key_id == [0; 16]
            || self.crashed_generation == 0
            || self.fresh_generation <= self.crashed_generation
            || self.output_length == 0
        {
            return Err("encrypted retirement fields".into());
        }
        let mut bytes = [0u8; ENCRYPTED_RETIREMENT_BODY_BYTES];
        bytes[..8].copy_from_slice(ENCRYPTED_RETIREMENT_MAGIC);
        bytes[8] = ENCRYPTED_RETIREMENT_VERSION;
        bytes[9] = self.state as u8;
        bytes[16..32].copy_from_slice(&self.key_id);
        bytes[32..36].copy_from_slice(&self.nonce_prefix);
        bytes[40..48].copy_from_slice(&self.crashed_generation.to_le_bytes());
        bytes[48..56].copy_from_slice(&self.fresh_generation.to_le_bytes());
        bytes[56..88].copy_from_slice(&self.stage_identity);
        bytes[88..120].copy_from_slice(&self.manifest_identity);
        bytes[120..128].copy_from_slice(&self.output_length.to_le_bytes());
        bytes[128..160].copy_from_slice(&self.output_sha256);
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> super::CandidateResult<Self> {
        if bytes.len() != ENCRYPTED_RETIREMENT_BODY_BYTES {
            return Err("encrypted retirement length".into());
        }
        if &bytes[..8] != ENCRYPTED_RETIREMENT_MAGIC || bytes[8] != ENCRYPTED_RETIREMENT_VERSION {
            return Err("encrypted retirement header".into());
        }
        if bytes[10..16].iter().any(|byte| *byte != 0)
            || bytes[36..40].iter().any(|byte| *byte != 0)
            || bytes[160..176].iter().any(|byte| *byte != 0)
        {
            return Err("encrypted retirement reserved bytes".into());
        }
        let record = Self {
            state: EncryptedRetirementState::from_byte(bytes[9])?,
            key_id: bytes[16..32].try_into().expect("retirement key id"),
            nonce_prefix: bytes[32..36].try_into().expect("retirement nonce prefix"),
            crashed_generation: u64::from_le_bytes(
                bytes[40..48].try_into().expect("retirement crashed generation"),
            ),
            fresh_generation: u64::from_le_bytes(
                bytes[48..56].try_into().expect("retirement fresh generation"),
            ),
            stage_identity: bytes[56..88].try_into().expect("retirement stage identity"),
            manifest_identity: bytes[88..120]
                .try_into()
                .expect("retirement manifest identity"),
            output_length: u64::from_le_bytes(
                bytes[120..128].try_into().expect("retirement output length"),
            ),
            output_sha256: bytes[128..160].try_into().expect("retirement output digest"),
        };
        record.encode()?;
        Ok(record)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncryptedRetirementCut {
    AfterPreparedBeforeUnlink,
    AfterStageUnlinkBeforeDirectorySync,
    AfterUnlinksBeforeDirectorySync,
    AfterDirectorySyncBeforeTerminal,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EncryptedRetirementOutcome {
    NoPreparedAuthority,
    RetainIndeterminate,
    Cut(EncryptedRetirementCut),
    Terminal,
    AlreadyTerminal,
}

fn encrypted_retirement_name(
    crashed_generation: u64,
    fresh_generation: u64,
    state: EncryptedRetirementState,
) -> OsString {
    OsString::from(format!(
        "{ENCRYPTED_RETIREMENT_PREFIX}{crashed_generation:020}-{fresh_generation:020}-{}{ENCRYPTED_RETIREMENT_SUFFIX}",
        state.label()
    ))
}

fn seal_encrypted_retirement_record(
    journal: &LinuxDurableNonceJournal,
    record: EncryptedRestartRetirementRecord,
) -> super::CandidateResult<[u8; ENCRYPTED_RETIREMENT_BYTES]> {
    let body = record.encode()?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, &journal.journal_auth_key);
    let tag = hmac::sign(&key, &body);
    if tag.as_ref().len() != ENCRYPTED_RETIREMENT_TAG_BYTES {
        return Err("encrypted retirement HMAC width".into());
    }
    let mut sealed = [0u8; ENCRYPTED_RETIREMENT_BYTES];
    sealed[..ENCRYPTED_RETIREMENT_BODY_BYTES].copy_from_slice(&body);
    sealed[ENCRYPTED_RETIREMENT_BODY_BYTES..].copy_from_slice(tag.as_ref());
    Ok(sealed)
}

fn open_encrypted_retirement_record(
    journal: &LinuxDurableNonceJournal,
    sealed: &[u8; ENCRYPTED_RETIREMENT_BYTES],
) -> super::CandidateResult<EncryptedRestartRetirementRecord> {
    let (body, tag) = sealed.split_at(ENCRYPTED_RETIREMENT_BODY_BYTES);
    let key = hmac::Key::new(hmac::HMAC_SHA256, &journal.journal_auth_key);
    hmac::verify(&key, body, tag).map_err(|_| "encrypted retirement authentication".to_owned())?;
    EncryptedRestartRetirementRecord::decode(body)
}

fn persist_encrypted_retirement_record(
    journal: &LinuxDurableNonceJournal,
    record: EncryptedRestartRetirementRecord,
) -> super::CandidateResult<()> {
    let name = encrypted_retirement_name(
        record.crashed_generation,
        record.fresh_generation,
        record.state,
    );
    let path = linux_nonce_procfd_child(&journal.directory, &name).map_err(|error| error.to_string())?;
    let sealed = seal_encrypted_retirement_record(journal, record)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(&sealed).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    linux_nonce_verify_procfd_directory(&journal.directory).map_err(|error| error.to_string())?;
    journal.directory.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

fn load_encrypted_retirement_record(
    journal: &LinuxDurableNonceJournal,
    crashed_generation: u64,
    fresh_generation: u64,
    state: EncryptedRetirementState,
) -> super::CandidateResult<Option<EncryptedRestartRetirementRecord>> {
    let name = encrypted_retirement_name(crashed_generation, fresh_generation, state);
    let Some(mut file) = linux_nonce_open_relative_readonly(&journal.directory, &name)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file()
        || metadata.len() != u64::try_from(ENCRYPTED_RETIREMENT_BYTES).expect("retirement width")
    {
        return Err("encrypted retirement file shape".into());
    }
    let mut sealed = [0u8; ENCRYPTED_RETIREMENT_BYTES];
    file.read_exact(&mut sealed).map_err(|error| error.to_string())?;
    let mut trailing = [0u8; 1];
    if file.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
        return Err("encrypted retirement exact end".into());
    }
    let record = open_encrypted_retirement_record(journal, &sealed)?;
    if record.crashed_generation != crashed_generation
        || record.fresh_generation != fresh_generation
        || record.state != state
        || record.key_id != journal.key_id
        || record.nonce_prefix != journal.nonce_prefix
    {
        return Err("encrypted retirement context".into());
    }
    Ok(Some(record))
}

fn exact_file_identity_in_directory(
    directory: &File,
    name: &OsStr,
    max_identity_bytes: u64,
) -> super::CandidateResult<[u8; 32]> {
    let file = linux_nonce_open_relative_readonly(directory, name)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "retirement target missing".to_owned())?;
    encrypted_stage_file_identity(&file, max_identity_bytes)
        .map(|(identity, _)| identity)
        .map_err(|error| error.to_string())
}

fn prepare_encrypted_restart_retirement(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    durable: &DurableEncryptedRestartPublication,
    limits: LinuxEncryptedStageRestartLimits,
) -> super::CandidateResult<EncryptedRestartRetirementRecord> {
    let crashed_generation = durable.continuation.crashed_generation;
    let fresh_generation = durable.continuation.fresh_generation;
    let recovery = journal.scan(None).map_err(|error| error.to_string())?;
    if recovery.durable.generation != fresh_generation {
        return Err("retirement fresh generation".into());
    }
    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let manifest = load_encrypted_stage_manifest(journal, crashed_generation, role)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "retirement manifest missing".to_owned())?;
    if manifest.key_id != journal.key_id || manifest.nonce_prefix != journal.nonce_prefix {
        return Err("retirement manifest context".into());
    }

    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| error.to_string())?;
    let stage_name = encrypted_stage_file_name(crashed_generation, role);
    let stage_report = scan_encrypted_stage_inventory(
        &stage_directory,
        &stage_name,
        manifest.identity(),
        limits,
    )
    .map_err(|error| error.to_string())?;
    let stage_actual_name = match stage_report.observation {
        crate::private_cleanup_restart_inventory::InventoryObservation::ExactIdentity => stage_name,
        crate::private_cleanup_restart_inventory::InventoryObservation::MissingMatchingIdentityElsewhere => {
            stage_report
                .matched_name
                .ok_or_else(|| "retirement matched stage name".to_owned())?
        }
        _ => return Err("retirement stage is not exact".into()),
    };
    let stage_identity = exact_file_identity_in_directory(
        &stage_directory,
        &stage_actual_name,
        limits.max_identity_bytes,
    )?;
    if stage_identity != manifest.identity() {
        return Err("retirement stage identity".into());
    }

    let manifest_name = encrypted_stage_manifest_name(crashed_generation, role);
    let manifest_identity = exact_file_identity_in_directory(
        &journal.directory,
        &manifest_name,
        limits.max_identity_bytes,
    )?;
    let record = EncryptedRestartRetirementRecord {
        state: EncryptedRetirementState::Prepared,
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        crashed_generation,
        fresh_generation,
        stage_identity,
        manifest_identity,
        output_length: durable.output_length,
        output_sha256: durable.output_sha256,
    };
    persist_encrypted_retirement_record(journal, record)?;
    Ok(record)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EncryptedRetirementTarget {
    Present(OsString),
    Absent,
}

fn retirement_actionable_target(
    report: &EncryptedStageInventoryReport,
    expected_name: &OsStr,
) -> Option<EncryptedRetirementTarget> {
    match report.observation {
        crate::private_cleanup_restart_inventory::InventoryObservation::ExactIdentity => {
            Some(EncryptedRetirementTarget::Present(expected_name.to_os_string()))
        }
        crate::private_cleanup_restart_inventory::InventoryObservation::MissingMatchingIdentityElsewhere => {
            report
                .matched_name
                .clone()
                .map(EncryptedRetirementTarget::Present)
        }
        crate::private_cleanup_restart_inventory::InventoryObservation::MissingNoMatchingIdentityCompleteScan => {
            Some(EncryptedRetirementTarget::Absent)
        }
        crate::private_cleanup_restart_inventory::InventoryObservation::DifferentIdentity
        | crate::private_cleanup_restart_inventory::InventoryObservation::MissingScanTruncated
        | crate::private_cleanup_restart_inventory::InventoryObservation::NameMetadataUnreadable => None,
    }
}

fn unlink_identity_bound_target(
    directory: &File,
    name: &OsStr,
    expected_identity: [u8; 32],
    max_identity_bytes: u64,
) -> super::CandidateResult<()> {
    let current = exact_file_identity_in_directory(directory, name, max_identity_bytes)?;
    if current != expected_identity {
        return Err("retirement target identity changed".into());
    }
    let path = linux_nonce_procfd_child(directory, name).map_err(|error| error.to_string())?;
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

fn execute_encrypted_restart_retirement(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    crashed_generation: u64,
    fresh_generation: u64,
    limits: LinuxEncryptedStageRestartLimits,
    cut: EncryptedRetirementCut,
) -> super::CandidateResult<EncryptedRetirementOutcome> {
    if load_encrypted_retirement_record(
        journal,
        crashed_generation,
        fresh_generation,
        EncryptedRetirementState::Terminal,
    )?
    .is_some()
    {
        return Ok(EncryptedRetirementOutcome::AlreadyTerminal);
    }
    let Some(prepared) = load_encrypted_retirement_record(
        journal,
        crashed_generation,
        fresh_generation,
        EncryptedRetirementState::Prepared,
    )?
    else {
        return Ok(EncryptedRetirementOutcome::NoPreparedAuthority);
    };
    if cut == EncryptedRetirementCut::AfterPreparedBeforeUnlink {
        return Ok(EncryptedRetirementOutcome::Cut(cut));
    }

    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| error.to_string())?;
    let stage_name = encrypted_stage_file_name(crashed_generation, role);
    let manifest_name = encrypted_stage_manifest_name(crashed_generation, role);
    let stage_report = scan_encrypted_stage_inventory(
        &stage_directory,
        &stage_name,
        prepared.stage_identity,
        limits,
    )
    .map_err(|error| error.to_string())?;
    let manifest_report = scan_encrypted_stage_inventory(
        &journal.directory,
        &manifest_name,
        prepared.manifest_identity,
        limits,
    )
    .map_err(|error| error.to_string())?;
    let Some(stage_action) = retirement_actionable_target(&stage_report, &stage_name) else {
        return Ok(EncryptedRetirementOutcome::RetainIndeterminate);
    };
    let Some(manifest_action) = retirement_actionable_target(&manifest_report, &manifest_name) else {
        return Ok(EncryptedRetirementOutcome::RetainIndeterminate);
    };

    if let EncryptedRetirementTarget::Present(name) = stage_action {
        unlink_identity_bound_target(
            &stage_directory,
            &name,
            prepared.stage_identity,
            limits.max_identity_bytes,
        )?;
    }
    if cut == EncryptedRetirementCut::AfterStageUnlinkBeforeDirectorySync {
        return Ok(EncryptedRetirementOutcome::Cut(cut));
    }
    if let EncryptedRetirementTarget::Present(name) = manifest_action {
        unlink_identity_bound_target(
            &journal.directory,
            &name,
            prepared.manifest_identity,
            limits.max_identity_bytes,
        )?;
    }
    if cut == EncryptedRetirementCut::AfterUnlinksBeforeDirectorySync {
        return Ok(EncryptedRetirementOutcome::Cut(cut));
    }

    linux_nonce_verify_procfd_directory(&stage_directory).map_err(|error| error.to_string())?;
    stage_directory.sync_all().map_err(|error| error.to_string())?;
    linux_nonce_verify_procfd_directory(&journal.directory).map_err(|error| error.to_string())?;
    journal.directory.sync_all().map_err(|error| error.to_string())?;
    if cut == EncryptedRetirementCut::AfterDirectorySyncBeforeTerminal {
        return Ok(EncryptedRetirementOutcome::Cut(cut));
    }

    let terminal = prepared.with_state(EncryptedRetirementState::Terminal);
    persist_encrypted_retirement_record(journal, terminal)?;
    Ok(EncryptedRetirementOutcome::Terminal)
}
