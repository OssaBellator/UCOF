const ENCRYPTED_STAGE_MANIFEST_MAGIC: &[u8; 8] = b"UCOFST03";
const ENCRYPTED_STAGE_MANIFEST_VERSION: u8 = 1;
const ENCRYPTED_STAGE_MANIFEST_BODY_BYTES: usize = 128;
const ENCRYPTED_STAGE_MANIFEST_TAG_BYTES: usize = 32;
const ENCRYPTED_STAGE_MANIFEST_BYTES: usize =
    ENCRYPTED_STAGE_MANIFEST_BODY_BYTES + ENCRYPTED_STAGE_MANIFEST_TAG_BYTES;
const ENCRYPTED_STAGE_MANIFEST_PREFIX: &str = ".ucof-encrypted-stage-manifest-v1-";
const ENCRYPTED_STAGE_MANIFEST_SUFFIX: &str = ".bin";
const ENCRYPTED_STAGE_FILE_PREFIX: &str = ".ucof-encrypted-stage-v1-";
const ENCRYPTED_STAGE_FILE_SUFFIX: &str = ".bin";
const ENCRYPTED_STAGE_IDENTITY_DOMAIN: &[u8] = b"UCOF-EXP-0173-STAGE-IDENTITY\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncryptedRestartStageRole {
    SortedDescriptorSpill = 1,
}

impl EncryptedRestartStageRole {
    fn from_byte(value: u8) -> Result<Self, LinuxEncryptedStageRestartError> {
        match value {
            1 => Ok(Self::SortedDescriptorSpill),
            _ => Err(LinuxEncryptedStageRestartError::Invalid("stage role")),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SortedDescriptorSpill => "descriptor-spill",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinuxEncryptedStageRestartLimits {
    max_directory_entries: usize,
    max_metadata_bytes: u64,
    max_identity_bytes: u64,
    max_stage_bytes: u64,
    max_stage_records: usize,
}

impl Default for LinuxEncryptedStageRestartLimits {
    fn default() -> Self {
        Self {
            max_directory_entries: 4096,
            max_metadata_bytes: 4 * 1024 * 1024,
            max_identity_bytes: 256 * 1024 * 1024,
            max_stage_bytes: 128 * 1024 * 1024,
            max_stage_records: 1_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncryptedStageManifestCommitCut {
    AfterStageSyncBeforeManifest,
    Complete,
}

#[derive(Debug, PartialEq, Eq)]
enum LinuxEncryptedStageRestartError {
    Invalid(&'static str),
    Io(&'static str),
    AuthenticationFailed,
    ForeignKey,
    ForeignNoncePrefix,
    ForeignOperation,
    ForeignGeneration,
    StaleGeneration,
    StageAuthenticationFailed,
    StageDescriptorInvalid,
    Limit(&'static str),
    InjectedCut(EncryptedStageManifestCommitCut),
    Journal(String),
}

impl std::fmt::Display for LinuxEncryptedStageRestartError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(label) => write!(formatter, "invalid encrypted restart stage: {label}"),
            Self::Io(label) => write!(formatter, "encrypted restart stage I/O failed: {label}"),
            Self::AuthenticationFailed => write!(formatter, "encrypted stage manifest authentication failed"),
            Self::ForeignKey => write!(formatter, "encrypted stage manifest belongs to another key"),
            Self::ForeignNoncePrefix => {
                write!(formatter, "encrypted stage manifest belongs to another nonce prefix")
            }
            Self::ForeignOperation => write!(formatter, "encrypted stage manifest operation mismatch"),
            Self::ForeignGeneration => write!(formatter, "encrypted stage manifest generation mismatch"),
            Self::StaleGeneration => write!(formatter, "encrypted stage is not for the latest journal generation"),
            Self::StageAuthenticationFailed => write!(formatter, "encrypted spill stage authentication failed"),
            Self::StageDescriptorInvalid => write!(formatter, "encrypted spill descriptor is invalid"),
            Self::Limit(label) => write!(formatter, "encrypted restart stage limit exceeded: {label}"),
            Self::InjectedCut(cut) => write!(formatter, "injected encrypted stage cut: {cut:?}"),
            Self::Journal(error) => write!(formatter, "nonce journal failed: {error}"),
        }
    }
}

impl std::error::Error for LinuxEncryptedStageRestartError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinuxEncryptedStageManifest {
    role: EncryptedRestartStageRole,
    key_id: [u8; 16],
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    generation: u64,
    stage_length: u64,
    stage_dev: u64,
    stage_ino: u64,
    stage_sha256: [u8; 32],
}

impl LinuxEncryptedStageManifest {
    fn encode(&self) -> Result<[u8; ENCRYPTED_STAGE_MANIFEST_BODY_BYTES], LinuxEncryptedStageRestartError> {
        if self.key_id == [0; 16]
            || self.operation_id == [0; 16]
            || self.generation == 0
            || self.stage_length == 0
            || self.stage_ino == 0
        {
            return Err(LinuxEncryptedStageRestartError::Invalid("manifest fields"));
        }
        let mut bytes = [0u8; ENCRYPTED_STAGE_MANIFEST_BODY_BYTES];
        bytes[..8].copy_from_slice(ENCRYPTED_STAGE_MANIFEST_MAGIC);
        bytes[8] = ENCRYPTED_STAGE_MANIFEST_VERSION;
        bytes[9] = self.role as u8;
        bytes[16..32].copy_from_slice(&self.key_id);
        bytes[32..36].copy_from_slice(&self.nonce_prefix);
        bytes[40..56].copy_from_slice(&self.operation_id);
        bytes[56..64].copy_from_slice(&self.generation.to_le_bytes());
        bytes[64..72].copy_from_slice(&self.stage_length.to_le_bytes());
        bytes[72..80].copy_from_slice(&self.stage_dev.to_le_bytes());
        bytes[80..88].copy_from_slice(&self.stage_ino.to_le_bytes());
        bytes[88..120].copy_from_slice(&self.stage_sha256);
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, LinuxEncryptedStageRestartError> {
        if bytes.len() != ENCRYPTED_STAGE_MANIFEST_BODY_BYTES {
            return Err(LinuxEncryptedStageRestartError::Invalid("manifest length"));
        }
        if &bytes[..8] != ENCRYPTED_STAGE_MANIFEST_MAGIC {
            return Err(LinuxEncryptedStageRestartError::Invalid("manifest magic"));
        }
        if bytes[8] != ENCRYPTED_STAGE_MANIFEST_VERSION {
            return Err(LinuxEncryptedStageRestartError::Invalid("manifest version"));
        }
        if bytes[10..16].iter().any(|byte| *byte != 0)
            || bytes[36..40].iter().any(|byte| *byte != 0)
            || bytes[120..128].iter().any(|byte| *byte != 0)
        {
            return Err(LinuxEncryptedStageRestartError::Invalid("manifest reserved bytes"));
        }
        let manifest = Self {
            role: EncryptedRestartStageRole::from_byte(bytes[9])?,
            key_id: bytes[16..32].try_into().expect("stage manifest key id"),
            nonce_prefix: bytes[32..36].try_into().expect("stage manifest nonce prefix"),
            operation_id: bytes[40..56].try_into().expect("stage manifest operation id"),
            generation: u64::from_le_bytes(
                bytes[56..64].try_into().expect("stage manifest generation"),
            ),
            stage_length: u64::from_le_bytes(
                bytes[64..72].try_into().expect("stage manifest length"),
            ),
            stage_dev: u64::from_le_bytes(
                bytes[72..80].try_into().expect("stage manifest device"),
            ),
            stage_ino: u64::from_le_bytes(
                bytes[80..88].try_into().expect("stage manifest inode"),
            ),
            stage_sha256: bytes[88..120].try_into().expect("stage manifest digest"),
        };
        manifest.encode()?;
        Ok(manifest)
    }

    fn identity(&self) -> [u8; 32] {
        encrypted_stage_identity_digest(
            self.stage_dev,
            self.stage_ino,
            self.stage_length,
            self.stage_sha256,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EncryptedStageInventoryReport {
    observation: crate::private_cleanup_restart_inventory::InventoryObservation,
    matched_name: Option<OsString>,
    scanned_entries: usize,
    scanned_metadata_bytes: u64,
    scanned_identity_bytes: u64,
    truncated: bool,
    unreadable_entries: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EncryptedStageRestartDisposition {
    NoDurableManifestRestartWork,
    StageAbsentRestartWork,
    VerifiedExactNeedsFreshLease { object_count: usize },
    VerifiedRenamedNeedsFreshLease { object_count: usize, actual_name: OsString },
    RetainIndeterminate,
}

fn encrypted_stage_manifest_name(
    generation: u64,
    role: EncryptedRestartStageRole,
) -> OsString {
    OsString::from(format!(
        "{ENCRYPTED_STAGE_MANIFEST_PREFIX}{generation:020}-{}{ENCRYPTED_STAGE_MANIFEST_SUFFIX}",
        role.label()
    ))
}

fn encrypted_stage_file_name(generation: u64, role: EncryptedRestartStageRole) -> OsString {
    OsString::from(format!(
        "{ENCRYPTED_STAGE_FILE_PREFIX}{generation:020}-{}{ENCRYPTED_STAGE_FILE_SUFFIX}",
        role.label()
    ))
}

fn encrypted_stage_identity_digest(
    dev: u64,
    ino: u64,
    length: u64,
    content_sha256: [u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(ENCRYPTED_STAGE_IDENTITY_DOMAIN);
    hasher.update(dev.to_le_bytes());
    hasher.update(ino.to_le_bytes());
    hasher.update(length.to_le_bytes());
    hasher.update(content_sha256);
    hasher.finalize().into()
}

fn encrypted_stage_metadata_charge(name: &OsStr) -> u64 {
    u64::try_from(name.as_bytes().len())
        .ok()
        .and_then(|name_bytes| 64u64.checked_add(name_bytes))
        .unwrap_or(u64::MAX)
}

fn encrypted_stage_file_digest(
    file: &File,
    max_bytes: u64,
) -> Result<([u8; 32], u64), LinuxEncryptedStageRestartError> {
    let metadata = file
        .metadata()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("stage identity metadata"))?;
    if metadata.len() > max_bytes {
        return Err(LinuxEncryptedStageRestartError::Limit("stage identity bytes"));
    }
    let mut reader = file
        .try_clone()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("stage identity clone"))?;
    std::io::Seek::seek(&mut reader, std::io::SeekFrom::Start(0))
        .map_err(|_| LinuxEncryptedStageRestartError::Io("stage identity seek"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| LinuxEncryptedStageRestartError::Io("stage identity read"))?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(
                u64::try_from(read)
                    .map_err(|_| LinuxEncryptedStageRestartError::Limit("stage identity bytes"))?,
            )
            .ok_or(LinuxEncryptedStageRestartError::Limit("stage identity bytes"))?;
        if total > max_bytes {
            return Err(LinuxEncryptedStageRestartError::Limit("stage identity bytes"));
        }
        hasher.update(&buffer[..read]);
    }
    if total != metadata.len() {
        return Err(LinuxEncryptedStageRestartError::Io("stage identity length"));
    }
    Ok((hasher.finalize().into(), total))
}

fn encrypted_stage_file_identity(
    file: &File,
    max_bytes: u64,
) -> Result<([u8; 32], u64), LinuxEncryptedStageRestartError> {
    let metadata = file
        .metadata()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("stage identity metadata"))?;
    if !metadata.file_type().is_file()
        || metadata.nlink() != 1
        || metadata.uid()
        != linux_nonce_effective_uid()
            .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(LinuxEncryptedStageRestartError::Invalid("stage file invariants"));
    }
    let (content_sha256, bytes) = encrypted_stage_file_digest(file, max_bytes)?;
    Ok((
        encrypted_stage_identity_digest(
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            content_sha256,
        ),
        bytes,
    ))
}

fn seal_encrypted_stage_manifest(
    journal: &LinuxDurableNonceJournal,
    manifest: LinuxEncryptedStageManifest,
) -> Result<[u8; ENCRYPTED_STAGE_MANIFEST_BYTES], LinuxEncryptedStageRestartError> {
    let body = manifest.encode()?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, &journal.journal_auth_key);
    let tag = hmac::sign(&key, &body);
    if tag.as_ref().len() != ENCRYPTED_STAGE_MANIFEST_TAG_BYTES {
        return Err(LinuxEncryptedStageRestartError::Invalid("manifest HMAC width"));
    }
    let mut sealed = [0u8; ENCRYPTED_STAGE_MANIFEST_BYTES];
    sealed[..ENCRYPTED_STAGE_MANIFEST_BODY_BYTES].copy_from_slice(&body);
    sealed[ENCRYPTED_STAGE_MANIFEST_BODY_BYTES..].copy_from_slice(tag.as_ref());
    Ok(sealed)
}

fn open_encrypted_stage_manifest(
    journal: &LinuxDurableNonceJournal,
    sealed: &[u8; ENCRYPTED_STAGE_MANIFEST_BYTES],
) -> Result<LinuxEncryptedStageManifest, LinuxEncryptedStageRestartError> {
    let (body, tag) = sealed.split_at(ENCRYPTED_STAGE_MANIFEST_BODY_BYTES);
    let key = hmac::Key::new(hmac::HMAC_SHA256, &journal.journal_auth_key);
    hmac::verify(&key, body, tag)
        .map_err(|_| LinuxEncryptedStageRestartError::AuthenticationFailed)?;
    LinuxEncryptedStageManifest::decode(body)
}

fn load_nonce_generation_record(
    journal: &LinuxDurableNonceJournal,
    generation: u64,
) -> Result<LinuxNonceJournalRecord, LinuxEncryptedStageRestartError> {
    let name = OsString::from(linux_nonce_journal_name(generation));
    let file = linux_nonce_open_relative_readonly(&journal.directory, &name)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?
        .ok_or(LinuxEncryptedStageRestartError::ForeignGeneration)?;
    let sealed = linux_nonce_read_exact_file(file)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    journal
        .open_record(&sealed)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))
}

fn load_encrypted_stage_manifest(
    journal: &LinuxDurableNonceJournal,
    generation: u64,
    role: EncryptedRestartStageRole,
) -> Result<Option<LinuxEncryptedStageManifest>, LinuxEncryptedStageRestartError> {
    let name = encrypted_stage_manifest_name(generation, role);
    let Some(file) = linux_nonce_open_relative_readonly(&journal.directory, &name)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?
    else {
        return Ok(None);
    };
    let metadata = file
        .metadata()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("manifest metadata"))?;
    if !metadata.file_type().is_file()
        || metadata.len()
            != u64::try_from(ENCRYPTED_STAGE_MANIFEST_BYTES).expect("manifest width")
    {
        return Err(LinuxEncryptedStageRestartError::Invalid("manifest file shape"));
    }
    let mut reader = file;
    let mut sealed = [0u8; ENCRYPTED_STAGE_MANIFEST_BYTES];
    reader
        .read_exact(&mut sealed)
        .map_err(|_| LinuxEncryptedStageRestartError::Io("manifest read"))?;
    let mut trailing = [0u8; 1];
    if reader
        .read(&mut trailing)
        .map_err(|_| LinuxEncryptedStageRestartError::Io("manifest exact end"))?
        != 0
    {
        return Err(LinuxEncryptedStageRestartError::Invalid("manifest exact end"));
    }
    let manifest = open_encrypted_stage_manifest(journal, &sealed)?;
    if manifest.generation != generation || manifest.role != role {
        return Err(LinuxEncryptedStageRestartError::ForeignGeneration);
    }
    if manifest.key_id != journal.key_id {
        return Err(LinuxEncryptedStageRestartError::ForeignKey);
    }
    if manifest.nonce_prefix != journal.nonce_prefix {
        return Err(LinuxEncryptedStageRestartError::ForeignNoncePrefix);
    }
    Ok(Some(manifest))
}

fn persist_sorted_encrypted_spill_restart_stage(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    preflight: &EncryptedSpillPreflight,
    session: &DescriptorEncryptionSession,
    limits: LinuxEncryptedStageRestartLimits,
    cut: EncryptedStageManifestCommitCut,
) -> Result<LinuxEncryptedStageManifest, LinuxEncryptedStageRestartError> {
    if limits.max_stage_bytes == 0 || limits.max_stage_records == 0 {
        return Err(LinuxEncryptedStageRestartError::Invalid("stage limits"));
    }
    if linux_nonce_key_id(&session.key) != journal.key_id {
        return Err(LinuxEncryptedStageRestartError::ForeignKey);
    }
    if session.nonce_prefix != journal.nonce_prefix {
        return Err(LinuxEncryptedStageRestartError::ForeignNoncePrefix);
    }
    let _mutation = acquire_restart_metadata_mutation_lock(journal)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    let nonce_record = load_nonce_generation_record(journal, session.journal_generation)?;
    if nonce_record.operation_id != session.operation_id {
        return Err(LinuxEncryptedStageRestartError::ForeignOperation);
    }
    let journal_recovery = journal
        .scan(None)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    if journal_recovery.durable.generation != session.journal_generation {
        return Err(LinuxEncryptedStageRestartError::StaleGeneration);
    }
    require_linux_nonce_journal_metadata_slots(journal, 1, "encrypted stage manifest")
        .map_err(LinuxEncryptedStageRestartError::Journal)?;

    let source_bytes = preflight
        .descriptor_stage
        .validate_bytes()
        .map_err(LinuxEncryptedStageRestartError::Journal)?;
    if source_bytes == 0 || source_bytes > limits.max_stage_bytes {
        return Err(LinuxEncryptedStageRestartError::Limit("stage bytes"));
    }
    let records = preflight.descriptor_stage.records;
    if records == 0 || records > limits.max_stage_records {
        return Err(LinuxEncryptedStageRestartError::Limit("stage records"));
    }

    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    let stage_name = encrypted_stage_file_name(
        session.journal_generation,
        EncryptedRestartStageRole::SortedDescriptorSpill,
    );
    let stage_path = linux_nonce_procfd_child(&stage_directory, &stage_name)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    let mut stage_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(stage_path)
        .map_err(|_| LinuxEncryptedStageRestartError::Io("exclusive stage create"))?;
    let mut source = preflight
        .descriptor_stage
        .reader()
        .map_err(LinuxEncryptedStageRestartError::Journal)?;
    let mut hasher = Sha256::new();
    let mut copied = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|_| LinuxEncryptedStageRestartError::Io("stage source read"))?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(
                u64::try_from(read)
                    .map_err(|_| LinuxEncryptedStageRestartError::Limit("stage bytes"))?,
            )
            .ok_or(LinuxEncryptedStageRestartError::Limit("stage bytes"))?;
        if copied > limits.max_stage_bytes || copied > source_bytes {
            return Err(LinuxEncryptedStageRestartError::Limit("stage bytes"));
        }
        stage_file
            .write_all(&buffer[..read])
            .map_err(|_| LinuxEncryptedStageRestartError::Io("stage write"))?;
        hasher.update(&buffer[..read]);
    }
    if copied != source_bytes {
        return Err(LinuxEncryptedStageRestartError::Io("stage source length"));
    }
    stage_file
        .flush()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("stage flush"))?;
    stage_file
        .sync_all()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("stage file sync"))?;
    linux_nonce_verify_procfd_directory(&stage_directory)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    stage_directory
        .sync_all()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("stage directory sync"))?;

    let metadata = stage_file
        .metadata()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("stage metadata"))?;
    if !metadata.file_type().is_file()
        || metadata.nlink() != 1
        || metadata.uid()
        != linux_nonce_effective_uid()
            .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() != copied
    {
        return Err(LinuxEncryptedStageRestartError::Invalid("stage file invariants"));
    }
    let manifest = LinuxEncryptedStageManifest {
        role: EncryptedRestartStageRole::SortedDescriptorSpill,
        key_id: journal.key_id,
        nonce_prefix: journal.nonce_prefix,
        operation_id: session.operation_id,
        generation: session.journal_generation,
        stage_length: copied,
        stage_dev: metadata.dev(),
        stage_ino: metadata.ino(),
        stage_sha256: hasher.finalize().into(),
    };

    if cut == EncryptedStageManifestCommitCut::AfterStageSyncBeforeManifest {
        return Err(LinuxEncryptedStageRestartError::InjectedCut(cut));
    }

    require_linux_nonce_journal_metadata_slots(journal, 1, "encrypted stage manifest")
        .map_err(LinuxEncryptedStageRestartError::Journal)?;
    let manifest_name = encrypted_stage_manifest_name(manifest.generation, manifest.role);
    let manifest_path = linux_nonce_procfd_child(&journal.directory, &manifest_name)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    let sealed = seal_encrypted_stage_manifest(journal, manifest)?;
    let mut manifest_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(manifest_path)
        .map_err(|_| LinuxEncryptedStageRestartError::Io("exclusive manifest create"))?;
    manifest_file
        .write_all(&sealed)
        .map_err(|_| LinuxEncryptedStageRestartError::Io("manifest write"))?;
    manifest_file
        .flush()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("manifest flush"))?;
    manifest_file
        .sync_all()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("manifest file sync"))?;
    linux_nonce_verify_procfd_directory(&journal.directory)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    journal
        .directory
        .sync_all()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("manifest directory sync"))?;
    Ok(manifest)
}

fn scan_encrypted_stage_inventory(
    stage_directory: &File,
    expected_name: &OsStr,
    expected_identity: [u8; 32],
    limits: LinuxEncryptedStageRestartLimits,
) -> Result<EncryptedStageInventoryReport, LinuxEncryptedStageRestartError> {
    if limits.max_directory_entries == 0
        || limits.max_metadata_bytes == 0
        || limits.max_identity_bytes == 0
    {
        return Err(LinuxEncryptedStageRestartError::Invalid("inventory limits"));
    }
    linux_nonce_verify_procfd_directory(stage_directory)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    let entries = std::fs::read_dir(linux_nonce_procfd_directory(stage_directory))
        .map_err(|_| LinuxEncryptedStageRestartError::Io("stage directory scan"))?;
    let mut classified: Vec<(bool, Option<[u8; 32]>, u64)> = Vec::new();
    let mut names: Vec<(OsString, Option<[u8; 32]>)> = Vec::new();
    let mut metadata_bytes = 0u64;
    let mut identity_bytes = 0u64;
    let mut forced_truncated = false;

    for entry in entries {
        if classified.len() >= limits.max_directory_entries {
            forced_truncated = true;
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                classified.push((false, None, 64));
                names.push((OsString::from("<unreadable>"), None));
                continue;
            }
        };
        let name = entry.file_name();
        let charge = encrypted_stage_metadata_charge(&name);
        let Some(next_metadata_bytes) = metadata_bytes.checked_add(charge) else {
            forced_truncated = true;
            break;
        };
        if next_metadata_bytes > limits.max_metadata_bytes {
            forced_truncated = true;
            break;
        }
        metadata_bytes = next_metadata_bytes;
        let is_expected_name = name == expected_name;
        let identity = match linux_nonce_open_relative_readonly(stage_directory, &name) {
            Ok(Some(file)) => {
                let length = file.metadata().ok().map(|metadata| metadata.len());
                match length.and_then(|length| identity_bytes.checked_add(length)) {
                    Some(next_identity_bytes) if next_identity_bytes <= limits.max_identity_bytes => {
                        match encrypted_stage_file_identity(
                            &file,
                            limits.max_identity_bytes - identity_bytes,
                        ) {
                            Ok((identity, consumed)) => {
                                identity_bytes = identity_bytes
                                    .checked_add(consumed)
                                    .ok_or(LinuxEncryptedStageRestartError::Limit(
                                        "stage identity bytes",
                                    ))?;
                                Some(identity)
                            }
                            Err(_) => None,
                        }
                    }
                    _ => None,
                }
            }
            Ok(None) | Err(_) => None,
        };
        classified.push((is_expected_name, identity, charge));
        names.push((name, identity));
    }

    if forced_truncated {
        classified.push((false, None, 1));
        names.push((OsString::from("<truncated>"), None));
    }

    let (observation, scanned_entries, scanned_metadata_bytes, truncated, unreadable_entries) =
        crate::private_cleanup_restart_inventory::classify_external_restart_inventory(
            classified,
            expected_identity,
            limits.max_directory_entries,
            limits.max_metadata_bytes,
        )
        .map_err(|_| LinuxEncryptedStageRestartError::Invalid("inventory classification"))?;
    let matched_name = match observation {
        crate::private_cleanup_restart_inventory::InventoryObservation::ExactIdentity => {
            Some(expected_name.to_os_string())
        }
        crate::private_cleanup_restart_inventory::InventoryObservation::MissingMatchingIdentityElsewhere => {
            names
                .into_iter()
                .find_map(|(name, identity)| (identity == Some(expected_identity)).then_some(name))
        }
        _ => None,
    };
    Ok(EncryptedStageInventoryReport {
        observation,
        matched_name,
        scanned_entries,
        scanned_metadata_bytes,
        scanned_identity_bytes: identity_bytes,
        truncated,
        unreadable_entries,
    })
}

fn verify_manifest_bound_stage_identity(
    file: &File,
    manifest: LinuxEncryptedStageManifest,
    max_identity_bytes: u64,
) -> Result<(), LinuxEncryptedStageRestartError> {
    let (identity, _) = encrypted_stage_file_identity(file, max_identity_bytes)?;
    if identity != manifest.identity() {
        return Err(LinuxEncryptedStageRestartError::Invalid("stage identity changed"));
    }
    Ok(())
}

fn verify_persisted_sorted_encrypted_spill(
    file: &File,
    manifest: LinuxEncryptedStageManifest,
    nonce_record: LinuxNonceJournalRecord,
    aes_key: &[u8; 32],
    limits: LinuxEncryptedStageRestartLimits,
) -> Result<usize, LinuxEncryptedStageRestartError> {
    if manifest.role != EncryptedRestartStageRole::SortedDescriptorSpill
        || manifest.key_id != linux_nonce_key_id(aes_key)
        || nonce_record.key_id != manifest.key_id
    {
        return Err(LinuxEncryptedStageRestartError::ForeignKey);
    }
    if nonce_record.nonce_prefix != manifest.nonce_prefix {
        return Err(LinuxEncryptedStageRestartError::ForeignNoncePrefix);
    }
    if nonce_record.operation_id != manifest.operation_id {
        return Err(LinuxEncryptedStageRestartError::ForeignOperation);
    }
    if nonce_record.generation != manifest.generation {
        return Err(LinuxEncryptedStageRestartError::ForeignGeneration);
    }
    if manifest.stage_length == 0
        || manifest.stage_length > limits.max_stage_bytes
        || manifest.stage_length
            % u64::try_from(ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES).expect("spill width")
            != 0
    {
        return Err(LinuxEncryptedStageRestartError::Invalid("spill stage length"));
    }
    let object_count_u64 = manifest.stage_length
        / u64::try_from(ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES).expect("spill width");
    let object_count = usize::try_from(object_count_u64)
        .map_err(|_| LinuxEncryptedStageRestartError::Limit("stage records"))?;
    if object_count == 0 || object_count > limits.max_stage_records {
        return Err(LinuxEncryptedStageRestartError::Limit("stage records"));
    }
    let expected_lease = object_count_u64
        .checked_mul(2)
        .ok_or(LinuxEncryptedStageRestartError::Limit("lease size"))?;
    let actual_lease = nonce_record
        .lease_last
        .checked_sub(nonce_record.lease_first)
        .and_then(|delta| delta.checked_add(1))
        .ok_or(LinuxEncryptedStageRestartError::Invalid("lease range"))?;
    if actual_lease != expected_lease {
        return Err(LinuxEncryptedStageRestartError::Invalid("lease size"));
    }
    let spill_counter_end = nonce_record
        .lease_first
        .checked_add(object_count_u64)
        .ok_or(LinuxEncryptedStageRestartError::Invalid("spill counter range"))?;

    let mut reader = file
        .try_clone()
        .map_err(|_| LinuxEncryptedStageRestartError::Io("spill verification clone"))?;
    std::io::Seek::seek(&mut reader, std::io::SeekFrom::Start(0))
        .map_err(|_| LinuxEncryptedStageRestartError::Io("spill verification seek"))?;
    let key = descriptor_key(aes_key)
        .map_err(|_| LinuxEncryptedStageRestartError::StageAuthenticationFailed)?;
    let mut seen_counters = vec![false; object_count];
    let mut previous_object_id = None;

    for _ in 0..object_count {
        let mut frame = [0u8; ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES];
        reader
            .read_exact(&mut frame)
            .map_err(|_| LinuxEncryptedStageRestartError::Io("spill verification read"))?;
        let object_id = u64::from_le_bytes(
            frame[..ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES]
                .try_into()
                .expect("spill object id"),
        );
        if object_id == 0
            || previous_object_id.is_some_and(|previous| object_id <= previous)
        {
            return Err(LinuxEncryptedStageRestartError::StageDescriptorInvalid);
        }
        let nonce: [u8; ENCRYPTED_DESCRIPTOR_NONCE_BYTES] = frame
            [ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES
                ..ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES + ENCRYPTED_DESCRIPTOR_NONCE_BYTES]
            .try_into()
            .expect("spill nonce");
        let counter = spill_counter_from_nonce(&nonce, manifest.nonce_prefix)
            .map_err(|_| LinuxEncryptedStageRestartError::StageAuthenticationFailed)?;
        if counter < nonce_record.lease_first || counter >= spill_counter_end {
            return Err(LinuxEncryptedStageRestartError::StageAuthenticationFailed);
        }
        let counter_index = usize::try_from(counter - nonce_record.lease_first)
            .map_err(|_| LinuxEncryptedStageRestartError::Limit("spill counter index"))?;
        if seen_counters[counter_index] {
            return Err(LinuxEncryptedStageRestartError::StageAuthenticationFailed);
        }
        seen_counters[counter_index] = true;
        let aad = descriptor_spill_aad(
            manifest.operation_id,
            manifest.generation,
            object_id,
            counter,
        );
        let mut protected = frame[ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES
            + ENCRYPTED_DESCRIPTOR_NONCE_BYTES..]
            .to_vec();
        let plaintext = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad.as_slice()),
                &mut protected,
            )
            .map_err(|_| LinuxEncryptedStageRestartError::StageAuthenticationFailed)?;
        if plaintext.len() != super::DESCRIPTOR_STAGE_BYTES {
            return Err(LinuxEncryptedStageRestartError::StageDescriptorInvalid);
        }
        let mut descriptor_bytes = [0u8; super::DESCRIPTOR_STAGE_BYTES];
        descriptor_bytes.copy_from_slice(plaintext);
        let descriptor = super::SourceDescriptor::decode(&descriptor_bytes)
            .map_err(|_| LinuxEncryptedStageRestartError::StageDescriptorInvalid)?;
        if descriptor.object_id != object_id {
            return Err(LinuxEncryptedStageRestartError::StageDescriptorInvalid);
        }
        previous_object_id = Some(object_id);
    }
    let mut trailing = [0u8; 1];
    if reader
        .read(&mut trailing)
        .map_err(|_| LinuxEncryptedStageRestartError::Io("spill verification exact end"))?
        != 0
    {
        return Err(LinuxEncryptedStageRestartError::Invalid("spill exact end"));
    }
    if seen_counters.iter().any(|seen| !seen) {
        return Err(LinuxEncryptedStageRestartError::StageAuthenticationFailed);
    }
    Ok(object_count)
}

fn classify_encrypted_spill_restart(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    aes_key: &[u8; 32],
    generation: u64,
    trusted_floor: Option<TrustedNonceFloor>,
    limits: LinuxEncryptedStageRestartLimits,
) -> Result<(EncryptedStageRestartDisposition, Option<EncryptedStageInventoryReport>), LinuxEncryptedStageRestartError> {
    let recovery = journal
        .scan(trusted_floor)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    if recovery.durable.generation != generation {
        return Ok((EncryptedStageRestartDisposition::NoDurableManifestRestartWork, None));
    }
    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let Some(manifest) = load_encrypted_stage_manifest(journal, generation, role)? else {
        return Ok((EncryptedStageRestartDisposition::NoDurableManifestRestartWork, None));
    };
    if manifest.key_id != linux_nonce_key_id(aes_key) {
        return Err(LinuxEncryptedStageRestartError::ForeignKey);
    }
    let nonce_record = load_nonce_generation_record(journal, generation)?;
    if nonce_record.operation_id != manifest.operation_id {
        return Err(LinuxEncryptedStageRestartError::ForeignOperation);
    }

    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?;
    let expected_name = encrypted_stage_file_name(generation, role);
    let report = scan_encrypted_stage_inventory(
        &stage_directory,
        &expected_name,
        manifest.identity(),
        limits,
    )?;
    let disposition = match report.observation {
        crate::private_cleanup_restart_inventory::InventoryObservation::ExactIdentity
        | crate::private_cleanup_restart_inventory::InventoryObservation::MissingMatchingIdentityElsewhere => {
            let actual_name = report
                .matched_name
                .clone()
                .ok_or(LinuxEncryptedStageRestartError::Invalid("matched stage name"))?;
            let file = linux_nonce_open_relative_readonly(&stage_directory, &actual_name)
                .map_err(|error| LinuxEncryptedStageRestartError::Journal(error.to_string()))?
                .ok_or(LinuxEncryptedStageRestartError::Io("matched stage disappeared"))?;
            verify_manifest_bound_stage_identity(&file, manifest, limits.max_identity_bytes)?;
            let object_count = verify_persisted_sorted_encrypted_spill(
                &file,
                manifest,
                nonce_record,
                aes_key,
                limits,
            )?;
            if report.observation
                == crate::private_cleanup_restart_inventory::InventoryObservation::ExactIdentity
            {
                EncryptedStageRestartDisposition::VerifiedExactNeedsFreshLease { object_count }
            } else {
                EncryptedStageRestartDisposition::VerifiedRenamedNeedsFreshLease {
                    object_count,
                    actual_name,
                }
            }
        }
        crate::private_cleanup_restart_inventory::InventoryObservation::MissingNoMatchingIdentityCompleteScan => {
            EncryptedStageRestartDisposition::StageAbsentRestartWork
        }
        crate::private_cleanup_restart_inventory::InventoryObservation::DifferentIdentity
        | crate::private_cleanup_restart_inventory::InventoryObservation::MissingScanTruncated
        | crate::private_cleanup_restart_inventory::InventoryObservation::NameMetadataUnreadable => {
            EncryptedStageRestartDisposition::RetainIndeterminate
        }
    };
    Ok((disposition, Some(report)))
}
