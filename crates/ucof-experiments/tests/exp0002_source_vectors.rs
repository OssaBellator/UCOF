use std::fs;
use std::path::{Path, PathBuf};
use ucof_experiments::exp0002_source::{Exp0002SliceSource, Exp0002SourceLimits};
use ucof_experiments::{
    scan_valid_prefixes_at, validate_strict_at, Exp0002SourceRecoveryLimits,
};

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

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn read_hex(path: impl AsRef<Path>) -> Vec<u8> {
    let path = path.as_ref();
    decode_hex(&fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display())
    }))
}

#[test]
fn every_valid_vector_passes_full_source_validation() {
    let root = repository_root().join("tests/vectors/exp-0002");
    for name in [
        "genesis-two-object.hex",
        "append-add-third.hex",
        "multi-leaf-400.hex",
    ] {
        let bytes = read_hex(root.join(name));
        let mut source = Exp0002SliceSource::new(&bytes);
        let report = validate_strict_at(&mut source, &Exp0002SourceLimits::default())
            .unwrap_or_else(|error| panic!("{name} failed full source validation: {error}"));
        assert!(!report.objects.is_empty(), "{name} has no objects");
        assert!(report.pages_verified > 0, "{name} has no verified pages");
        assert!(report.stats.bytes_hashed > 0, "{name} hashed no bytes");
    }
}

#[test]
fn every_pinned_invalid_vector_fails_full_source_validation() {
    let root = repository_root().join("tests/vectors/exp-0002-invalid");
    let mut paths: Vec<_> = fs::read_dir(&root)
        .expect("invalid vector directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "hex"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 13, "unexpected invalid-vector count");

    for path in paths {
        let bytes = read_hex(&path);
        let mut source = Exp0002SliceSource::new(&bytes);
        assert!(
            validate_strict_at(&mut source, &Exp0002SourceLimits::default()).is_err(),
            "{} unexpectedly passed full source validation",
            path.display()
        );
    }
}

#[test]
fn interrupted_append_vectors_recover_only_the_previous_complete_commit() {
    let root = repository_root();
    let genesis = read_hex(root.join("tests/vectors/exp-0002/genesis-two-object.hex"));
    let expected_prefix_len = u64::try_from(genesis.len()).expect("genesis length");
    let invalid = root.join("tests/vectors/exp-0002-invalid");

    for name in [
        "append-cut-after-object-header.hex",
        "append-cut-before-snapshot-complete.hex",
        "append-cut-footer-prefix.hex",
    ] {
        let bytes = read_hex(invalid.join(name));
        let mut source = Exp0002SliceSource::new(&bytes);
        let report = scan_valid_prefixes_at(
            &mut source,
            &Exp0002SourceRecoveryLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{name} recovery failed: {error}"));
        assert!(
            report
                .results
                .iter()
                .any(|candidate| candidate.prefix_len == expected_prefix_len),
            "{name} did not recover the pinned genesis prefix"
        );
        assert!(
            report.results.iter().all(|candidate| candidate.sequence == 0),
            "{name} reported an incomplete append as a valid newer commit"
        );
    }
}
