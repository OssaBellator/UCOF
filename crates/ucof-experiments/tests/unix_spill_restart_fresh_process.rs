#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use ucof_experiments::{
    inspect_unix_spill_after_restart, SpillRestartDisposition, SpillRestartJournalEvidence,
    SpillRestartJournalPhase, UnixSpillRestartExpectedArtifact,
};

const BYTES: &[u8] = b"fresh-process-restart-bytes";
const TOKEN: [u8; 32] = [13; 32];
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

fn root() -> PathBuf {
    let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ucof-spill-fresh-restart-{}-{id}",
        std::process::id()
    ))
}

fn staged_name() -> String {
    let mut name = String::from("ucof-fault-");
    for byte in TOKEN {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02x}").expect("formatting");
    }
    name.push_str(".tmp");
    name
}

fn phase(value: &str) -> SpillRestartJournalPhase {
    match value {
        "staged" => SpillRestartJournalPhase::StagedFileSynced,
        "linked" => SpillRestartJournalPhase::DestinationLinkCreated,
        "synced" => SpillRestartJournalPhase::DestinationDirectorySynced,
        "retired" => SpillRestartJournalPhase::PrivateNameRetired,
        _ => panic!("unknown phase"),
    }
}

fn expected_disposition(value: &str) -> SpillRestartDisposition {
    match value {
        "retry" => SpillRestartDisposition::RetainOwnedStageForRetry,
        "indeterminate" => SpillRestartDisposition::PublicationIndeterminate,
        "cleanup" => SpillRestartDisposition::PublishedAndDurableCleanupStage,
        "durable" => SpillRestartDisposition::PublishedAndDurable,
        _ => panic!("unknown disposition"),
    }
}

fn run_child(staged: &Path, destination: &Path, phase: &str, expected: &str) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--exact")
        .arg("restart_child_process")
        .arg("--nocapture")
        .env("UCOF_RESTART_CHILD", "1")
        .env("UCOF_RESTART_STAGED", staged)
        .env("UCOF_RESTART_DESTINATION", destination)
        .env("UCOF_RESTART_PHASE", phase)
        .env("UCOF_RESTART_EXPECTED", expected)
        .status()
        .expect("spawn fresh process");
    assert!(status.success(), "fresh process classification failed");
}

#[test]
fn restart_child_process() {
    if std::env::var_os("UCOF_RESTART_CHILD").is_none() {
        return;
    }
    let staged = PathBuf::from(std::env::var_os("UCOF_RESTART_STAGED").expect("staged path"));
    let destination = PathBuf::from(
        std::env::var_os("UCOF_RESTART_DESTINATION").expect("destination path"),
    );
    let journal = SpillRestartJournalEvidence {
        phase: phase(&std::env::var("UCOF_RESTART_PHASE").expect("phase")),
        authenticated: true,
        ownership_matches: true,
    };
    let expected = UnixSpillRestartExpectedArtifact {
        ownership_token: TOKEN,
        length: u64::try_from(BYTES.len()).expect("length"),
        sha256: Sha256::digest(BYTES).into(),
    };
    let inspection = inspect_unix_spill_after_restart(
        &staged,
        &destination,
        Some(expected),
        Some(journal),
    )
    .expect("fresh process inspection");
    assert_eq!(
        inspection.disposition,
        expected_disposition(&std::env::var("UCOF_RESTART_EXPECTED").expect("expected"))
    );
}

#[test]
fn fresh_process_distinguishes_retry_indeterminate_and_durable_cleanup() {
    let root = root();
    fs::create_dir_all(&root).expect("root");
    let staged = root.join(staged_name());
    let destination = root.join("archive.ucof");
    fs::write(&staged, BYTES).expect("staged bytes");

    run_child(&staged, &destination, "staged", "retry");
    fs::hard_link(&staged, &destination).expect("destination link");
    run_child(&staged, &destination, "linked", "indeterminate");
    run_child(&staged, &destination, "synced", "cleanup");
    fs::remove_file(&staged).expect("retire private name");
    run_child(&staged, &destination, "retired", "durable");

    fs::remove_dir_all(root).expect("cleanup");
}
