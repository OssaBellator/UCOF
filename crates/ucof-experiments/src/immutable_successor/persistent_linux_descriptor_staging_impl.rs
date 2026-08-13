use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const LINUX_O_DIRECTORY: i32 = 0o200000;
const LINUX_O_NOFOLLOW: i32 = 0o400000;
const LINUX_O_CLOEXEC: i32 = 0o2000000;

fn linux_effective_uid() -> Result<u32, &'static str> {
    let status = fs::read_to_string("/proc/self/status").map_err(|_| "effective uid")?;
    let line = status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .ok_or("effective uid")?;
    line.split_whitespace()
        .nth(2)
        .ok_or("effective uid")?
        .parse()
        .map_err(|_| "effective uid")
}

fn linux_single_component(name: &OsStr) -> Result<OsString, &'static str> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes == b"."
        || bytes == b".."
        || bytes.contains(&b'/')
        || bytes.contains(&0)
    {
        return Err("descriptor-relative filename");
    }
    Ok(name.to_os_string())
}

fn linux_destination_parts(destination: &Path) -> Result<(PathBuf, OsString), &'static str> {
    let name = destination.file_name().ok_or("destination filename")?;
    let name = linux_single_component(name)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    Ok((parent, name))
}

fn linux_procfd_directory(directory: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
}

fn linux_verify_procfd_directory(directory: &File) -> Result<(), &'static str> {
    let descriptor_metadata = directory
        .metadata()
        .map_err(|_| "descriptor directory metadata")?;
    let procfd_metadata = fs::metadata(linux_procfd_directory(directory))
        .map_err(|_| "procfd directory metadata")?;
    if descriptor_metadata.dev() != procfd_metadata.dev()
        || descriptor_metadata.ino() != procfd_metadata.ino()
    {
        return Err("procfd directory identity");
    }
    Ok(())
}

fn linux_procfd_child(directory: &File, name: &OsStr) -> Result<PathBuf, &'static str> {
    let name = linux_single_component(name)?;
    linux_verify_procfd_directory(directory)?;
    Ok(linux_procfd_directory(directory).join(name))
}

fn linux_open_directory(path: &Path, private: bool) -> Result<File, &'static str> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(LINUX_O_DIRECTORY | LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
        .map_err(|_| "descriptor directory open")?;
    let metadata = file
        .metadata()
        .map_err(|_| "descriptor directory metadata")?;
    if !metadata.file_type().is_dir() {
        return Err("descriptor directory");
    }
    if private {
        let effective_uid = linux_effective_uid()?;
        if metadata.uid() != effective_uid || metadata.permissions().mode() & 0o077 != 0 {
            return Err("private staging directory");
        }
    }
    linux_verify_procfd_directory(&file)?;
    Ok(file)
}

fn linux_open_relative_readonly(
    directory: &File,
    name: &OsStr,
) -> Result<Option<File>, &'static str> {
    let path = linux_procfd_child(directory, name)?;
    match OpenOptions::new()
        .read(true)
        .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
        .open(path)
    {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("descriptor-relative file open"),
    }
}

fn linux_same_file(left: &File, right: &File) -> Result<bool, &'static str> {
    let left = left.metadata().map_err(|_| "file identity metadata")?;
    let right = right.metadata().map_err(|_| "file identity metadata")?;
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

/// Linux production-candidate staging backend that pins the staging and destination directories.
///
/// The directory paths are resolved only by `begin_private`. Later child creation, lookup,
/// hard-link publication, cleanup, and directory synchronization route through `/proc/self/fd/N`,
/// where `N` is the already-open directory descriptor. This prevents a later rename/replacement of
/// the original directory pathname from redirecting the operation.
///
/// The staged name is reopened and inode-compared with the original staged file immediately before
/// publication and cleanup. This detects an already-observed same-user name replacement, but the
/// check plus `hard_link`/`remove_file` is not an atomic link-by-handle primitive. A hostile process
/// with sufficient access to replace names inside the private staging directory could still race
/// the final check. The backend therefore advances descriptor-pinned directory safety without
/// claiming to close that stronger same-UID adversary boundary.
///
/// This backend requires a usable Linux procfs and does not encrypt staged bytes.
pub struct PersistentLinuxDescriptorStagingBackend {
    staging_directory_path: PathBuf,
    destination_parent_path: PathBuf,
    destination_name: OsString,
    staged_name: OsString,
    staging_directory: Option<File>,
    destination_directory: Option<File>,
    staged_file: Option<File>,
    expected_length: Option<u64>,
    effective_uid: Option<u32>,
}

impl PersistentLinuxDescriptorStagingBackend {
    pub fn new(
        staging_directory: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        ownership_token: [u8; 32],
    ) -> Result<Self, &'static str> {
        let staging_directory_path = staging_directory.into();
        let destination = destination.into();
        let (destination_parent_path, destination_name) = linux_destination_parts(&destination)?;
        let staged_name = linux_single_component(OsStr::new(&persistent_unix_staged_name(
            &ownership_token,
        )))?;
        Ok(Self {
            staging_directory_path,
            destination_parent_path,
            destination_name,
            staged_name,
            staging_directory: None,
            destination_directory: None,
            staged_file: None,
            expected_length: None,
            effective_uid: None,
        })
    }

    fn staging_directory(&self) -> Result<&File, &'static str> {
        self.staging_directory
            .as_ref()
            .ok_or("staging directory descriptor")
    }

    fn destination_directory(&self) -> Result<&File, &'static str> {
        self.destination_directory
            .as_ref()
            .ok_or("destination directory descriptor")
    }

    fn staged_file_mut(&mut self) -> Result<&mut File, &'static str> {
        self.staged_file.as_mut().ok_or("staged file")
    }

    fn clear_private_state(&mut self) {
        self.staged_file = None;
        self.staging_directory = None;
        self.destination_directory = None;
        self.expected_length = None;
        self.effective_uid = None;
    }

    fn verify_staged_name_identity(&self) -> Result<bool, &'static str> {
        let staged = self.staged_file.as_ref().ok_or("staged file")?;
        let named = linux_open_relative_readonly(self.staging_directory()?, &self.staged_name)?;
        match named {
            Some(named) => linux_same_file(staged, &named),
            None => Ok(false),
        }
    }

    fn sync_staging_directory(&self) -> Result<(), &'static str> {
        linux_verify_procfd_directory(self.staging_directory()?)?;
        self.staging_directory()?
            .sync_all()
            .map_err(|_| "staging directory synchronization")
    }

    fn unlink_staged_name(&self, missing_is_success: bool) -> Result<(), &'static str> {
        let staging = self.staging_directory()?;
        let named = linux_open_relative_readonly(staging, &self.staged_name)?;
        let Some(named) = named else {
            return if missing_is_success {
                Ok(())
            } else {
                Err("staged name missing")
            };
        };
        if !linux_same_file(self.staged_file.as_ref().ok_or("staged file")?, &named)? {
            return Err("staged name identity");
        }
        drop(named);
        let path = linux_procfd_child(staging, &self.staged_name)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if missing_is_success && error.kind() == std::io::ErrorKind::NotFound => {
                Ok(())
            }
            Err(_) => Err("descriptor-relative private unlink"),
        }
    }
}

impl Write for PersistentLinuxDescriptorStagingBackend {
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

impl PersistentStagingBackend for PersistentLinuxDescriptorStagingBackend {
    fn begin_private(&mut self, expected_length: u64) -> Result<(), &'static str> {
        if self.staged_file.is_some()
            || self.staging_directory.is_some()
            || self.destination_directory.is_some()
        {
            return Err("private staging already active");
        }
        let effective_uid = linux_effective_uid()?;
        let staging = linux_open_directory(&self.staging_directory_path, true)?;
        let destination = linux_open_directory(&self.destination_parent_path, false)?;
        let staging_metadata = staging
            .metadata()
            .map_err(|_| "staging directory metadata")?;
        let destination_metadata = destination
            .metadata()
            .map_err(|_| "destination directory metadata")?;
        if staging_metadata.uid() != effective_uid {
            return Err("private staging owner");
        }
        if staging_metadata.dev() != destination_metadata.dev() {
            return Err("staging and destination filesystems differ");
        }

        let staged_path = linux_procfd_child(&staging, &self.staged_name)?;
        let staged = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(LINUX_O_NOFOLLOW | LINUX_O_CLOEXEC)
            .open(&staged_path)
            .map_err(|_| "exclusive descriptor-relative staged file creation")?;
        let metadata = staged.metadata().map_err(|_| "staged file metadata")?;
        if !metadata.file_type().is_file()
            || metadata.nlink() != 1
            || metadata.uid() != effective_uid
            || metadata.permissions().mode() & 0o077 != 0
        {
            let _ = fs::remove_file(staged_path);
            return Err("staged file invariants");
        }

        self.effective_uid = Some(effective_uid);
        self.expected_length = Some(expected_length);
        self.staging_directory = Some(staging);
        self.destination_directory = Some(destination);
        self.staged_file = Some(staged);
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
        let effective_uid = self.effective_uid.ok_or("effective user")?;
        let staged = self.staged_file_mut()?;
        staged.flush().map_err(|_| "staged file flush")?;
        let metadata = staged.metadata().map_err(|_| "staged file metadata")?;
        if !metadata.file_type().is_file()
            || metadata.nlink() != 1
            || metadata.uid() != effective_uid
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
        if !self.verify_staged_name_identity()? {
            return Err("staged name identity");
        }
        let staging = self.staging_directory()?;
        let destination = self.destination_directory()?;
        let staged_path = linux_procfd_child(staging, &self.staged_name)?;
        let destination_path = linux_procfd_child(destination, &self.destination_name)?;
        match fs::hard_link(staged_path, &destination_path) {
            Ok(()) => Ok(PersistentPublicationLinkOutcome::Linked),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Ok(PersistentPublicationLinkOutcome::DestinationExists)
            }
            Err(_) => {
                if let Some(destination_file) =
                    linux_open_relative_readonly(destination, &self.destination_name)?
                {
                    if linux_same_file(
                        self.staged_file.as_ref().ok_or("staged file")?,
                        &destination_file,
                    )? {
                        return Ok(PersistentPublicationLinkOutcome::Indeterminate);
                    }
                }
                Err("descriptor-relative no-overwrite publication link")
            }
        }
    }

    fn sync_parent(&mut self) -> Result<(), &'static str> {
        linux_verify_procfd_directory(self.destination_directory()?)?;
        self.destination_directory()?
            .sync_all()
            .map_err(|_| "destination directory synchronization")
    }

    fn retire_private(&mut self) -> Result<(), &'static str> {
        self.unlink_staged_name(false)?;
        self.sync_staging_directory()?;
        self.clear_private_state();
        Ok(())
    }

    fn abort_private(&mut self) -> Result<(), &'static str> {
        if self.staging_directory.is_some() && self.staged_file.is_some() {
            self.unlink_staged_name(true)?;
            self.sync_staging_directory()?;
        }
        self.clear_private_state();
        Ok(())
    }
}

#[cfg(test)]
mod persistent_linux_descriptor_staging_tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    struct VersionedBytes {
        bytes: Vec<u8>,
        version: PersistentSourceVersion,
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
            "ucof-linux-descriptor-stage-{label}-{}-{id}",
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
            version: PersistentSourceVersion([23; 32]),
        }
    }

    fn limits(length: usize) -> ImmutableSourceLimits {
        ImmutableSourceLimits {
            format: ImmutableLimits {
                max_file_bytes: 1024 * 1024,
                max_output_bytes: 1024 * 1024,
                max_allocation_bytes: 1024 * 1024,
                ..ImmutableLimits::default()
            },
            max_read_request_bytes: 127,
            max_total_bytes_read: u64::try_from(length.saturating_mul(4)).expect("budget"),
            max_read_operations: 1_000_000,
            ..ImmutableSourceLimits::default()
        }
    }

    #[test]
    fn publishes_without_overwrite_through_pinned_directories() {
        let root = test_directory("success");
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        let base = vec![81_u8; 4096];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut backend = PersistentLinuxDescriptorStagingBackend::new(
            &staging,
            &destination,
            [1_u8; 32],
        )
        .expect("backend");
        let report = stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len()),
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

        fs::write(&destination, b"old").expect("replace destination content for second run");
        let second = vec![91_u8; 1024];
        let second_identity = PersistentSourceIdentity::from_bytes(&second).expect("identity");
        let mut second_input = source(second.clone());
        let mut second_backend = PersistentLinuxDescriptorStagingBackend::new(
            &staging,
            &destination,
            [2_u8; 32],
        )
        .expect("backend");
        let second_report = stage_and_publish_versioned_source_with_tail(
            &mut second_input,
            &mut second_backend,
            second_identity,
            b"new",
            limits(second.len()),
            PersistentSourceCopyOptions::default(),
        )
        .expect("destination exists");
        assert_eq!(
            second_report.outcome,
            PersistentStagedPublicationOutcome::NotPublishedDestinationExists
        );
        assert_eq!(fs::read(&destination).expect("destination"), b"old");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn path_replacement_after_begin_cannot_redirect_publication() {
        let root = test_directory("path-swap");
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        let original_staging = root.join("staging-original");
        let original_destination = root.join("destination-original");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        let payload = b"descriptor-pinned-payload";
        let digest = <[u8; 32]>::from(Sha256::digest(payload));
        let mut backend = PersistentLinuxDescriptorStagingBackend::new(
            &staging,
            &destination,
            [3_u8; 32],
        )
        .expect("backend");
        backend
            .begin_private(payload.len() as u64)
            .expect("begin private");
        backend.write_all(payload).expect("write");

        fs::rename(&staging, &original_staging).expect("move staging directory");
        fs::rename(&destination_directory, &original_destination)
            .expect("move destination directory");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("replacement destination");

        backend
            .validate_private(payload.len() as u64, digest)
            .expect("validate");
        backend.sync_private().expect("sync private");
        assert_eq!(
            backend.publish_no_replace().expect("publish"),
            PersistentPublicationLinkOutcome::Linked
        );
        backend.sync_parent().expect("sync destination parent");
        backend.retire_private().expect("retire private");

        assert_eq!(
            fs::read(original_destination.join("archive.ucof")).expect("pinned destination"),
            payload
        );
        assert!(!destination.exists());
        assert!(fs::read_dir(&staging)
            .expect("replacement staging")
            .next()
            .is_none());
        assert!(fs::read_dir(&original_staging)
            .expect("original staging")
            .next()
            .is_none());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn observed_staged_name_replacement_is_rejected_before_link_or_cleanup() {
        let root = test_directory("stage-swap");
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        let token = [4_u8; 32];
        let payload = b"original-staged-bytes";
        let digest = <[u8; 32]>::from(Sha256::digest(payload));
        let mut backend = PersistentLinuxDescriptorStagingBackend::new(
            &staging,
            &destination,
            token,
        )
        .expect("backend");
        backend
            .begin_private(payload.len() as u64)
            .expect("begin private");
        backend.write_all(payload).expect("write");
        backend
            .validate_private(payload.len() as u64, digest)
            .expect("validate original handle");
        backend.sync_private().expect("sync private");

        let staged_path = staging.join(persistent_unix_staged_name(&token));
        fs::remove_file(&staged_path).expect("remove private name");
        fs::write(&staged_path, b"attacker replacement").expect("replace private name");
        assert_eq!(backend.publish_no_replace(), Err("staged name identity"));
        assert!(!destination.exists());
        assert_eq!(
            backend.abort_private(),
            Err("staged name identity"),
            "cleanup must not delete a replacement file"
        );
        assert_eq!(
            fs::read(&staged_path).expect("replacement retained"),
            b"attacker replacement"
        );
        drop(backend);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn symlink_staging_directory_is_rejected_before_private_creation() {
        let root = test_directory("symlink");
        let real_staging = root.join("real-staging");
        let staging_link = root.join("staging-link");
        let destination_directory = root.join("destination");
        private_directory(&real_staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        symlink(&real_staging, &staging_link).expect("staging symlink");
        let destination = destination_directory.join("archive.ucof");
        let mut backend = PersistentLinuxDescriptorStagingBackend::new(
            &staging_link,
            &destination,
            [5_u8; 32],
        )
        .expect("backend");
        assert!(backend.begin_private(10).is_err());
        assert!(fs::read_dir(&real_staging)
            .expect("real staging")
            .next()
            .is_none());
        fs::remove_dir_all(root).expect("cleanup");
    }
}
