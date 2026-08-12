use ucof_core::{
    DiagnosticStatus, DiagnosticValidator, Error, ErrorCategory, Limits, Manifest, PrefixSalvager,
    SliceSource, Writer,
};

#[test]
fn valid_file_diagnoses_as_verified() {
    let bytes = demo_file();
    let mut source = SliceSource::new(&bytes);
    let report = DiagnosticValidator::default()
        .diagnose(&mut source)
        .expect("diagnostic report");

    assert_eq!(report.status, DiagnosticStatus::Verified);
    assert!(report.diagnostics.is_empty());
    assert!(report.inspection.is_some());
    assert!(report.validation.is_some());
}

#[test]
fn digest_failure_retains_structural_report_without_upgrading_validity() {
    let mut bytes = demo_file();
    bytes[32 + 40] ^= 1;
    let mut source = SliceSource::new(&bytes);
    let report = DiagnosticValidator::default()
        .diagnose(&mut source)
        .expect("diagnostic report");

    assert_eq!(report.status, DiagnosticStatus::Invalid);
    assert!(report.inspection.is_some());
    assert!(report.validation.is_none());
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(
        report.diagnostics[0].category,
        ErrorCategory::DigestMismatch
    );
}

#[test]
fn malformed_bootstrap_has_no_partial_structural_claim() {
    let mut bytes = demo_file();
    bytes[0] ^= 1;
    let mut source = SliceSource::new(&bytes);
    let report = DiagnosticValidator::default()
        .diagnose(&mut source)
        .expect("diagnostic report");

    assert_eq!(report.status, DiagnosticStatus::Invalid);
    assert!(report.inspection.is_none());
    assert!(report.validation.is_none());
    assert_eq!(report.diagnostics[0].category, ErrorCategory::InvalidMagic);
}

#[test]
fn prefix_salvage_reports_only_complete_records_before_truncation() {
    let bytes = demo_file();
    let truncated = &bytes[..120];
    let mut source = SliceSource::new(truncated);
    let report = PrefixSalvager::default()
        .scan(&mut source)
        .expect("prefix salvage");

    assert_eq!(report.status, DiagnosticStatus::UnverifiedPrefix);
    assert_eq!(report.records.len(), 1);
    assert_eq!(report.records[0].object_id, 1);
    assert!(!report.reached_directory);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].category, ErrorCategory::Truncated);
    assert_eq!(report.diagnostics[0].offset, Some(75));
}

#[test]
fn complete_prefix_scan_still_never_claims_conformance() {
    let bytes = demo_file();
    let mut source = SliceSource::new(&bytes);
    let report = PrefixSalvager::default()
        .scan(&mut source)
        .expect("prefix salvage");

    assert_eq!(report.status, DiagnosticStatus::UnverifiedPrefix);
    assert!(report.reached_directory);
    assert!(report.diagnostics.is_empty());
    assert_eq!(report.records.last().expect("directory").object_id, 0);
}

#[test]
fn zero_diagnostic_budget_fails_before_work() {
    let bytes = demo_file();
    let limits = Limits {
        max_diagnostics: 0,
        ..Limits::default()
    };
    let mut source = SliceSource::new(&bytes);
    let error = DiagnosticValidator::new(limits)
        .diagnose(&mut source)
        .expect_err("zero diagnostic budget");

    assert_eq!(error, Error::LimitExceeded("diagnostics"));
}

fn demo_file() -> Vec<u8> {
    let mut writer = Writer::new();
    writer.add_opaque(1, b"abc").expect("object one");
    writer.add_opaque(2, &[0x5a; 20]).expect("object two");
    writer
        .add_manifest(3, &Manifest::new(vec![1, 2]))
        .expect("manifest");
    writer.finish(3).expect("file")
}
