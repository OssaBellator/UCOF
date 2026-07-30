use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};
use ucof_experiments::exp0002::{validate_strict, ValidationLimits};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ucof-exp0002")
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn decode_hex(text: &str) -> Vec<u8> {
    let text = text.trim();
    assert_eq!(text.len() % 2, 0, "hex vector has an odd length");
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("hex high nibble");
            let low = (pair[1] as char).to_digit(16).expect("hex low nibble");
            u8::try_from((high << 4) | low).expect("hex byte")
        })
        .collect()
}

fn read_hex(path: impl AsRef<Path>) -> Vec<u8> {
    let path = path.as_ref();
    decode_hex(
        &fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
    )
}

fn temporary_directory() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "ucof-exp0002-cli-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&path).expect("temporary directory");
    path
}

fn run(arguments: &[&str]) -> Output {
    Command::new(binary())
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("failed to run CLI: {error}"))
}

fn assert_success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "CLI failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).expect("UTF-8 stdout")
}

#[test]
fn read_only_commands_preserve_assurance_boundaries() {
    let temporary = temporary_directory();
    let root = repository_root();
    let archive = temporary.join("append.ucof");
    fs::write(
        &archive,
        read_hex(root.join("tests/vectors/exp-0002/append-add-third.hex")),
    )
    .expect("archive");
    let archive_text = archive.to_string_lossy();

    let verify = assert_success(&run(&["verify", &archive_text]));
    assert!(verify.contains("assurance: full strict exact-end validation"));
    assert!(verify.contains("sequence: 1"));

    let roots = assert_success(&run(&["roots", &archive_text]));
    assert!(roots.contains("fully validated active exact-end snapshot"));
    assert!(roots.contains("root: 1"));

    let history = assert_success(&run(&["history", &archive_text]));
    assert!(history.contains("commits: 2"));
    assert!(history.contains("sequence=1"));
    assert!(history.contains("sequence=0"));

    let lookup = assert_success(&run(&["lookup", &archive_text, "1"]));
    assert!(lookup.contains("one directory path, selected object"));
    assert!(lookup.contains("object_id: 1"));

    let missing = assert_success(&run(&["lookup", &archive_text, "999999"]));
    assert!(missing.contains("authenticated absence"));

    fs::remove_dir_all(temporary).expect("cleanup");
}

#[test]
fn recovery_reports_only_strict_prefixes() {
    let temporary = temporary_directory();
    let root = repository_root();
    let damaged = temporary.join("interrupted.ucof");
    fs::write(
        &damaged,
        read_hex(
            root.join("tests/vectors/exp-0002-invalid/append-cut-footer-prefix.hex"),
        ),
    )
    .expect("damaged vector");

    let output = assert_success(&run(&["recover", &damaged.to_string_lossy()]));
    assert!(output.contains("explicitly requested bounded recovery"));
    assert!(output.contains("verified_prefixes: 1"));
    assert!(output.contains("sequence=0"));
    assert!(output.contains("roots=1"));

    fs::remove_dir_all(temporary).expect("cleanup");
}

#[test]
fn rewrite_commands_create_new_strictly_valid_outputs() {
    let temporary = temporary_directory();
    let root = repository_root();
    let input = temporary.join("input.ucof");
    fs::write(
        &input,
        read_hex(root.join("tests/vectors/exp-0002/append-add-third.hex")),
    )
    .expect("input");

    let repaired = temporary.join("repaired.ucof");
    let repair = assert_success(&run(&[
        "repair-all",
        &input.to_string_lossy(),
        &repaired.to_string_lossy(),
        "00112233445566778899aabbccddeeff",
        "102132435465768798a9bacbdcedfe0f",
    ]));
    assert!(repair.contains("verified-source repair"));
    validate_strict(
        &fs::read(&repaired).expect("repaired bytes"),
        &ValidationLimits::default(),
    )
    .expect("valid repair output");

    let selected = temporary.join("selected.ucof");
    let rewrite = assert_success(&run(&[
        "rewrite-selected",
        &input.to_string_lossy(),
        &selected.to_string_lossy(),
        "ffeeddccbbaa99887766554433221100",
        "0ffeeddccbbaa9988776655443322110",
        "1",
        "1",
    ]));
    assert!(rewrite.contains("semantic_compaction_claim: false"));
    let selected_bytes = fs::read(&selected).expect("selected bytes");
    let selected_report =
        validate_strict(&selected_bytes, &ValidationLimits::default()).expect("valid rewrite");
    assert_eq!(selected_report.objects.len(), 1);
    assert_eq!(selected_report.snapshot.roots, vec![1]);

    let existing = run(&[
        "repair-all",
        &input.to_string_lossy(),
        &repaired.to_string_lossy(),
        "00112233445566778899aabbccddeeff",
        "102132435465768798a9bacbdcedfe0f",
    ]);
    assert!(!existing.status.success());

    fs::remove_dir_all(temporary).expect("cleanup");
}
