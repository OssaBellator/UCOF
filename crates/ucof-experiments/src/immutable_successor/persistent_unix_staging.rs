use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

fn persistent_unix_staged_name(token: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut name = String::with_capacity(11 + token.len() * 2 + 4);
    name.push_str("ucof-stage-");
    for byte in token {
        name.push(char::from(HEX[usize::from(byte >> 4)]));
        name.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    name.push_str(".tmp");
    name
}

fn persistent_unix_private_directory(path: &Path) -> Result<fs::Metadata, &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "staging directory metadata")?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_dir()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("private staging directory");
    }
    Ok(metadata)
}

fn persistent_unix_destination_directory(path: &Path) -> Result<fs::Metadata, &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "destination directory metadata")?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err("destination directory");
    }
    Ok(metadata)
}

/// Unix research backend for [`PersistentStagingBackend`].
///
/// The backend creates one exclusive mode-0600 file in a private non-symlink staging directory,
/// validates its owner, link count, length, and SHA-256, publishes with a same-filesystem hard link,
/// and synchronizes both the destination and staging directories at their respective durability
/// boundaries. It is path-based rather than descriptor-relative and does not encrypt staged bytes.
pub struct PersistentUnixStagingBackend {
    staging_directory: PathBuf,
    destination: PathBuf,
    ownership_token: [u8; 32],
    staged_path: Option<PathBuf>,
    staged_file: Option<File>,
    staging_uid: Option<u32>,
    expected_length: Option<u64>,
}

impl PersistentUnixStagingBackend {
    pub fn new(
        staging_directory: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        ownership_token: [u8; 32],
    ) -> Self {
        Self {
            staging_directory: staging_directory.into(),
            destination: destination.into(),
            ownership_token,
            staged_path: None,
            staged_file: None,
            staging_uid: None,
            expected_length: None,
        }
    }

    pub fn staged_path(&self) -> Option<&Path> {
        self.staged_path.as_deref()
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }

    fn destination_parent(&self) -> Result<&Path, &'static str> {
        self.destination.parent().ok_or("destination parent")
    }

    fn staged_file_mut(&mut self) -> Result<&mut File, &'static str> {
        self.staged_file.as_mut().ok_or("staged file")
    }

    fn clear_private_state(&mut self) {
        self.staged_file = None;
        self.staged_path = None;
        self.staging_uid = None;
        self.expected_length = None;
    }

    fn sync_staging_directory(&self) -> Result<(), &'static str> {
        File::open(&self.staging_directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "staging directory synchronization")
    }
}

impl Write for PersistentUnixStagingBackend {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.staged_file
            .as_mut()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, "staging not begun")
            })?
            .write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.staged_file
            .as_mut()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, "staging not begun")
            })?
            .flush()
    }
}

impl PersistentStagingBackend for PersistentUnixStagingBackend {
    fn begin_private(&mut self, expected_length: u64) -> Result<(), &'static str> {
        if self.staged_file.is_some() || self.staged_path.is_some() {
            return Err("private staging already active");
        }
        let staging_metadata = persistent_unix_private_directory(&self.staging_directory)?;
        let destination_parent = self.destination_parent()?;
        let destination_metadata = persistent_unix_destination_directory(destination_parent)?;
        if staging_metadata.dev() != destination_metadata.dev() {
            return Err("staging and destination filesystems differ");
        }

        let staged_path = self
            .staging_directory
            .join(persistent_unix_staged_name(&self.ownership_token));
        let staged_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&staged_path)
            .map_err(|_| "exclusive staged file creation")?;
        self.staging_uid = Some(staging_metadata.uid());
        self.expected_length = Some(expected_length);
        self.staged_path = Some(staged_path);
        self.staged_file = Some(staged_file);
        Ok(())
    }

    fn validate_private(
        &mut self,
        expected_length: u64,
        expected_sha256: [u8; 32],
    ) -> Result<(), &'static str> {
        if self.expected_length != Some(expected_length) {
            return Err("staged expected length");
        }
        let staging_uid = self.staging_uid.ok_or("staging owner")?;
        let staged = self.staged_file_mut()?;
        staged.flush().map_err(|_| "staged file flush")?;
        let metadata = staged.metadata().map_err(|_| "staged file metadata")?;
        if !metadata.file_type().is_file()
            || metadata.nlink() != 1
            || metadata.uid() != staging_uid
            || metadata.permissions().mode() & 0o077 != 0
            || metadata.len() != expected_length
        {
            return Err("staged file invariants");
        }

        let mut verifier = staged.try_clone().map_err(|_| "staged validation clone")?;
        verifier
            .seek(SeekFrom::Start(0))
            .map_err(|_| "staged validation seek")?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut total = 0_u64;
        loop {
            let read = verifier
                .read(&mut buffer)
                .map_err(|_| "staged validation read")?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(u64::try_from(read).map_err(|_| "staged validation length")?)
                .ok_or("staged validation length")?;
            hasher.update(&buffer[..read]);
        }
        if total != expected_length || <[u8; 32]>::from(hasher.finalize()) != expected_sha256 {
            return Err("staged content validation");
        }
        Ok(())
    }

    fn sync_private(&mut self) -> Result<(), &'static str> {
        self.staged_file_mut()?
            .sync_all()
            .map_err(|_| "staged file synchronization")
    }

    fn publish_no_replace(&mut self) -> Result<PersistentPublicationLinkOutcome, &'static str> {
        let staged_path = self.staged_path.as_ref().ok_or("staged path")?;
        match fs::hard_link(staged_path, &self.destination) {
            Ok(()) => Ok(PersistentPublicationLinkOutcome::Linked),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Ok(PersistentPublicationLinkOutcome::DestinationExists)
            }
            Err(_) => match (fs::metadata(staged_path), fs::metadata(&self.destination)) {
                (Ok(staged), Ok(destination))
                    if staged.dev() == destination.dev() && staged.ino() == destination.ino() =>
                {
                    Ok(PersistentPublicationLinkOutcome::Indeterminate)
                }
                _ => Err("no-overwrite publication link"),
            },
        }
    }

    fn sync_parent(&mut self) -> Result<(), &'static str> {
        File::open(self.destination_parent()?)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "destination directory synchronization")
    }

    fn retire_private(&mut self) -> Result<(), &'static str> {
        self.staged_file = None;
        let staged_path = self.staged_path.as_ref().ok_or("staged path")?;
        fs::remove_file(staged_path).map_err(|_| "private name retirement")?;
        self.sync_staging_directory()?;
        self.clear_private_state();
        Ok(())
    }

    fn abort_private(&mut self) -> Result<(), &'static str> {
        self.staged_file = None;
        if let Some(staged_path) = &self.staged_path {
            match fs::remove_file(staged_path) {
                Ok(()) => self.sync_staging_directory()?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("private staging abort"),
            }
        }
        self.clear_private_state();
        Ok(())
    }
}

#[cfg(test)]
mod persistent_unix_staging_tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    struct VersionedBytes {
        bytes: Vec<u8>,
        version: PersistentSourceVersion,
        reads: usize,
        mutate_after_read: Option<usize>,
    }

    impl ImmutableReadAt for VersionedBytes {
        fn len(&mut self) -> Result<u64, ImmutableSourceError> {
            u64::try_from(self.bytes.len()).map_err(|_| ImmutableSourceError::Limit("length"))
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            buffer: &mut [u8],
        ) -> Result<(), ImmutableSourceError> {
            let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
            let end = start
                .checked_add(buffer.len())
                .ok_or(ImmutableSourceError::Io("range"))?;
            buffer.copy_from_slice(
                self.bytes
                    .get(start..end)
                    .ok_or(ImmutableSourceError::Io("range"))?,
            );
            self.reads += 1;
            if self.mutate_after_read == Some(self.reads) {
                self.version.0[0] ^= 1;
            }
            Ok(())
        }
    }

    impl PersistentVersionedReadAt for VersionedBytes {
        fn version_token(&mut self) -> Result<PersistentSourceVersion, ImmutableSourceError> {
            Ok(self.version)
        }
    }

    fn test_directory(label: &str) -> PathBuf {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ucof-persistent-stage-{label}-{}-{id}",
            std::process::id()
        ))
    }

    fn private_directory(path: &Path) {
        fs::create_dir_all(path).expect("create private directory");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("private permissions");
    }

    fn source(bytes: Vec<u8>) -> VersionedBytes {
        VersionedBytes {
            bytes,
            version: PersistentSourceVersion([17; 32]),
            reads: 0,
            mutate_after_read: None,
        }
    }

    fn limits(length: usize, chunk: usize) -> ImmutableSourceLimits {
        ImmutableSourceLimits {
            format: ImmutableLimits {
                max_file_bytes: 1024 * 1024,
                max_output_bytes: 1024 * 1024,
                max_allocation_bytes: 1024 * 1024,
                ..ImmutableLimits::default()
            },
            max_read_request_bytes: chunk,
            max_total_bytes_read: u64::try_from(length * 2).expect("budget"),
            max_read_operations: 1_000_000,
            ..ImmutableSourceLimits::default()
        }
    }

    #[test]
    fn publishes_exact_artifact_without_overwrite() {
        let root = test_directory("success");
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        let base = vec![67_u8; 4096];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut backend =
            PersistentUnixStagingBackend::new(&staging, &destination, [1_u8; 32]);
        let report = stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), 127),
            PersistentSourceCopyOptions {
                max_write_request_bytes: 43,
            },
        )
        .expect("publication");
        let mut expected = base;
        expected.extend_from_slice(b"tail");
        assert_eq!(fs::read(&destination).expect("destination"), expected);
        assert_eq!(
            report.outcome,
            PersistentStagedPublicationOutcome::PublishedAndDurable {
                cleanup_pending: false
            }
        );
        assert!(backend.staged_path().is_none());
        assert_eq!(
            fs::metadata(&destination)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o077,
            0
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn existing_destination_is_untouched_and_private_state_is_removed() {
        let root = test_directory("exists");
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        fs::write(&destination, b"old").expect("old destination");
        let base = vec![71_u8; 1024];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut backend =
            PersistentUnixStagingBackend::new(&staging, &destination, [2_u8; 32]);
        let report = stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), 128),
            PersistentSourceCopyOptions::default(),
        )
        .expect("destination exists");
        assert_eq!(
            report.outcome,
            PersistentStagedPublicationOutcome::NotPublishedDestinationExists
        );
        assert_eq!(fs::read(&destination).expect("destination"), b"old");
        assert!(fs::read_dir(&staging)
            .expect("staging")
            .next()
            .is_none());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn symlink_staging_directory_is_rejected_before_file_creation() {
        let root = test_directory("symlink");
        let real = root.join("real");
        let staging = root.join("staging-link");
        let destination_directory = root.join("destination");
        private_directory(&real);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        symlink(&real, &staging).expect("symlink");
        let destination = destination_directory.join("archive.ucof");
        let base = vec![73_u8; 256];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut backend =
            PersistentUnixStagingBackend::new(&staging, &destination, [3_u8; 32]);
        assert!(stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), 64),
            PersistentSourceCopyOptions::default(),
        )
        .is_err());
        assert!(!destination.exists());
        assert!(fs::read_dir(&real).expect("real").next().is_none());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn version_change_aborts_private_file_before_link() {
        let root = test_directory("version-change");
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        let base = vec![79_u8; 1024];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let chunk = 128;
        let first_pass_reads = base.len().div_ceil(chunk);
        let mut input = source(base.clone());
        input.mutate_after_read = Some(first_pass_reads + 2);
        let mut backend =
            PersistentUnixStagingBackend::new(&staging, &destination, [4_u8; 32]);
        assert!(stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), chunk),
            PersistentSourceCopyOptions::default(),
        )
        .is_err());
        assert!(!destination.exists());
        assert!(fs::read_dir(&staging)
            .expect("staging")
            .next()
            .is_none());
        fs::remove_dir_all(root).expect("cleanup");
    }
}
