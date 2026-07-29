use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn demo_inspect_verify_diagnose_and_salvage_have_distinct_semantics() {
    let path = temporary_path("valid.ucof");
    let make = command().args(["make-demo", path_str(&path)]).output().expect("make demo");
    assert!(make.status.success(), "{}", String::from_utf8_lossy(&make.stderr));

    let inspect = command().args(["inspect", path_str(&path)]).output().expect("inspect");
    assert!(inspect.status.success());
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("integrity not checked"));

    let verify = command().args(["verify", path_str(&path)]).output().expect("verify");
    assert!(verify.status.success());
    assert!(String::from_utf8_lossy(&verify.stdout).contains("verified UCOF-EXP-0001"));

    let diagnose = command().args(["diagnose", path_str(&path)]).output().expect("diagnose");
    assert!(diagnose.status.success());
    assert!(String::from_utf8_lossy(&diagnose.stdout).contains("Verified"));

    let salvage = command().args(["salvage", path_str(&path)]).output().expect("salvage");
    assert!(salvage.status.success());
    assert!(String::from_utf8_lossy(&salvage.stdout).contains("UNVERIFIED"));

    let _ = fs::remove_file(path);
}

#[test]
fn diagnose_returns_failure_for_digest_tampering() {
    let path = temporary_path("tampered.ucof");
    let make = command().args(["make-demo", path_str(&path)]).output().expect("make demo");
    assert!(make.status.success());

    let mut bytes = fs::read(&path).expect("read demo");
    bytes[32 + 40] ^= 1;
    fs::write(&path, bytes).expect("write tampered demo");

    let output = command().args(["diagnose", path_str(&path)]).output().expect("diagnose");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("DigestMismatch"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("input is invalid"));

    let _ = fs::remove_file(path);
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ucof"))
}

fn temporary_path(suffix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("ucof-cli-{}-{nanos}-{suffix}", std::process::id()))
}

fn path_str(path: &PathBuf) -> &str {
    path.to_str().expect("temporary path is UTF-8")
}
