use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, scan_recovery_candidates, validate, validate_history,
    ImmutableError, ImmutableLimits, ImmutableObjectInput, FOOTER_LEN,
};

fn objects() -> Vec<ImmutableObjectInput> {
    vec![
        ImmutableObjectInput::new(1, 1, b"alpha".to_vec()),
        ImmutableObjectInput::new(2, 2, b"bravo".to_vec()),
        ImmutableObjectInput::new(3, 3, b"charlie".to_vec()),
        ImmutableObjectInput::new(4, 1, b"delta".to_vec()),
    ]
}

fn two_commits() -> (Vec<u8>, Vec<u8>) {
    let genesis = build_genesis(&objects(), ImmutableLimits::default()).expect("genesis");
    let appended = append_replacement(
        &genesis,
        &ImmutableObjectInput::new(1, 9, b"alpha-v2".to_vec()),
        ImmutableLimits::default(),
    )
    .expect("append");
    (genesis, appended)
}

#[test]
fn validates_linked_history_newest_to_oldest() {
    let (genesis, appended) = two_commits();
    let history = validate_history(&appended, ImmutableLimits::default()).expect("history");
    assert_eq!(history.entries.len(), 2);
    assert_eq!(history.entries[0].report.sequence, 1);
    assert_eq!(history.entries[1].report.sequence, 0);
    assert_eq!(
        history.entries[0].footer_offset as usize,
        appended.len() - FOOTER_LEN
    );
    assert_eq!(
        history.entries[1].footer_offset as usize,
        genesis.len() - FOOTER_LEN
    );
    assert_eq!(history.entries[0].report.object_count, 4);
    assert_eq!(history.entries[1].report.object_count, 4);
}

#[test]
fn history_revalidates_prior_commit_instead_of_trusting_linkage() {
    let (genesis, mut appended) = two_commits();
    let previous_footer = genesis.len() - FOOTER_LEN;
    appended[previous_footer + 80] ^= 0x01;

    assert_eq!(
        validate(&appended, ImmutableLimits::default())
            .expect("current commit remains structurally valid")
            .sequence,
        1
    );
    assert_eq!(
        validate_history(&appended, ImmutableLimits::default()),
        Err(ImmutableError::Invalid("commit digest"))
    );
}

#[test]
fn history_limit_fails_closed_before_omitting_an_older_entry() {
    let (_, appended) = two_commits();
    let limits = ImmutableLimits {
        max_history_entries: 1,
        ..ImmutableLimits::default()
    };
    assert_eq!(
        validate_history(&appended, limits),
        Err(ImmutableError::Limit("history entries"))
    );
}

#[test]
fn recovery_reports_valid_prefixes_without_selecting_one() {
    let (genesis, appended) = two_commits();
    let report =
        scan_recovery_candidates(&appended, ImmutableLimits::default()).expect("scan");
    assert_eq!(report.candidates.len(), 2);
    assert_eq!(report.candidates[0].report.sequence, 1);
    assert_eq!(report.candidates[1].report.sequence, 0);
    assert_eq!(report.candidates[0].prefix_len as usize, appended.len());
    assert_eq!(report.candidates[1].prefix_len as usize, genesis.len());
    assert!(!report.attempts_truncated);
    assert!(!report.candidates_truncated);
}

#[test]
fn interrupted_append_reports_only_the_complete_genesis_prefix() {
    let (genesis, mut appended) = two_commits();
    appended.truncate(appended.len() - 17);
    let report =
        scan_recovery_candidates(&appended, ImmutableLimits::default()).expect("scan");
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].report.sequence, 0);
    assert_eq!(report.candidates[0].prefix_len as usize, genesis.len());
}

#[test]
fn recovery_scan_and_attempt_budgets_are_explicit() {
    let (_, appended) = two_commits();
    let no_window = ImmutableLimits {
        max_recovery_scan_bytes: 0,
        ..ImmutableLimits::default()
    };
    let report = scan_recovery_candidates(&appended, no_window).expect("zero scan");
    assert_eq!(report.scanned_bytes, 0);
    assert!(report.candidates.is_empty());

    let hostile = b"UCFTIM02UCFTIM02UCFTIM02";
    let no_attempts = ImmutableLimits {
        max_recovery_scan_bytes: hostile.len(),
        max_recovery_attempts: 0,
        ..ImmutableLimits::default()
    };
    let report = scan_recovery_candidates(hostile, no_attempts).expect("bounded scan");
    assert_eq!(report.attempted_footers, 0);
    assert!(report.attempts_truncated);
    assert!(report.candidates.is_empty());
}

#[test]
fn recovery_candidate_cap_keeps_newest_valid_prefix_and_marks_truncation() {
    let (_, appended) = two_commits();
    let limits = ImmutableLimits {
        max_recovery_candidates: 1,
        ..ImmutableLimits::default()
    };
    let report = scan_recovery_candidates(&appended, limits).expect("scan");
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].report.sequence, 1);
    assert!(report.candidates_truncated);
}
