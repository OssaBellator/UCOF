use ucof_core::{Error, ErrorCategory, Limits, Manifest, ValidatedFile, Writer};

#[test]
fn deterministic_round_trip_and_lookup() {
    let bytes_a = demo_bytes();
    let bytes_b = demo_bytes();
    assert_eq!(bytes_a, bytes_b);

    let file = ValidatedFile::parse(&bytes_a, &Limits::default()).expect("valid demo");
    assert_eq!(file.manifest.roots, vec![1, 2]);
    assert_eq!(file.object(1), Some(&b"hello"[..]));
    assert_eq!(file.object(2), Some(&b""[..]));
    assert!(file.inspect().contains("UCOF-EXP-0001"));
}

#[test]
fn checked_in_minimal_vector_matches_writer() {
    let expected = decode_hex(include_str!("../../../tests/vectors/exp-0001/minimal-valid.hex"));
    let mut writer = Writer::new();
    writer
        .add_manifest(1, &Manifest::new(Vec::new()))
        .expect("manifest");
    let actual = writer.finish(1).expect("finish");
    assert_eq!(actual, expected);
}

#[test]
fn checked_in_two_object_vector_is_valid() {
    let bytes = decode_hex(include_str!("../../../tests/vectors/exp-0001/two-objects.hex"));
    let file = ValidatedFile::parse(&bytes, &Limits::default()).expect("valid vector");
    assert_eq!(file.manifest.roots, vec![1, 2]);
    assert_eq!(file.object(1), Some(&b"hello"[..]));
    assert_eq!(file.object(2), Some(&b""[..]));
}

#[test]
fn every_truncation_fails_without_panic() {
    let bytes = demo_bytes();
    for length in 0..bytes.len() {
        let result = ValidatedFile::parse(&bytes[..length], &Limits::default());
        assert!(result.is_err(), "truncation at {length} unexpectedly passed");
    }
}

#[test]
fn modified_prefix_fails_digest() {
    let mut bytes = demo_bytes();
    bytes[40] ^= 1;
    let error = ValidatedFile::parse(&bytes, &Limits::default()).expect_err("tamper must fail");
    assert_eq!(error.category(), ErrorCategory::DigestMismatch);
}

#[test]
fn required_capability_fails_closed() {
    let mut writer = Writer::new();
    writer.add_opaque(1, b"x").expect("object");
    let mut manifest = Manifest::new(vec![1]);
    manifest.required_capabilities.push(42);
    writer.add_manifest(2, &manifest).expect("manifest");
    let bytes = writer.finish(2).expect("finish");
    let error = ValidatedFile::parse(&bytes, &Limits::default()).expect_err("unsupported");
    assert_eq!(error, Error::UnsupportedRequiredCapability(42));
}

#[test]
fn file_size_limit_is_enforced_before_parsing() {
    let bytes = demo_bytes();
    let limits = Limits {
        max_file_bytes: 1,
        ..Limits::default()
    };
    let error = ValidatedFile::parse(&bytes, &limits).expect_err("limit must fail");
    assert_eq!(error.category(), ErrorCategory::LimitExceeded);
}

fn demo_bytes() -> Vec<u8> {
    let mut writer = Writer::new();
    writer.add_opaque(1, b"hello").expect("object one");
    writer.add_opaque(2, b"").expect("object two");
    let mut manifest = Manifest::new(vec![1, 2]);
    manifest.optional_capabilities.push(9001);
    writer.add_manifest(3, &manifest).expect("manifest");
    writer.finish(3).expect("finish")
}

fn decode_hex(source: &str) -> Vec<u8> {
    let compact: String = source.chars().filter(|character| !character.is_whitespace()).collect();
    assert_eq!(compact.len() % 2, 0, "hex fixture length");
    compact
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).expect("ASCII hex");
            u8::from_str_radix(text, 16).expect("valid hex")
        })
        .collect()
}
