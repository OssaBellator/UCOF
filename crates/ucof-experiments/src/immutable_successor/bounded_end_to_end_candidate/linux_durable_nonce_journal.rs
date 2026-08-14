use aws_lc_rs::hmac;
use std::ffi::{OsStr, OsString};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

const LINUX_NONCE_JOURNAL_MAGIC: &[u8; 8] = b"UCOFNJ02";
const LINUX_NONCE_JOURNAL_VERSION: u8 = 1;
const LINUX_NONCE_JOURNAL_PLAINTEXT_BYTES: usize = 96;
const LINUX_NONCE_JOURNAL_TAG_BYTES: usize = 32;
const LINUX_NONCE_JOURNAL_BYTES: usize =
    LINUX_NONCE_JOURNAL_PLAINTEXT_BYTES + LINUX_NONCE_JOURNAL_TAG_BYTES;
const LINUX_NONCE_JOURNAL_PREFIX: &str = ".ucof-nonce-journal-v1-";
const LINUX_NONCE_JOURNAL_SUFFIX: &str = ".bin";
const LINUX_NONCE_JOURNAL_KEY_ID_DOMAIN: &[u8] = b"UCOF-EXP-0172-KEY-ID\0";
const LINUX_O_DIRECTORY: i32 = 0o200000;
const LINUX_O_NOFOLLOW: i32 = 0o400000;
const LINUX_O_CLOEXEC: i32 = 0o2000000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinuxNonceJournalLimits {
    max_directory_entries: usize,
    max_generations: usize,
    max_journal_bytes: u64,
    max_lease_size: u64,
}

impl Default for LinuxNonceJournalLimits {
    fn default() -> Self {
        Self {
            max_directory_entries: 4096,
            max_generations: 4096,
            max_journal_bytes: 4096 * u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("width"),
            max_lease_size: 1_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TrustedNonceFloor {
    generation: u64,
    next_unreserved: Option<u64>,
}

impl TrustedNonceFloor {
    fn from_authority(authority: &DescriptorNonceAuthority) -> Self {
        Self {
            generation: authority.durable.generation,
            next_unreserved: authority.durable.next_unreserved,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JournalCommitCut {
    AfterWriteBeforeFileSync,
    AfterFileSyncBeforeDirectorySync,
    Complete,
}

#[derive(Debug, PartialEq, Eq)]
enum LinuxNonceJournalError {
    Invalid(&'static str),
    Io(&'static str),
    AuthenticationFailed,
    ForeignKey,
    ForeignNoncePrefix,
    GenerationGap,
    LeaseRange,
    Rollback,
    StaleAuthority,
    Limit(&'static str),
    InjectedCut(JournalCommitCut),
    Lease(String),
}

impl std::fmt::Display for LinuxNonceJournalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(label) => write!(formatter, "invalid nonce journal: {label}"),
            Self::Io(label) => write!(formatter, "nonce journal I/O failed: {label}"),
            Self::AuthenticationFailed => write!(formatter, "nonce journal authentication failed"),
            Self::ForeignKey => write!(formatter, "nonce journal belongs to a different key"),
            Self::ForeignNoncePrefix => {
                write!(formatter, "nonce journal belongs to a different nonce prefix")
            }
            Self::GenerationGap => write!(formatter, "nonce journal generation gap"),
            Self::LeaseRange => write!(formatter, "nonce journal lease range mismatch"),
            Self::Rollback => write!(formatter, "nonce journal is below trusted freshness floor"),
            Self::StaleAuthority => write!(formatter, "nonce authority is stale"),
            Self::Limit(label) => write!(formatter, "nonce journal limit exceeded: {label}"),
            Self::InjectedCut(cut) => write!(formatter, "injected nonce journal cut: {cut:?}"),
            Self::Lease(error) => write!(formatter, "nonce lease failed: {error}"),
        }
    }
}

impl std::error::Error for LinuxNonceJournalError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinuxNonceJournalRecord {
    key_id: [u8; 16],
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    generation: u64,
    lease_first: u64,
    lease_last: u64,
    next_unreserved: Option<u64>,
}

impl LinuxNonceJournalRecord {
    fn from_pending(
        key_id: [u8; 16],
        nonce_prefix: [u8; 4],
        operation_id: [u8; 16],
        pending: PendingNonceLease,
    ) -> Result<Self, LinuxNonceJournalError> {
        if operation_id == [0; 16] {
            return Err(LinuxNonceJournalError::Invalid("operation id"));
        }
        Ok(Self {
            key_id,
            nonce_prefix,
            operation_id,
            generation: pending.committed.generation,
            lease_first: pending.first,
            lease_last: pending.last,
            next_unreserved: pending.committed.next_unreserved,
        })
    }

    fn validate(&self) -> Result<(), LinuxNonceJournalError> {
        if self.key_id == [0; 16] || self.operation_id == [0; 16] || self.generation == 0 {
            return Err(LinuxNonceJournalError::Invalid("journal identity"));
        }
        if self.lease_first > self.lease_last {
            return Err(LinuxNonceJournalError::LeaseRange);
        }
        let expected_next = self.lease_last.checked_add(1);
        if self.next_unreserved != expected_next {
            return Err(LinuxNonceJournalError::LeaseRange);
        }
        Ok(())
    }

    fn encode(&self) -> Result<[u8; LINUX_NONCE_JOURNAL_PLAINTEXT_BYTES], LinuxNonceJournalError> {
        self.validate()?;
        let mut bytes = [0u8; LINUX_NONCE_JOURNAL_PLAINTEXT_BYTES];
        bytes[..8].copy_from_slice(LINUX_NONCE_JOURNAL_MAGIC);
        bytes[8] = LINUX_NONCE_JOURNAL_VERSION;
        bytes[9] = u8::from(self.next_unreserved.is_some());
        bytes[16..32].copy_from_slice(&self.key_id);
        bytes[32..36].copy_from_slice(&self.nonce_prefix);
        bytes[40..56].copy_from_slice(&self.operation_id);
        bytes[56..64].copy_from_slice(&self.generation.to_le_bytes());
        bytes[64..72].copy_from_slice(&self.lease_first.to_le_bytes());
        bytes[72..80].copy_from_slice(&self.lease_last.to_le_bytes());
        if let Some(next) = self.next_unreserved {
            bytes[80..88].copy_from_slice(&next.to_le_bytes());
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, LinuxNonceJournalError> {
        if bytes.len() != LINUX_NONCE_JOURNAL_PLAINTEXT_BYTES {
            return Err(LinuxNonceJournalError::Invalid("exact length"));
        }
        if &bytes[..8] != LINUX_NONCE_JOURNAL_MAGIC {
            return Err(LinuxNonceJournalError::Invalid("magic"));
        }
        if bytes[8] != LINUX_NONCE_JOURNAL_VERSION {
            return Err(LinuxNonceJournalError::Invalid("version"));
        }
        if bytes[9] > 1
            || bytes[10..16].iter().any(|byte| *byte != 0)
            || bytes[36..40].iter().any(|byte| *byte != 0)
            || bytes[88..96].iter().any(|byte| *byte != 0)
        {
            return Err(LinuxNonceJournalError::Invalid("reserved bytes"));
        }
        let next_encoded = u64::from_le_bytes(
            bytes[80..88]
                .try_into()
                .expect("nonce journal next counter field"),
        );
        let next_unreserved = if bytes[9] == 1 {
            Some(next_encoded)
        } else {
            if next_encoded != 0 {
                return Err(LinuxNonceJournalError::Invalid("exhausted counter encoding"));
            }
            None
        };
        let record = Self {
            key_id: bytes[16..32].try_into().expect("nonce journal key id"),
            nonce_prefix: bytes[32..36].try_into().expect("nonce journal prefix"),
            operation_id: bytes[40..56]
                .try_into()
                .expect("nonce journal operation id"),
            generation: u64::from_le_bytes(
                bytes[56..64]
                    .try_into()
                    .expect("nonce journal generation"),
            ),
            lease_first: u64::from_le_bytes(
                bytes[64..72]
                    .try_into()
                    .expect("nonce journal lease first"),
            ),
            lease_last: u64::from_le_bytes(
                bytes[72..80]
                    .try_into()
                    .expect("nonce journal lease last"),
            ),
            next_unreserved,
        };
        record.validate()?;
        Ok(record)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinuxNonceJournalRecovery {
    durable: DurableNonceState,
    generations: usize,
    bytes_read: u64,
}

struct LinuxDurableNonceJournal {
    directory: File,
    key_id: [u8; 16],
    nonce_prefix: [u8; 4],
    journal_auth_key: [u8; 32],
    limits: LinuxNonceJournalLimits,
}

impl LinuxDurableNonceJournal {
    fn open(
        directory: &Path,
        aes_key: &[u8; 32],
        nonce_prefix: [u8; 4],
        journal_auth_key: [u8; 32],
        limits: LinuxNonceJournalLimits,
    ) -> Result<Self, LinuxNonceJournalError> {
        if limits.max_directory_entries == 0
            || limits.max_generations == 0
            || limits.max_journal_bytes < u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("width")
            || limits.max_lease_size == 0
        {
            return Err(LinuxNonceJournalError::Invalid("limits"));
        }
        let directory = linux_nonce_open_private_directory(directory)?;
        Ok(Self {
            directory,
            key_id: linux_nonce_key_id(aes_key),
            nonce_prefix,
            journal_auth_key,
            limits,
        })
    }

    fn recover_authority(
        &self,
        trusted_floor: Option<TrustedNonceFloor>,
    ) -> Result<DescriptorNonceAuthority, LinuxNonceJournalError> {
        let recovery = self.scan(trusted_floor)?;
        Ok(DescriptorNonceAuthority {
            durable: recovery.durable,
        })
    }

    fn scan(
        &self,
        trusted_floor: Option<TrustedNonceFloor>,
    ) -> Result<LinuxNonceJournalRecovery, LinuxNonceJournalError> {
        linux_nonce_verify_procfd_directory(&self.directory)?;
        let mut records = Vec::new();
        let mut directory_entries = 0usize;
        let mut bytes_read = 0u64;
        for entry in std::fs::read_dir(linux_nonce_procfd_directory(&self.directory))
            .map_err(|_| LinuxNonceJournalError::Io("directory scan"))?
        {
            let entry = entry.map_err(|_| LinuxNonceJournalError::Io("directory entry"))?;
            directory_entries = directory_entries
                .checked_add(1)
                .ok_or(LinuxNonceJournalError::Limit("directory entries"))?;
            if directory_entries > self.limits.max_directory_entries {
                return Err(LinuxNonceJournalError::Limit("directory entries"));
            }
            let Some(generation) = linux_nonce_parse_generation_name(&entry.file_name()) else {
                continue;
            };
            if records.len() == self.limits.max_generations {
                return Err(LinuxNonceJournalError::Limit("journal generations"));
            }
            let file = linux_nonce_open_relative_readonly(&self.directory, &entry.file_name())?
                .ok_or(LinuxNonceJournalError::Io("journal disappeared during scan"))?;
            let metadata = file
                .metadata()
                .map_err(|_| LinuxNonceJournalError::Io("journal metadata"))?;
            if !metadata.file_type().is_file()
                || metadata.len()
                    != u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width")
            {
                return Err(LinuxNonceJournalError::Invalid("journal file shape"));
            }
            bytes_read = bytes_read
                .checked_add(metadata.len())
                .ok_or(LinuxNonceJournalError::Limit("journal bytes"))?;
            if bytes_read > self.limits.max_journal_bytes {
                return Err(LinuxNonceJournalError::Limit("journal bytes"));
            }
            let sealed = linux_nonce_read_exact_file(file)?;
            let record = self.open_record(&sealed)?;
            if record.generation != generation {
                return Err(LinuxNonceJournalError::Invalid("filename generation"));
            }
            records.push(record);
        }

        records.sort_unstable_by_key(|record| record.generation);
        let mut durable = DurableNonceState::initial();
        for (index, record) in records.iter().enumerate() {
            let expected_generation = u64::try_from(index)
                .map_err(|_| LinuxNonceJournalError::Limit("journal generations"))?
                .checked_add(1)
                .ok_or(LinuxNonceJournalError::Limit("journal generations"))?;
            if record.generation != expected_generation {
                return Err(LinuxNonceJournalError::GenerationGap);
            }
            if record.key_id != self.key_id {
                return Err(LinuxNonceJournalError::ForeignKey);
            }
            if record.nonce_prefix != self.nonce_prefix {
                return Err(LinuxNonceJournalError::ForeignNoncePrefix);
            }
            if durable.generation.checked_add(1) != Some(record.generation)
                || durable.next_unreserved != Some(record.lease_first)
            {
                return Err(LinuxNonceJournalError::LeaseRange);
            }
            durable = DurableNonceState {
                generation: record.generation,
                next_unreserved: record.next_unreserved,
            };
        }

        if let Some(floor) = trusted_floor {
            if durable.generation < floor.generation
                || !nonce_at_least(floor.next_unreserved, durable.next_unreserved)
            {
                return Err(LinuxNonceJournalError::Rollback);
            }
        }

        Ok(LinuxNonceJournalRecovery {
            durable,
            generations: records.len(),
            bytes_read,
        })
    }

    fn commit_descriptor_session(
        &self,
        authority: &mut DescriptorNonceAuthority,
        aes_key: [u8; 32],
        operation_id: [u8; 16],
        lease_size: u64,
        cut: JournalCommitCut,
    ) -> Result<DescriptorEncryptionSession, LinuxNonceJournalError> {
        if linux_nonce_key_id(&aes_key) != self.key_id {
            return Err(LinuxNonceJournalError::ForeignKey);
        }
        let observed = self.scan(None)?.durable;
        if observed != authority.durable {
            return Err(LinuxNonceJournalError::StaleAuthority);
        }
        let pending = reserve_nonce_lease(
            authority.durable,
            lease_size,
            self.limits.max_lease_size,
        )
        .map_err(|error| LinuxNonceJournalError::Lease(format!("{error:?}")))?;
        let record = LinuxNonceJournalRecord::from_pending(
            self.key_id,
            self.nonce_prefix,
            operation_id,
            pending,
        )?;
        self.persist_record(record, cut)?;
        let (durable, lease) = activate_nonce_lease(authority.durable, pending, true)
            .map_err(|error| LinuxNonceJournalError::Lease(format!("{error:?}")))?;
        authority.durable = durable;
        Ok(DescriptorEncryptionSession {
            key: aes_key,
            nonce_prefix: self.nonce_prefix,
            operation_id,
            journal_generation: durable.generation,
            lease,
        })
    }

    fn persist_record(
        &self,
        record: LinuxNonceJournalRecord,
        cut: JournalCommitCut,
    ) -> Result<(), LinuxNonceJournalError> {
        let sealed = self.seal_record(record)?;
        let name = OsString::from(linux_nonce_journal_name(record.generation));
        let path = linux_nonce_procfd_child(&self.directory, &name)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
            .open(path)
            .map_err(|_| LinuxNonceJournalError::Io("exclusive journal create"))?;
        file.write_all(&sealed)
            .map_err(|_| LinuxNonceJournalError::Io("journal write"))?;
        file.flush()
            .map_err(|_| LinuxNonceJournalError::Io("journal flush"))?;
        if cut == JournalCommitCut::AfterWriteBeforeFileSync {
            return Err(LinuxNonceJournalError::InjectedCut(cut));
        }
        file.sync_all()
            .map_err(|_| LinuxNonceJournalError::Io("journal file sync"))?;
        if cut == JournalCommitCut::AfterFileSyncBeforeDirectorySync {
            return Err(LinuxNonceJournalError::InjectedCut(cut));
        }
        linux_nonce_verify_procfd_directory(&self.directory)?;
        self.directory
            .sync_all()
            .map_err(|_| LinuxNonceJournalError::Io("journal directory sync"))?;
        Ok(())
    }

    fn seal_record(
        &self,
        record: LinuxNonceJournalRecord,
    ) -> Result<[u8; LINUX_NONCE_JOURNAL_BYTES], LinuxNonceJournalError> {
        let plaintext = record.encode()?;
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.journal_auth_key);
        let tag = hmac::sign(&key, &plaintext);
        if tag.as_ref().len() != LINUX_NONCE_JOURNAL_TAG_BYTES {
            return Err(LinuxNonceJournalError::Invalid("HMAC width"));
        }
        let mut sealed = [0u8; LINUX_NONCE_JOURNAL_BYTES];
        sealed[..LINUX_NONCE_JOURNAL_PLAINTEXT_BYTES].copy_from_slice(&plaintext);
        sealed[LINUX_NONCE_JOURNAL_PLAINTEXT_BYTES..].copy_from_slice(tag.as_ref());
        Ok(sealed)
    }

    fn open_record(
        &self,
        sealed: &[u8; LINUX_NONCE_JOURNAL_BYTES],
    ) -> Result<LinuxNonceJournalRecord, LinuxNonceJournalError> {
        let (plaintext, tag) = sealed.split_at(LINUX_NONCE_JOURNAL_PLAINTEXT_BYTES);
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.journal_auth_key);
        hmac::verify(&key, plaintext, tag)
            .map_err(|_| LinuxNonceJournalError::AuthenticationFailed)?;
        LinuxNonceJournalRecord::decode(plaintext)
    }
}

fn linux_nonce_key_id(aes_key: &[u8; 32]) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(LINUX_NONCE_JOURNAL_KEY_ID_DOMAIN);
    hasher.update(aes_key);
    let digest: [u8; 32] = hasher.finalize().into();
    digest[..16].try_into().expect("key-id prefix")
}

fn linux_nonce_effective_uid() -> Result<u32, LinuxNonceJournalError> {
    let status = std::fs::read_to_string("/proc/self/status")
        .map_err(|_| LinuxNonceJournalError::Io("effective uid"))?;
    let line = status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .ok_or(LinuxNonceJournalError::Invalid("effective uid"))?;
    line.split_whitespace()
        .nth(2)
        .ok_or(LinuxNonceJournalError::Invalid("effective uid"))?
        .parse()
        .map_err(|_| LinuxNonceJournalError::Invalid("effective uid"))
}

fn linux_nonce_single_component(name: &OsStr) -> Result<OsString, LinuxNonceJournalError> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes == b"."
        || bytes == b".."
        || bytes.contains(&b'/')
        || bytes.contains(&0)
    {
        return Err(LinuxNonceJournalError::Invalid("journal filename"));
    }
    Ok(name.to_os_string())
}

fn linux_nonce_open_private_directory(path: &Path) -> Result<File, LinuxNonceJournalError> {
    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(LINUX_O_DIRECTORY | LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .map_err(|_| LinuxNonceJournalError::Io("private directory open"))?;
    let metadata = directory
        .metadata()
        .map_err(|_| LinuxNonceJournalError::Io("private directory metadata"))?;
    if !metadata.file_type().is_dir()
        || metadata.uid() != linux_nonce_effective_uid()?
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(LinuxNonceJournalError::Invalid("private directory"));
    }
    linux_nonce_verify_procfd_directory(&directory)?;
    Ok(directory)
}

fn linux_nonce_procfd_directory(directory: &File) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
}

fn linux_nonce_verify_procfd_directory(directory: &File) -> Result<(), LinuxNonceJournalError> {
    let descriptor = directory
        .metadata()
        .map_err(|_| LinuxNonceJournalError::Io("directory descriptor metadata"))?;
    let procfd = std::fs::metadata(linux_nonce_procfd_directory(directory))
        .map_err(|_| LinuxNonceJournalError::Io("procfd directory metadata"))?;
    if descriptor.dev() != procfd.dev() || descriptor.ino() != procfd.ino() {
        return Err(LinuxNonceJournalError::Invalid("procfd directory identity"));
    }
    Ok(())
}

fn linux_nonce_procfd_child(
    directory: &File,
    name: &OsStr,
) -> Result<std::path::PathBuf, LinuxNonceJournalError> {
    let name = linux_nonce_single_component(name)?;
    linux_nonce_verify_procfd_directory(directory)?;
    Ok(linux_nonce_procfd_directory(directory).join(name))
}

fn linux_nonce_open_relative_readonly(
    directory: &File,
    name: &OsStr,
) -> Result<Option<File>, LinuxNonceJournalError> {
    let path = linux_nonce_procfd_child(directory, name)?;
    match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
    {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(LinuxNonceJournalError::Io("relative journal open")),
    }
}

fn linux_nonce_journal_name(generation: u64) -> String {
    format!(
        "{LINUX_NONCE_JOURNAL_PREFIX}{generation:020}{LINUX_NONCE_JOURNAL_SUFFIX}"
    )
}

fn linux_nonce_parse_generation_name(name: &OsStr) -> Option<u64> {
    let name = name.to_str()?;
    let digits = name
        .strip_prefix(LINUX_NONCE_JOURNAL_PREFIX)?
        .strip_suffix(LINUX_NONCE_JOURNAL_SUFFIX)?;
    if digits.len() != 20 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let generation = digits.parse::<u64>().ok()?;
    if generation == 0 || linux_nonce_journal_name(generation) != name {
        return None;
    }
    Some(generation)
}

fn linux_nonce_read_exact_file(
    mut file: File,
) -> Result<[u8; LINUX_NONCE_JOURNAL_BYTES], LinuxNonceJournalError> {
    let mut bytes = [0u8; LINUX_NONCE_JOURNAL_BYTES];
    file.read_exact(&mut bytes)
        .map_err(|_| LinuxNonceJournalError::Io("journal read"))?;
    let mut trailing = [0u8; 1];
    if file
        .read(&mut trailing)
        .map_err(|_| LinuxNonceJournalError::Io("journal exact end"))?
        != 0
    {
        return Err(LinuxNonceJournalError::Invalid("journal exact end"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod linux_durable_nonce_journal_tests {
    use super::*;

    fn private_directory(label: &str) -> super::super::TestDirectory {
        let directory = super::super::TestDirectory::new(label);
        let mut permissions = std::fs::metadata(&directory.0)
            .expect("private directory metadata")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&directory.0, permissions).expect("private directory permissions");
        directory
    }

    fn journal<'a>(
        directory: &'a Path,
        aes_key: &[u8; 32],
        prefix: [u8; 4],
    ) -> LinuxDurableNonceJournal {
        LinuxDurableNonceJournal::open(
            directory,
            aes_key,
            prefix,
            [0x5a; 32],
            LinuxNonceJournalLimits::default(),
        )
        .expect("open durable nonce journal")
    }

    #[test]
    fn durable_journal_authorizes_real_encrypted_spill_writer_and_restart_burns_lease() {
        const OBJECTS: u64 = 17;
        let directory = private_directory("durable-nonce-writer");
        let aes_key = [0xc1; 32];
        let prefix = [0x31; 4];
        let lease_size = OBJECTS.checked_mul(2).expect("nonce uses");
        let journal = journal(&directory.0, &aes_key, prefix);
        let mut authority = journal.recover_authority(None).expect("initial authority");
        let mut session = journal
            .commit_descriptor_session(
                &mut authority,
                aes_key,
                [0x41; 16],
                lease_size,
                JournalCommitCut::Complete,
            )
            .expect("durably committed session");

        let limits = super::super::ImmutableLimits::default();
        let original: Vec<_> = (1..=OBJECTS)
            .rev()
            .map(super::super::TinySource::new)
            .collect();
        let mut baseline_sources = original.clone();
        let mut baseline = Vec::new();
        let baseline_report = super::super::write_genesis_sources_to(
            &mut baseline,
            &mut baseline_sources,
            super::super::options(),
            limits,
        )
        .expect("baseline writer");
        let mut sources = original.clone();
        let mut output = Vec::new();
        let evidence = write_genesis_sources_end_to_end_encrypted_spill_candidate(
            &mut output,
            &mut sources,
            &directory.0,
            super::super::options(),
            limits,
            super::super::spill_limits(5, 2),
            &mut session,
        )
        .expect("journal-authorized encrypted writer");
        assert_eq!(output, baseline);
        assert_eq!(evidence.output.output, baseline_report);
        assert_eq!(session.remaining(), 0);
        assert_eq!(authority.next_unreserved(), Some(lease_size));
        assert_eq!(session.journal_generation, 1);

        drop(journal);
        let restarted = journal(&directory.0, &aes_key, prefix);
        let mut restarted_authority = restarted
            .recover_authority(None)
            .expect("restart authority");
        assert_eq!(restarted_authority.durable.generation, 1);
        assert_eq!(restarted_authority.next_unreserved(), Some(lease_size));
        let mut second_session = restarted
            .commit_descriptor_session(
                &mut restarted_authority,
                aes_key,
                [0x42; 16],
                lease_size,
                JournalCommitCut::Complete,
            )
            .expect("second operation lease");
        assert_eq!(second_session.lease.first, lease_size);
        assert_eq!(second_session.journal_generation, 2);

        let mut second_sources = original;
        let mut second_output = Vec::new();
        let second_evidence = write_genesis_sources_end_to_end_encrypted_spill_candidate(
            &mut second_output,
            &mut second_sources,
            &directory.0,
            super::super::options(),
            limits,
            super::super::spill_limits(7, 3),
            &mut second_session,
        )
        .expect("second journal-authorized encrypted writer");
        assert_eq!(second_output, baseline);
        assert_eq!(second_evidence.output.output, baseline_report);
        assert_eq!(second_session.remaining(), 0);
        assert_eq!(restarted_authority.next_unreserved(), Some(lease_size * 2));
        assert_eq!(restarted.scan(None).expect("final scan").generations, 2);
        directory.assert_empty();
    }

    #[test]
    fn pre_directory_sync_cuts_never_return_an_issuable_session() {
        let aes_key = [0xd1; 32];
        let prefix = [0x32; 4];
        for (label, cut) in [
            ("journal-cut-write", JournalCommitCut::AfterWriteBeforeFileSync),
            (
                "journal-cut-file-sync",
                JournalCommitCut::AfterFileSyncBeforeDirectorySync,
            ),
        ] {
            let directory = private_directory(label);
            let journal = journal(&directory.0, &aes_key, prefix);
            let mut authority = journal.recover_authority(None).expect("initial authority");
            let error = journal
                .commit_descriptor_session(&mut authority, aes_key, [0x51; 16], 4, cut)
                .expect_err("cut must not activate session");
            assert_eq!(error, LinuxNonceJournalError::InjectedCut(cut));
            assert_eq!(authority.durable, DurableNonceState::initial());

            let visible = journal.scan(None).expect("visible candidate is safe to burn");
            assert_eq!(visible.durable.generation, 1);
            assert_eq!(visible.durable.next_unreserved, Some(4));
            directory.assert_empty();
        }
    }

    #[test]
    fn lost_pre_sync_candidate_can_be_reused_only_because_no_session_was_issued() {
        let directory = private_directory("journal-lost-pre-sync");
        let aes_key = [0xe1; 32];
        let prefix = [0x33; 4];
        let journal = journal(&directory.0, &aes_key, prefix);
        let mut authority = journal.recover_authority(None).expect("initial authority");
        let error = journal
            .commit_descriptor_session(
                &mut authority,
                aes_key,
                [0x61; 16],
                4,
                JournalCommitCut::AfterWriteBeforeFileSync,
            )
            .expect_err("pre-sync cut");
        assert_eq!(
            error,
            LinuxNonceJournalError::InjectedCut(JournalCommitCut::AfterWriteBeforeFileSync)
        );
        assert_eq!(authority.next_unreserved(), Some(0));

        std::fs::remove_file(directory.0.join(linux_nonce_journal_name(1)))
            .expect("simulate lost uncommitted generation");
        journal.directory.sync_all().expect("sync simulated loss");
        let mut recovered = journal
            .recover_authority(None)
            .expect("recover initial after lost candidate");
        let session = journal
            .commit_descriptor_session(
                &mut recovered,
                aes_key,
                [0x62; 16],
                4,
                JournalCommitCut::Complete,
            )
            .expect("reuse never-issued counters");
        assert_eq!(session.lease.first, 0);
        assert_eq!(recovered.next_unreserved(), Some(4));
        directory.assert_empty();
    }

    #[test]
    fn tamper_and_generation_gap_fail_closed() {
        let directory = private_directory("journal-tamper-gap");
        let aes_key = [0xf1; 32];
        let prefix = [0x34; 4];
        let journal = journal(&directory.0, &aes_key, prefix);
        let mut authority = journal.recover_authority(None).expect("initial authority");
        let _first = journal
            .commit_descriptor_session(
                &mut authority,
                aes_key,
                [0x71; 16],
                4,
                JournalCommitCut::Complete,
            )
            .expect("first lease");
        let _second = journal
            .commit_descriptor_session(
                &mut authority,
                aes_key,
                [0x72; 16],
                4,
                JournalCommitCut::Complete,
            )
            .expect("second lease");

        let first_path = directory.0.join(linux_nonce_journal_name(1));
        let mut first = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&first_path)
            .expect("open first journal");
        std::io::Seek::seek(&mut first, std::io::SeekFrom::Start(64)).expect("seek journal");
        let mut byte = [0u8; 1];
        first.read_exact(&mut byte).expect("read journal byte");
        byte[0] ^= 0x80;
        std::io::Seek::seek(&mut first, std::io::SeekFrom::Start(64)).expect("seek journal");
        first.write_all(&byte).expect("tamper journal");
        first.sync_all().expect("sync tamper");
        assert_eq!(
            journal.scan(None).expect_err("tamper must fail"),
            LinuxNonceJournalError::AuthenticationFailed
        );

        drop(first);
        std::fs::remove_file(&first_path).expect("remove first generation");
        journal.directory.sync_all().expect("sync gap");
        assert_eq!(
            journal.scan(None).expect_err("gap must fail"),
            LinuxNonceJournalError::GenerationGap
        );
    }

    #[test]
    fn external_floor_detects_tail_rollback_that_self_journal_cannot_prove() {
        let directory = private_directory("journal-tail-rollback");
        let aes_key = [0xa2; 32];
        let prefix = [0x35; 4];
        let journal = journal(&directory.0, &aes_key, prefix);
        let mut authority = journal.recover_authority(None).expect("initial authority");
        let _first = journal
            .commit_descriptor_session(
                &mut authority,
                aes_key,
                [0x81; 16],
                4,
                JournalCommitCut::Complete,
            )
            .expect("first lease");
        let _second = journal
            .commit_descriptor_session(
                &mut authority,
                aes_key,
                [0x82; 16],
                4,
                JournalCommitCut::Complete,
            )
            .expect("second lease");
        let floor = TrustedNonceFloor::from_authority(&authority);
        assert_eq!(floor.generation, 2);
        assert_eq!(floor.next_unreserved, Some(8));

        std::fs::remove_file(directory.0.join(linux_nonce_journal_name(2)))
            .expect("simulate authenticated tail rollback");
        journal.directory.sync_all().expect("sync rollback");
        let self_only = journal
            .recover_authority(None)
            .expect("self journal cannot know deleted tail existed");
        assert_eq!(self_only.durable.generation, 1);
        assert_eq!(self_only.next_unreserved(), Some(4));
        assert_eq!(
            journal
                .recover_authority(Some(floor))
                .expect_err("trusted floor must detect rollback"),
            LinuxNonceJournalError::Rollback
        );
    }

    #[test]
    fn key_prefix_and_stale_authority_are_fail_closed() {
        let directory = private_directory("journal-binding-stale");
        let aes_key = [0xb2; 32];
        let prefix = [0x36; 4];
        let journal = journal(&directory.0, &aes_key, prefix);
        let mut first_authority = journal.recover_authority(None).expect("first authority");
        let mut stale_authority = journal.recover_authority(None).expect("stale authority");
        let _session = journal
            .commit_descriptor_session(
                &mut first_authority,
                aes_key,
                [0x91; 16],
                4,
                JournalCommitCut::Complete,
            )
            .expect("winning lease");
        assert_eq!(
            journal
                .commit_descriptor_session(
                    &mut stale_authority,
                    aes_key,
                    [0x92; 16],
                    4,
                    JournalCommitCut::Complete,
                )
                .expect_err("stale authority"),
            LinuxNonceJournalError::StaleAuthority
        );
        assert_eq!(
            journal
                .commit_descriptor_session(
                    &mut first_authority,
                    [0xc2; 32],
                    [0x93; 16],
                    4,
                    JournalCommitCut::Complete,
                )
                .expect_err("wrong AES key"),
            LinuxNonceJournalError::ForeignKey
        );

        drop(journal);
        let wrong_prefix = journal(&directory.0, &aes_key, [0x37; 4]);
        assert_eq!(
            wrong_prefix
                .recover_authority(None)
                .expect_err("wrong prefix"),
            LinuxNonceJournalError::ForeignNoncePrefix
        );
    }
}
