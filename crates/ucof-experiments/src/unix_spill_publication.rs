#![cfg(unix)]

use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::{
    NoOverwriteLinkResult, SpillConfidentialityPolicy, SpillPublicationError,
    SpillPublicationLimits, SpillPublicationOutcome, SpillPublicationSession,
    SpillPublicationStage,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnixSpillPublicationReport {
    pub outcome: SpillPublicationOutcome,
    pub stage: SpillPublicationStage,
    pub staged_path: PathBuf,
    pub destination: PathBuf,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnixSpillPublicationError {
    Policy {
        error: SpillPublicationError,
        outcome: SpillPublicationOutcome,
        stage: SpillPublicationStage,
    },
    Io {
        label: &'static str,
        outcome: SpillPublicationOutcome,
        stage: SpillPublicationStage,
    },
}

impl fmt::Display for UnixSpillPublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Policy { error, .. } => write!(formatter, "{error}"),
            Self::Io { label, .. } => {
                write!(formatter, "spill filesystem operation failed: {label}")
            }
        }
    }
}

impl Error for UnixSpillPublicationError {}

fn policy_error(
    session: &SpillPublicationSession,
    error: SpillPublicationError,
) -> UnixSpillPublicationError {
    UnixSpillPublicationError::Policy {
        error,
        outcome: session.outcome(),
        stage: session.stage(),
    }
}

fn io_error(session: &SpillPublicationSession, label: &'static str) -> UnixSpillPublicationError {
    UnixSpillPublicationError::Io {
        label,
        outcome: session.outcome(),
        stage: session.stage(),
    }
}

fn owned_staged_name(token: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut name = String::with_capacity(11 + token.len() * 2);
    name.push_str("ucof-spill-");
    for byte in token {
        name.push(char::from(HEX[usize::from(byte >> 4)]));
        name.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    name.push_str(".tmp");
    name
}

fn private_staging_directory(path: &Path) -> Result<fs::Metadata, &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "staging directory metadata")?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_dir()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("private staging directory");
    }
    Ok(metadata)
}

fn destination_directory(path: &Path) -> Result<(), &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "destination directory metadata")?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err("destination directory");
    }
    Ok(())
}

fn cleanup_not_published(
    session: &mut SpillPublicationSession,
    token: &[u8; 32],
    staged_path: &Path,
) {
    let removed = fs::remove_file(staged_path).is_ok();
    let _ = session.record_owned_cleanup(token, removed);
}

/// Publishes one complete byte buffer through a private Unix staging directory and a same-filesystem
/// no-overwrite hard link.
///
/// The destination parent is synchronized before durable success is reported. A failed directory
/// synchronization is reported as indeterminate, and a later staged-name cleanup failure cannot
/// downgrade durable publication. This research harness does not encrypt spill bytes and therefore
/// rejects [`SpillConfidentialityPolicy::EncryptedSpillRequired`] before creating a file.
pub fn publish_bytes_no_overwrite<F>(
    staging_directory: &Path,
    destination: &Path,
    bytes: &[u8],
    ownership_token: [u8; 32],
    confidentiality: SpillConfidentialityPolicy,
    limits: SpillPublicationLimits,
    validate: F,
) -> Result<UnixSpillPublicationReport, UnixSpillPublicationError>
where
    F: FnOnce(&Path) -> Result<(), &'static str>,
{
    let mut session = SpillPublicationSession::new(ownership_token, confidentiality, limits)
        .map_err(|error| UnixSpillPublicationError::Policy {
            error,
            outcome: SpillPublicationOutcome::NotPublished,
            stage: SpillPublicationStage::PrivateStaging,
        })?;
    if confidentiality == SpillConfidentialityPolicy::EncryptedSpillRequired {
        return Err(policy_error(
            &session,
            SpillPublicationError::NotPublished("encrypted spill required"),
        ));
    }

    let staging_metadata =
        private_staging_directory(staging_directory).map_err(|label| io_error(&session, label))?;
    let destination_parent = destination
        .parent()
        .ok_or_else(|| io_error(&session, "destination parent"))?;
    destination_directory(destination_parent).map_err(|label| io_error(&session, label))?;

    let staged_path = staging_directory.join(owned_staged_name(&ownership_token));
    let mut staged = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&staged_path)
        .map_err(|_| io_error(&session, "exclusive staged file creation"))?;
    let byte_count = u64::try_from(bytes.len()).map_err(|_| io_error(&session, "byte count"))?;
    session
        .record_staged_file(&ownership_token, byte_count)
        .map_err(|error| policy_error(&session, error))?;

    if staged.write_all(bytes).is_err() {
        cleanup_not_published(&mut session, &ownership_token, &staged_path);
        return Err(io_error(&session, "staged file write"));
    }
    let metadata = staged
        .metadata()
        .map_err(|_| io_error(&session, "staged file metadata"))?;
    if !metadata.file_type().is_file()
        || metadata.nlink() != 1
        || metadata.uid() != staging_metadata.uid()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() != byte_count
    {
        cleanup_not_published(&mut session, &ownership_token, &staged_path);
        return Err(io_error(&session, "staged file invariants"));
    }
    if let Err(label) = validate(&staged_path) {
        cleanup_not_published(&mut session, &ownership_token, &staged_path);
        return Err(policy_error(
            &session,
            SpillPublicationError::NotPublished(label),
        ));
    }
    session
        .record_complete_validation(&ownership_token)
        .map_err(|error| policy_error(&session, error))?;

    let synchronized = staged.sync_all().is_ok();
    session
        .record_staged_file_sync(&ownership_token, synchronized)
        .map_err(|error| policy_error(&session, error))?;
    drop(staged);

    let link_result = match fs::hard_link(&staged_path, destination) {
        Ok(()) => NoOverwriteLinkResult::Created,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            NoOverwriteLinkResult::DestinationExists
        }
        Err(_) => match (fs::metadata(&staged_path), fs::metadata(destination)) {
            (Ok(staged_metadata), Ok(destination_metadata))
                if staged_metadata.dev() == destination_metadata.dev()
                    && staged_metadata.ino() == destination_metadata.ino() =>
            {
                NoOverwriteLinkResult::Indeterminate
            }
            _ => NoOverwriteLinkResult::NotCreated,
        },
    };
    if let Err(error) = session.record_no_overwrite_link(&ownership_token, link_result) {
        if link_result != NoOverwriteLinkResult::Indeterminate {
            cleanup_not_published(&mut session, &ownership_token, &staged_path);
        }
        return Err(policy_error(&session, error));
    }

    let destination_synchronized = File::open(destination_parent)
        .and_then(|directory| directory.sync_all())
        .is_ok();
    session
        .record_destination_directory_sync(&ownership_token, destination_synchronized)
        .map_err(|error| policy_error(&session, error))?;

    let retired = fs::remove_file(&staged_path).is_ok();
    session
        .record_private_name_retirement(&ownership_token, retired)
        .map_err(|error| policy_error(&session, error))?;
    if File::open(staging_directory)
        .and_then(|directory| directory.sync_all())
        .is_err()
    {
        return Err(UnixSpillPublicationError::Io {
            label: "staging directory synchronization",
            outcome: session.outcome(),
            stage: session.stage(),
        });
    }

    Ok(UnixSpillPublicationReport {
        outcome: session.outcome(),
        stage: session.stage(),
        staged_path,
        destination: destination.to_path_buf(),
        bytes: byte_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn test_directory(label: &str) -> PathBuf {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ucof-spill-{label}-{}-{id}", std::process::id()))
    }

    fn private_directory(path: &Path) {
        fs::create_dir_all(path).expect("create private directory");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("private permissions");
    }

    fn token(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    #[test]
    fn publishes_without_overwriting_and_retires_private_name() {
        let root = test_directory("success");
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        let report = publish_bytes_no_overwrite(
            &staging,
            &destination,
            b"verified bytes",
            token(1),
            SpillConfidentialityPolicy::PlaintextPermitted,
            SpillPublicationLimits::default(),
            |path| {
                if fs::read(path).map_err(|_| "validation read")? == b"verified bytes" {
                    Ok(())
                } else {
                    Err("validation mismatch")
                }
            },
        )
        .expect("durable publication");
        assert_eq!(report.outcome, SpillPublicationOutcome::PublishedAndDurable);
        assert_eq!(report.stage, SpillPublicationStage::PrivateNameRetired);
        assert_eq!(
            fs::read(&destination).expect("destination bytes"),
            b"verified bytes"
        );
        assert!(!report.staged_path.exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn existing_destination_is_untouched() {
        let root = test_directory("exists");
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        fs::write(&destination, b"old").expect("existing destination");
        let error = publish_bytes_no_overwrite(
            &staging,
            &destination,
            b"new",
            token(2),
            SpillConfidentialityPolicy::PlaintextPermitted,
            SpillPublicationLimits::default(),
            |_| Ok(()),
        )
        .expect_err("must not overwrite");
        assert!(matches!(
            error,
            UnixSpillPublicationError::Policy {
                error: SpillPublicationError::DestinationExists,
                outcome: SpillPublicationOutcome::NotPublished,
                ..
            }
        ));
        assert_eq!(fs::read(&destination).expect("old destination"), b"old");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn encryption_requirement_and_validation_failure_leave_no_output() {
        let root = test_directory("reject");
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        private_directory(&staging);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let encrypted_destination = destination_directory.join("encrypted.ucof");
        assert!(publish_bytes_no_overwrite(
            &staging,
            &encrypted_destination,
            b"secret",
            token(3),
            SpillConfidentialityPolicy::EncryptedSpillRequired,
            SpillPublicationLimits::default(),
            |_| Ok(()),
        )
        .is_err());
        assert!(!encrypted_destination.exists());

        let invalid_destination = destination_directory.join("invalid.ucof");
        assert!(publish_bytes_no_overwrite(
            &staging,
            &invalid_destination,
            b"invalid",
            token(4),
            SpillConfidentialityPolicy::PlaintextPermitted,
            SpillPublicationLimits::default(),
            |_| Err("validation rejected"),
        )
        .is_err());
        assert!(!invalid_destination.exists());
        assert!(fs::read_dir(&staging)
            .expect("staging directory")
            .next()
            .is_none());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn symlink_staging_directory_is_rejected_before_creation() {
        let root = test_directory("symlink");
        let real = root.join("real");
        let staging = root.join("staging-link");
        let destination_directory = root.join("destination");
        private_directory(&real);
        fs::create_dir_all(&destination_directory).expect("destination directory");
        symlink(&real, &staging).expect("staging symlink");
        let destination = destination_directory.join("archive.ucof");
        assert!(publish_bytes_no_overwrite(
            &staging,
            &destination,
            b"bytes",
            token(5),
            SpillConfidentialityPolicy::PlaintextPermitted,
            SpillPublicationLimits::default(),
            |_| Ok(()),
        )
        .is_err());
        assert!(!destination.exists());
        fs::remove_dir_all(root).expect("cleanup");
    }
}
