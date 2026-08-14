const RESTART_SOURCE_SET_MAGIC: &[u8; 8] = b"UCOFSRC1";
const RESTART_SOURCE_SET_VERSION: u8 = 1;
const RESTART_SOURCE_SET_BODY_BYTES: usize = 144;
const RESTART_SOURCE_SET_TAG_BYTES: usize = 32;
const RESTART_SOURCE_SET_BYTES: usize = RESTART_SOURCE_SET_BODY_BYTES + RESTART_SOURCE_SET_TAG_BYTES;
const RESTART_SOURCE_SET_PREFIX: &str = ".ucof-restart-source-set-v1-";
const RESTART_SOURCE_SET_SUFFIX: &str = ".bin";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestartSourceSetAuthority {
    role: EncryptedRestartStageRole,
    key_id: [u8; 16],
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    generation: u64,
    stage_identity: [u8; 32],
    source_set_id: [u8; 32],
    object_count: u64,
}

impl RestartSourceSetAuthority {
    fn encode(&self) -> super::CandidateResult<[u8; RESTART_SOURCE_SET_BODY_BYTES]> {
        if self.key_id == [0; 16]
            || self.operation_id == [0; 16]
            || self.generation == 0
            || self.stage_identity == [0; 32]
            || self.source_set_id == [0; 32]
            || self.object_count == 0
        {
            return Err("restart source-set authority fields".into());
        }
        let mut bytes = [0u8; RESTART_SOURCE_SET_BODY_BYTES];
        bytes[..8].copy_from_slice(RESTART_SOURCE_SET_MAGIC);
        bytes[8] = RESTART_SOURCE_SET_VERSION;
        bytes[9] = self.role as u8;
        bytes[16..32].copy_from_slice(&self.key_id);
        bytes[32..36].copy_from_slice(&self.nonce_prefix);
        bytes[40..56].copy_from_slice(&self.operation_id);
        bytes[56..64].copy_from_slice(&self.generation.to_le_bytes());
        bytes[64..96].copy_from_slice(&self.stage_identity);
        bytes[96..128].copy_from_slice(&self.source_set_id);
        bytes[128..136].copy_from_slice(&self.object_count.to_le_bytes());
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> super::CandidateResult<Self> {
        if bytes.len() != RESTART_SOURCE_SET_BODY_BYTES {
            return Err("restart source-set authority length".into());
        }
        if &bytes[..8] != RESTART_SOURCE_SET_MAGIC || bytes[8] != RESTART_SOURCE_SET_VERSION {
            return Err("restart source-set authority header".into());
        }
        if bytes[10..16].iter().any(|byte| *byte != 0)
            || bytes[36..40].iter().any(|byte| *byte != 0)
            || bytes[136..144].iter().any(|byte| *byte != 0)
        {
            return Err("restart source-set authority reserved bytes".into());
        }
        let role = EncryptedRestartStageRole::from_byte(bytes[9]).map_err(|error| error.to_string())?;
        let record = Self {
            role,
            key_id: bytes[16..32].try_into().expect("source-set key id"),
            nonce_prefix: bytes[32..36].try_into().expect("source-set nonce prefix"),
            operation_id: bytes[40..56].try_into().expect("source-set operation id"),
            generation: u64::from_le_bytes(
                bytes[56..64].try_into().expect("source-set generation"),
            ),
            stage_identity: bytes[64..96].try_into().expect("source-set stage identity"),
            source_set_id: bytes[96..128].try_into().expect("source-set identity"),
            object_count: u64::from_le_bytes(
                bytes[128..136].try_into().expect("source-set object count"),
            ),
        };
        record.encode()?;
        Ok(record)
    }
}

fn restart_source_set_authority_name(
    generation: u64,
    role: EncryptedRestartStageRole,
) -> OsString {
    OsString::from(format!(
        "{RESTART_SOURCE_SET_PREFIX}{generation:020}-{}{RESTART_SOURCE_SET_SUFFIX}",
        role.label()
    ))
}

fn seal_restart_source_set_authority(
    journal: &LinuxDurableNonceJournal,
    record: RestartSourceSetAuthority,
) -> super::CandidateResult<[u8; RESTART_SOURCE_SET_BYTES]> {
    let body = record.encode()?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, &journal.journal_auth_key);
    let tag = hmac::sign(&key, &body);
    if tag.as_ref().len() != RESTART_SOURCE_SET_TAG_BYTES {
        return Err("restart source-set HMAC width".into());
    }
    let mut sealed = [0u8; RESTART_SOURCE_SET_BYTES];
    sealed[..RESTART_SOURCE_SET_BODY_BYTES].copy_from_slice(&body);
    sealed[RESTART_SOURCE_SET_BODY_BYTES..].copy_from_slice(tag.as_ref());
    Ok(sealed)
}

fn open_restart_source_set_authority(
    journal: &LinuxDurableNonceJournal,
    sealed: &[u8; RESTART_SOURCE_SET_BYTES],
) -> super::CandidateResult<RestartSourceSetAuthority> {
    let (body, tag) = sealed.split_at(RESTART_SOURCE_SET_BODY_BYTES);
    let key = hmac::Key::new(hmac::HMAC_SHA256, &journal.journal_auth_key);
    hmac::verify(&key, body, tag)
        .map_err(|_| "restart source-set authentication".to_owned())?;
    RestartSourceSetAuthority::decode(body)
}

fn load_restart_source_set_authority(
    journal: &LinuxDurableNonceJournal,
    generation: u64,
    role: EncryptedRestartStageRole,
) -> super::CandidateResult<Option<RestartSourceSetAuthority>> {
    let name = restart_source_set_authority_name(generation, role);
    let Some(file) = linux_nonce_open_relative_readonly(&journal.directory, &name)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file()
        || metadata.len() != u64::try_from(RESTART_SOURCE_SET_BYTES).expect("source-set width")
    {
        return Err("restart source-set file shape".into());
    }
    let mut reader = file;
    let mut sealed = [0u8; RESTART_SOURCE_SET_BYTES];
    reader.read_exact(&mut sealed).map_err(|error| error.to_string())?;
    let mut trailing = [0u8; 1];
    if reader.read(&mut trailing).map_err(|error| error.to_string())? != 0 {
        return Err("restart source-set exact end".into());
    }
    let record = open_restart_source_set_authority(journal, &sealed)?;
    if record.generation != generation || record.role != role {
        return Err("restart source-set foreign generation".into());
    }
    if record.key_id != journal.key_id || record.nonce_prefix != journal.nonce_prefix {
        return Err("restart source-set foreign journal context".into());
    }
    Ok(Some(record))
}

fn persist_restart_source_set_authority(
    journal: &LinuxDurableNonceJournal,
    manifest: LinuxEncryptedStageManifest,
    source_set_id: [u8; 32],
    object_count: usize,
) -> super::CandidateResult<RestartSourceSetAuthority> {
    if source_set_id == [0; 32] || object_count == 0 {
        return Err("restart source-set input".into());
    }
    if manifest.key_id != journal.key_id || manifest.nonce_prefix != journal.nonce_prefix {
        return Err("restart source-set manifest context".into());
    }
    let nonce_record = load_nonce_generation_record(journal, manifest.generation)
        .map_err(|error| error.to_string())?;
    if nonce_record.operation_id != manifest.operation_id {
        return Err("restart source-set operation mismatch".into());
    }
    let recovery = journal.scan(None).map_err(|error| error.to_string())?;
    if recovery.durable.generation != manifest.generation {
        return Err("restart source-set stale generation".into());
    }
    let record = RestartSourceSetAuthority {
        role: manifest.role,
        key_id: manifest.key_id,
        nonce_prefix: manifest.nonce_prefix,
        operation_id: manifest.operation_id,
        generation: manifest.generation,
        stage_identity: manifest.identity(),
        source_set_id,
        object_count: u64::try_from(object_count).map_err(|_| "restart source-set object count")?,
    };
    let sealed = seal_restart_source_set_authority(journal, record)?;
    let name = restart_source_set_authority_name(record.generation, record.role);
    let path = linux_nonce_procfd_child(&journal.directory, &name).map_err(|error| error.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .map_err(|error| format!("restart source-set exclusive create: {error}"))?;
    file.write_all(&sealed).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    linux_nonce_verify_procfd_directory(&journal.directory).map_err(|error| error.to_string())?;
    journal.directory.sync_all().map_err(|error| error.to_string())?;
    Ok(record)
}

fn verify_restart_source_set_authority(
    journal: &LinuxDurableNonceJournal,
    manifest: LinuxEncryptedStageManifest,
    expected_source_set_id: [u8; 32],
    expected_object_count: usize,
) -> super::CandidateResult<RestartSourceSetAuthority> {
    if expected_source_set_id == [0; 32] || expected_object_count == 0 {
        return Err("restart source-set expectation".into());
    }
    let record = load_restart_source_set_authority(journal, manifest.generation, manifest.role)?
        .ok_or_else(|| "restart source-set authority missing".to_owned())?;
    if record.key_id != manifest.key_id
        || record.nonce_prefix != manifest.nonce_prefix
        || record.operation_id != manifest.operation_id
        || record.generation != manifest.generation
        || record.stage_identity != manifest.identity()
        || record.object_count
            != u64::try_from(expected_object_count).map_err(|_| "restart source-set object count")?
    {
        return Err("restart source-set authority mismatch".into());
    }
    if record.source_set_id != expected_source_set_id {
        return Err("restart source-set identity mismatch".into());
    }
    Ok(record)
}
