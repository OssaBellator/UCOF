#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PersistentUnixDirectoryIdentity {
    device: u64,
    inode: u64,
}

impl PersistentUnixDirectoryIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

fn persistent_unix_check_private_directory_identity(
    path: &Path,
    expected: PersistentUnixDirectoryIdentity,
) -> Result<(), &'static str> {
    let metadata = persistent_unix_private_directory(path)?;
    if PersistentUnixDirectoryIdentity::from_metadata(&metadata) != expected {
        return Err("staging directory changed");
    }
    Ok(())
}

fn persistent_unix_check_destination_directory_identity(
    path: &Path,
    expected: PersistentUnixDirectoryIdentity,
) -> Result<(), &'static str> {
    let metadata = persistent_unix_destination_directory(path)?;
    if PersistentUnixDirectoryIdentity::from_metadata(&metadata) != expected {
        return Err("destination directory changed");
    }
    Ok(())
}

/// Path-identity-pinning wrapper for [`PersistentUnixStagingBackend`].
///
/// After private staging begins, the wrapper records the device and inode of the staging directory
/// and destination parent. Every later path-dependent operation fails closed if either observed
/// directory identity has changed. This narrows path-replacement exposure but remains path-based: it
/// is not a substitute for descriptor-relative `openat`/`linkat` resolution and does not close races
/// between an identity check and the following filesystem operation.
pub struct PersistentPinnedUnixStagingBackend {
    inner: PersistentUnixStagingBackend,
    staging_directory: PathBuf,
    destination_parent: PathBuf,
    staging_identity: Option<PersistentUnixDirectoryIdentity>,
    destination_identity: Option<PersistentUnixDirectoryIdentity>,
}

impl PersistentPinnedUnixStagingBackend {
    pub fn new(
        staging_directory: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        ownership_token: [u8; 32],
    ) -> Result<Self, &'static str> {
        let staging_directory = staging_directory.into();
        let destination = destination.into();
        let destination_parent = destination
            .parent()
            .ok_or("destination parent")?
            .to_path_buf();
        Ok(Self {
            inner: PersistentUnixStagingBackend::new(
                staging_directory.clone(),
                destination,
                ownership_token,
            ),
            staging_directory,
            destination_parent,
            staging_identity: None,
            destination_identity: None,
        })
    }

    pub fn staged_path(&self) -> Option<&Path> {
        self.inner.staged_path()
    }

    pub fn destination(&self) -> &Path {
        self.inner.destination()
    }

    fn check_staging_identity(&self) -> Result<(), &'static str> {
        persistent_unix_check_private_directory_identity(
            &self.staging_directory,
            self.staging_identity.ok_or("staging directory identity")?,
        )
    }

    fn check_destination_identity(&self) -> Result<(), &'static str> {
        persistent_unix_check_destination_directory_identity(
            &self.destination_parent,
            self.destination_identity
                .ok_or("destination directory identity")?,
        )
    }

    fn clear_identities(&mut self) {
        self.staging_identity = None;
        self.destination_identity = None;
    }
}

impl Write for PersistentPinnedUnixStagingBackend {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl PersistentStagingBackend for PersistentPinnedUnixStagingBackend {
    fn begin_private(&mut self, expected_length: u64) -> Result<(), &'static str> {
        if self.staging_identity.is_some() || self.destination_identity.is_some() {
            return Err("directory identities already pinned");
        }
        self.inner.begin_private(expected_length)?;

        let result = (|| {
            let staging = persistent_unix_private_directory(&self.staging_directory)?;
            let destination =
                persistent_unix_destination_directory(&self.destination_parent)?;
            if staging.dev() != destination.dev() {
                return Err("staging and destination filesystems differ");
            }
            self.staging_identity = Some(PersistentUnixDirectoryIdentity::from_metadata(&staging));
            self.destination_identity =
                Some(PersistentUnixDirectoryIdentity::from_metadata(&destination));
            Ok(())
        })();
        if let Err(error) = result {
            let _ = self.inner.abort_private();
            self.clear_identities();
            return Err(error);
        }
        Ok(())
    }

    fn validate_private(
        &mut self,
        expected_length: u64,
        expected_sha256: [u8; 32],
    ) -> Result<(), &'static str> {
        self.check_staging_identity()?;
        self.inner
            .validate_private(expected_length, expected_sha256)
    }

    fn sync_private(&mut self) -> Result<(), &'static str> {
        self.check_staging_identity()?;
        self.inner.sync_private()
    }

    fn publish_no_replace(&mut self) -> Result<PersistentPublicationLinkOutcome, &'static str> {
        self.check_staging_identity()?;
        self.check_destination_identity()?;
        self.inner.publish_no_replace()
    }

    fn sync_parent(&mut self) -> Result<(), &'static str> {
        self.check_destination_identity()?;
        self.inner.sync_parent()
    }

    fn retire_private(&mut self) -> Result<(), &'static str> {
        self.check_staging_identity()?;
        self.check_destination_identity()?;
        self.inner.retire_private()?;
        self.clear_identities();
        Ok(())
    }

    fn abort_private(&mut self) -> Result<(), &'static str> {
        self.check_staging_identity()?;
        self.inner.abort_private()?;
        self.clear_identities();
        Ok(())
    }
}

#[cfg(test)]
mod persistent_unix_directory_pinning_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn test_root(label: &str) -> PathBuf {
        let nonce = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ucof-pinned-staging-{}-{label}-{nonce}",
            std::process::id()
        ))
    }

    fn private_directory(path: &Path) {
        fs::create_dir_all(path).expect("create directory");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("private permissions");
    }

    #[test]
    fn stable_directory_identities_delegate_abort() {
        let root = test_root("stable");
        let staging = root.join("staging");
        let destination_parent = root.join("destination");
        private_directory(&staging);
        private_directory(&destination_parent);
        let destination = destination_parent.join("artifact.ucof");
        let mut backend = PersistentPinnedUnixStagingBackend::new(
            &staging,
            &destination,
            [17; 32],
        )
        .expect("backend");

        backend.begin_private(0).expect("begin");
        assert!(backend.staged_path().is_some());
        backend.abort_private().expect("abort");
        assert!(backend.staged_path().is_none());
        assert!(!destination.exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn staging_directory_replacement_blocks_cleanup() {
        let root = test_root("staging-replaced");
        let staging = root.join("staging");
        let displaced = root.join("staging-original");
        let destination_parent = root.join("destination");
        private_directory(&staging);
        private_directory(&destination_parent);
        let destination = destination_parent.join("artifact.ucof");
        let mut backend = PersistentPinnedUnixStagingBackend::new(
            &staging,
            &destination,
            [18; 32],
        )
        .expect("backend");
        backend.begin_private(0).expect("begin");
        let staged_name = backend
            .staged_path()
            .and_then(Path::file_name)
            .expect("staged name")
            .to_owned();

        fs::rename(&staging, &displaced).expect("displace staging directory");
        private_directory(&staging);
        assert_eq!(
            backend.abort_private(),
            Err("staging directory changed")
        );
        assert!(displaced.join(staged_name).exists());
        assert!(fs::read_dir(&staging)
            .expect("replacement directory")
            .next()
            .is_none());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn destination_directory_replacement_blocks_publication() {
        let root = test_root("destination-replaced");
        let staging = root.join("staging");
        let destination_parent = root.join("destination");
        let displaced = root.join("destination-original");
        private_directory(&staging);
        private_directory(&destination_parent);
        let destination = destination_parent.join("artifact.ucof");
        let mut backend = PersistentPinnedUnixStagingBackend::new(
            &staging,
            &destination,
            [19; 32],
        )
        .expect("backend");
        backend.begin_private(0).expect("begin");

        fs::rename(&destination_parent, &displaced).expect("displace destination directory");
        private_directory(&destination_parent);
        assert_eq!(
            backend.publish_no_replace(),
            Err("destination directory changed")
        );
        assert!(!destination.exists());
        backend.abort_private().expect("abort original staging");
        fs::remove_dir_all(root).expect("cleanup");
    }
}
