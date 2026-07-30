use ucof_experiments::exp0002::{validate_strict, ValidationLimits};
use ucof_experiments::exp0002_recovery::{scan_valid_prefixes, Exp0002RecoveryLimits};

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

const CASES: &[(&str, &str)] = &[
    (
        "header-reserved-nonzero",
        include_str!("../../../tests/vectors/exp-0002-invalid/header-reserved-nonzero.hex"),
    ),
    (
        "object-logical-length-mismatch",
        include_str!("../../../tests/vectors/exp-0002-invalid/object-logical-length-mismatch.hex"),
    ),
    (
        "leaf-padding-nonzero",
        include_str!("../../../tests/vectors/exp-0002-invalid/leaf-padding-nonzero.hex"),
    ),
    (
        "internal-child-range-overlap",
        include_str!("../../../tests/vectors/exp-0002-invalid/internal-child-range-overlap.hex"),
    ),
    (
        "object-overlaps-directory-page",
        include_str!("../../../tests/vectors/exp-0002-invalid/object-overlaps-directory-page.hex"),
    ),
    (
        "snapshot-reserved-nonzero",
        include_str!("../../../tests/vectors/exp-0002-invalid/snapshot-reserved-nonzero.hex"),
    ),
    (
        "previous-footer-forward-pointer",
        include_str!("../../../tests/vectors/exp-0002-invalid/previous-footer-forward-pointer.hex"),
    ),
    (
        "parent-snapshot-digest-mismatch",
        include_str!("../../../tests/vectors/exp-0002-invalid/parent-snapshot-digest-mismatch.hex"),
    ),
    (
        "strict-trailing-bytes",
        include_str!("../../../tests/vectors/exp-0002-invalid/strict-trailing-bytes.hex"),
    ),
    (
        "footer-truncated-one-byte",
        include_str!("../../../tests/vectors/exp-0002-invalid/footer-truncated-one-byte.hex"),
    ),
    (
        "append-cut-after-object-header",
        include_str!("../../../tests/vectors/exp-0002-invalid/append-cut-after-object-header.hex"),
    ),
    (
        "append-cut-before-snapshot-complete",
        include_str!("../../../tests/vectors/exp-0002-invalid/append-cut-before-snapshot-complete.hex"),
    ),
    (
        "append-cut-footer-prefix",
        include_str!("../../../tests/vectors/exp-0002-invalid/append-cut-footer-prefix.hex"),
    ),
];

#[test]
fn every_pinned_invalid_vector_is_rejected_strictly() {
    for (name, hex) in CASES {
        let bytes = decode_hex(hex);
        assert!(
            validate_strict(&bytes, &ValidationLimits::default()).is_err(),
            "invalid vector was accepted: {name}"
        );
    }
}

#[test]
fn interrupted_append_vectors_can_recover_the_earlier_complete_prefix() {
    for (name, hex) in CASES
        .iter()
        .filter(|(name, _)| name.starts_with("append-cut-"))
    {
        let bytes = decode_hex(hex);
        let report = scan_valid_prefixes(
            &bytes,
            &ValidationLimits::default(),
            &Exp0002RecoveryLimits {
                max_scan_bytes: bytes.len(),
                ..Exp0002RecoveryLimits::default()
            },
        )
        .expect("bounded recovery scan");
        let latest = report
            .latest()
            .unwrap_or_else(|| panic!("no recoverable prefix for {name}"));
        assert_eq!(latest.sequence, 0, "wrong recovered sequence for {name}");
        assert!(latest.prefix_len < bytes.len(), "tail was not excluded for {name}");
    }
}
