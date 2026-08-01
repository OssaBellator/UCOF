#![cfg(unix)]

use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{
    classify_spill_restart, SpillRestartDisposition, SpillRestartFacts,
    SpillRestartJournalEvidence, SpillRestartOwnership, SpillRestartValidation,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnixSpillRestartExpectedArtifact {
    pub ownership_token: [u8; 32],
    pub length: u64,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnixSpillRestartInspection {
    pub staged_path: PathBuf,
    pub destination: PathBuf,
    pub facts: SpillRestartFacts,
    pub disposition: SpillRestartDisposition,
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

fn exists_without_symlink(path: &Path) -> io::Result<(bool, bool)> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok((true, metadata.file_type().is_symlink())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((false, false)),
        Err(error) => Err(error),
    }
}

fn validate_artifact(
    path: &Path,
    expected: UnixSpillRestartExpectedArtifact,
) -> io::Result<SpillRestartValidation> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_file()
        || metadata.file_type().is_socket()
        || metadata.len() != expected.length
    {
        return Ok(SpillRestartValidation::Invalid);
    }
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual: [u8; 32] = hasher.finalize().into();
    Ok(if actual == expected.sha256 {
        SpillRestartValidation::Valid
    } else {
        SpillRestartValidation::Invalid
    })
}

/// Inspects observable Unix spill state after a fresh process starts and applies the restart model.
///
/// `expected` must come from separately authenticated, ownership-bound durable metadata. Without it,
/// existing private ownership and artifact bytes remain unverifiable. The inspector performs no
/// deletion, linking, synchronization, or publication. A symlink is always invalid and is never
/// followed. A destination name without a qualifying journal remains indeterminate even when its
/// bytes match the expected digest.
pub fn inspect_unix_spill_after_restart(
    staged_path: &Path,
    destination: &Path,
    expected: Option<UnixSpillRestartExpectedArtifact>,
    journal: Option<SpillRestartJournalEvidence>,
) -> io::Result<UnixSpillRestartInspection> {
    let (staged_name_exists, staged_symlink) = exists_without_symlink(staged_path)?;
    let (destination_exists, destination_symlink) = exists_without_symlink(destination)?;

    let staged_ownership = if !staged_name_exists {
        SpillRestartOwnership::Unverifiable
    } else if let Some(expected) = expected {
        let matches = staged_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == staged_name(&expected.ownership_token));
        if matches {
            SpillRestartOwnership::Owned
        } else {
            SpillRestartOwnership::Foreign
        }
    } else {
        SpillRestartOwnership::Unverifiable
    };

    let staged_validation = if !staged_name_exists {
        SpillRestartValidation::Unknown
    } else if staged_symlink {
        SpillRestartValidation::Invalid
    } else if staged_ownership == SpillRestartOwnership::Owned {
        match expected {
            Some(expected) => validate_artifact(staged_path, expected)?,
            None => SpillRestartValidation::Unknown,
        }
    } else {
        SpillRestartValidation::Unknown
    };

    let destination_validation = if !destination_exists {
        SpillRestartValidation::Unknown
    } else if destination_symlink {
        SpillRestartValidation::Invalid
    } else {
        match expected {
            Some(expected) => validate_artifact(destination, expected)?,
            None => SpillRestartValidation::Unknown,
        }
    };

    let facts = SpillRestartFacts {
        staged_name_exists,
        destination_exists,
        staged_ownership,
        staged_validation,
        destination_validation,
        journal,
    };
    Ok(UnixSpillRestartInspection {
        staged_path: staged_path.to_path_buf(),
        destination: destination.to_path_buf(),
        disposition: classify_spill_restart(facts),
        facts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SpillRestartJournalPhase, SpillRestartJournalEvidence};
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn root(label: &str) -> PathBuf {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ucof-spill-restart-{label}-{}-{id}",
            std::process::id()
        ))
    }

    fn expected(bytes: &[u8], token: [u8; 32]) -> UnixSpillRestartExpectedArtifact {
        UnixSpillRestartExpectedArtifact {
            ownership_token: token,
            length: u64::try_from(bytes.len()).expect("length"),
            sha256: Sha256::digest(bytes).into(),
        }
    }

    fn journal(phase: SpillRestartJournalPhase) -> SpillRestartJournalEvidence {
        SpillRestartJournalEvidence {
            phase,
            authenticated: true,
            ownership_matches: true,
        }
    }

    #[test]
    fn matching_private_stage_is_retryable_but_destination_without_sync_is_indeterminate() {
        let root = root("retry");
        fs::create_dir_all(&root).expect("root");
        let bytes = b"restart bytes";
        let token = [7; 32];
        let staged = root.join(staged_name(&token));
        let destination = root.join("archive.ucof");
        fs::write(&staged, bytes).expect("stage");
        let expected = expected(bytes, token);
        let inspection = inspect_unix_spill_after_restart(
            &staged,
            &destination,
            Some(expected),
            Some(journal(SpillRestartJournalPhase::StagedFileSynced)),
        )
        .expect("inspection");
        assert_eq!(
            inspection.disposition,
            SpillRestartDisposition::RetainOwnedStageForRetry
        );

        fs::hard_link(&staged, &destination).expect("link");
        let inspection = inspect_unix_spill_after_restart(
            &staged,
            &destination,
            Some(expected),
            Some(journal(SpillRestartJournalPhase::DestinationLinkCreated)),
        )
        .expect("inspection");
        assert_eq!(
            inspection.disposition,
            SpillRestartDisposition::PublicationIndeterminate
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn synced_destination_allows_only_matching_owned_cleanup() {
        let root = root("durable");
        fs::create_dir_all(&root).expect("root");
        let bytes = b"durable bytes";
        let token = [9; 32];
        let staged = root.join(staged_name(&token));
        let destination = root.join("archive.ucof");
        fs::write(&staged, bytes).expect("stage");
        fs::hard_link(&staged, &destination).expect("link");
        let inspection = inspect_unix_spill_after_restart(
            &staged,
            &destination,
            Some(expected(bytes, token)),
            Some(journal(
                SpillRestartJournalPhase::DestinationDirectorySynced,
            )),
        )
        .expect("inspection");
        assert_eq!(
            inspection.disposition,
            SpillRestartDisposition::PublishedAndDurableCleanupStage
        );
        fs::remove_file(&staged).expect("retire stage");
        let inspection = inspect_unix_spill_after_restart(
            &staged,
            &destination,
            Some(expected(bytes, token)),
            Some(journal(SpillRestartJournalPhase::PrivateNameRetired)),
        )
        .expect("inspection");
        assert_eq!(
            inspection.disposition,
            SpillRestartDisposition::PublishedAndDurable
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn foreign_names_and_symlinks_are_never_owned_cleanup_candidates() {
        let root = root("foreign");
        fs::create_dir_all(&root).expect("root");
        let bytes = b"foreign bytes";
        let expected = expected(bytes, [11; 32]);
        let foreign = root.join("foreign.tmp");
        let destination = root.join("archive.ucof");
        fs::write(&foreign, bytes).expect("foreign stage");
        let inspection = inspect_unix_spill_after_restart(
            &foreign,
            &destination,
            Some(expected),
            None,
        )
        .expect("inspection");
        assert_eq!(
            inspection.disposition,
            SpillRestartDisposition::PreserveForeignState
        );

        fs::remove_file(&foreign).expect("remove foreign");
        fs::write(root.join("target"), bytes).expect("target");
        symlink(root.join("target"), &foreign).expect("symlink");
        let inspection = inspect_unix_spill_after_restart(
            &foreign,
            &destination,
            Some(expected),
            None,
        )
        .expect("inspection");
        assert_eq!(inspection.facts.staged_ownership, SpillRestartOwnership::Foreign);
        assert_eq!(
            inspection.disposition,
            SpillRestartDisposition::PreserveForeignState
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
