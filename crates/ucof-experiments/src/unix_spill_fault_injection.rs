#![cfg(unix)]

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::{
    NoOverwriteLinkResult, SpillConfidentialityPolicy, SpillPublicationError,
    SpillPublicationLimits, SpillPublicationOutcome, SpillPublicationSession,
    SpillPublicationStage, UnixSpillPublicationError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnixSpillFaultPoint {
    BeforeDestinationLink,
    AfterDestinationLink,
    AfterDestinationDirectorySync,
    DuringPrivateNameRetirement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnixSpillFaultReport {
    pub fault: Option<UnixSpillFaultPoint>,
    pub outcome: SpillPublicationOutcome,
    pub stage: SpillPublicationStage,
    pub policy_error: Option<SpillPublicationError>,
    pub staged_path: PathBuf,
    pub destination: PathBuf,
    pub destination_exists: bool,
    pub staged_name_exists: bool,
}

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

fn io_error(
    session: &SpillPublicationSession,
    label: &'static str,
) -> UnixSpillPublicationError {
    UnixSpillPublicationError::Io {
        label,
        outcome: session.outcome(),
        stage: session.stage(),
    }
}

fn staged_name(token: &[u8; 32]) -> String {
    let mut name = String::from("ucof-fault-");
    for byte in token {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02x}").expect("string formatting cannot fail");
    }
    name.push_str(".tmp");
    name
}

fn report(
    session: &SpillPublicationSession,
    fault: Option<UnixSpillFaultPoint>,
    policy_error: Option<SpillPublicationError>,
    staged_path: PathBuf,
    destination: PathBuf,
) -> UnixSpillFaultReport {
    UnixSpillFaultReport {
        fault,
        outcome: session.outcome(),
        stage: session.stage(),
        policy_error,
        destination_exists: destination.exists(),
        staged_name_exists: staged_path.exists(),
        staged_path,
        destination,
    }
}

/// Executes one real Unix publication attempt with an optional deterministic injected failure.
///
/// The harness is deliberately narrower than [`crate::publish_bytes_no_overwrite`]: it accepts only
/// plaintext research bytes and assumes the caller created the destination directory. It exists to
/// prove that state-machine outcomes remain aligned with observable filesystem side effects at the
/// authority boundaries.
pub fn run_fault_injected_unix_publication(
    staging_directory: &Path,
    destination: &Path,
    bytes: &[u8],
    ownership_token: [u8; 32],
    fault: Option<UnixSpillFaultPoint>,
    limits: SpillPublicationLimits,
) -> Result<UnixSpillFaultReport, UnixSpillPublicationError> {
    let mut session = SpillPublicationSession::new(
        ownership_token,
        SpillConfidentialityPolicy::PlaintextPermitted,
        limits,
    )
    .map_err(|error| UnixSpillPublicationError::Policy {
        error,
        outcome: SpillPublicationOutcome::NotPublished,
        stage: SpillPublicationStage::PrivateStaging,
    })?;

    let staging_metadata = fs::symlink_metadata(staging_directory)
        .map_err(|_| io_error(&session, "staging directory metadata"))?;
    if staging_metadata.file_type().is_symlink()
        || !staging_metadata.file_type().is_dir()
        || staging_metadata.permissions().mode() & 0o077 != 0
    {
        return Err(io_error(&session, "private staging directory"));
    }
    let destination_parent = destination
        .parent()
        .ok_or_else(|| io_error(&session, "destination parent"))?;
    let destination_metadata = fs::symlink_metadata(destination_parent)
        .map_err(|_| io_error(&session, "destination directory metadata"))?;
    if destination_metadata.file_type().is_symlink()
        || !destination_metadata.file_type().is_dir()
    {
        return Err(io_error(&session, "destination directory"));
    }

    let staged_path = staging_directory.join(staged_name(&ownership_token));
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
    staged
        .write_all(bytes)
        .map_err(|_| io_error(&session, "staged file write"))?;
    let metadata = staged
        .metadata()
        .map_err(|_| io_error(&session, "staged file metadata"))?;
    if !metadata.file_type().is_file()
        || metadata.nlink() != 1
        || metadata.uid() != staging_metadata.uid()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() != byte_count
    {
        return Err(io_error(&session, "staged file invariants"));
    }
    session
        .record_complete_validation(&ownership_token)
        .map_err(|error| policy_error(&session, error))?;
    staged
        .sync_all()
        .map_err(|_| io_error(&session, "staged file synchronization"))?;
    session
        .record_staged_file_sync(&ownership_token, true)
        .map_err(|error| policy_error(&session, error))?;
    drop(staged);

    if fault == Some(UnixSpillFaultPoint::BeforeDestinationLink) {
        let removed = fs::remove_file(&staged_path).is_ok();
        let policy_failure = session
            .record_owned_cleanup(&ownership_token, removed)
            .err();
        return Ok(report(
            &session,
            fault,
            policy_failure,
            staged_path,
            destination.to_path_buf(),
        ));
    }

    fs::hard_link(&staged_path, destination)
        .map_err(|_| io_error(&session, "destination hard link"))?;
    session
        .record_no_overwrite_link(&ownership_token, NoOverwriteLinkResult::Created)
        .map_err(|error| policy_error(&session, error))?;
    if fault == Some(UnixSpillFaultPoint::AfterDestinationLink) {
        return Ok(report(
            &session,
            fault,
            None,
            staged_path,
            destination.to_path_buf(),
        ));
    }

    File::open(destination_parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| io_error(&session, "destination directory synchronization"))?;
    session
        .record_destination_directory_sync(&ownership_token, true)
        .map_err(|error| policy_error(&session, error))?;
    if fault == Some(UnixSpillFaultPoint::AfterDestinationDirectorySync) {
        return Ok(report(
            &session,
            fault,
            None,
            staged_path,
            destination.to_path_buf(),
        ));
    }

    if fault == Some(UnixSpillFaultPoint::DuringPrivateNameRetirement) {
        let policy_failure = session
            .record_private_name_retirement(&ownership_token, false)
            .err();
        return Ok(report(
            &session,
            fault,
            policy_failure,
            staged_path,
            destination.to_path_buf(),
        ));
    }

    let retired = fs::remove_file(&staged_path).is_ok();
    session
        .record_private_name_retirement(&ownership_token, retired)
        .map_err(|error| policy_error(&session, error))?;
    File::open(staging_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| io_error(&session, "staging directory synchronization"))?;
    Ok(report(
        &session,
        None,
        None,
        staged_path,
        destination.to_path_buf(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn root(label: &str) -> PathBuf {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ucof-spill-fault-{label}-{}-{id}",
            std::process::id()
        ))
    }

    fn layout(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = root(label);
        let staging = root.join("staging");
        let destination_directory = root.join("destination");
        fs::create_dir_all(&staging).expect("staging directory");
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))
            .expect("private staging permissions");
        fs::create_dir_all(&destination_directory).expect("destination directory");
        let destination = destination_directory.join("archive.ucof");
        (root, staging, destination)
    }

    #[test]
    fn before_link_failure_has_no_public_name() {
        let (root, staging, destination) = layout("before-link");
        let report = run_fault_injected_unix_publication(
            &staging,
            &destination,
            b"bytes",
            [1; 32],
            Some(UnixSpillFaultPoint::BeforeDestinationLink),
            SpillPublicationLimits::default(),
        )
        .expect("injected report");
        assert_eq!(report.outcome, SpillPublicationOutcome::NotPublished);
        assert!(!report.destination_exists);
        assert!(!report.staged_name_exists);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn after_link_failure_is_observably_indeterminate() {
        let (root, staging, destination) = layout("after-link");
        let report = run_fault_injected_unix_publication(
            &staging,
            &destination,
            b"bytes",
            [2; 32],
            Some(UnixSpillFaultPoint::AfterDestinationLink),
            SpillPublicationLimits::default(),
        )
        .expect("injected report");
        assert_eq!(
            report.outcome,
            SpillPublicationOutcome::PublicationIndeterminate
        );
        assert!(report.destination_exists);
        assert!(report.staged_name_exists);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn post_sync_failure_cannot_downgrade_durable_publication() {
        let (root, staging, destination) = layout("after-sync");
        let report = run_fault_injected_unix_publication(
            &staging,
            &destination,
            b"bytes",
            [3; 32],
            Some(UnixSpillFaultPoint::AfterDestinationDirectorySync),
            SpillPublicationLimits::default(),
        )
        .expect("injected report");
        assert_eq!(report.outcome, SpillPublicationOutcome::PublishedAndDurable);
        assert!(report.destination_exists);
        assert!(report.staged_name_exists);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn retirement_failure_preserves_durable_success_and_reports_cleanup_error() {
        let (root, staging, destination) = layout("retirement");
        let report = run_fault_injected_unix_publication(
            &staging,
            &destination,
            b"bytes",
            [4; 32],
            Some(UnixSpillFaultPoint::DuringPrivateNameRetirement),
            SpillPublicationLimits::default(),
        )
        .expect("injected report");
        assert_eq!(report.outcome, SpillPublicationOutcome::PublishedAndDurable);
        assert!(report.destination_exists);
        assert!(report.staged_name_exists);
        assert!(report.policy_error.is_some());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn no_fault_retires_the_private_name_after_durable_publication() {
        let (root, staging, destination) = layout("success");
        let report = run_fault_injected_unix_publication(
            &staging,
            &destination,
            b"bytes",
            [5; 32],
            None,
            SpillPublicationLimits::default(),
        )
        .expect("publication");
        assert_eq!(report.outcome, SpillPublicationOutcome::PublishedAndDurable);
        assert!(report.destination_exists);
        assert!(!report.staged_name_exists);
        assert_eq!(report.stage, SpillPublicationStage::PrivateNameRetired);
        fs::remove_dir_all(root).expect("cleanup");
    }
}
