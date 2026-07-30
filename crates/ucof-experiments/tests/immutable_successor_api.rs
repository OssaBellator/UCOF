use sha2::{Digest, Sha256};
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, validate, ImmutableError, ImmutableLimits,
    ImmutableObjectInput, FOOTER_LEN,
};

const COMMIT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-COMMIT\0";

fn decode_hex(input: &str) -> Vec<u8> {
    let digits: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    assert_eq!(digits.len() % 2, 0);
    digits
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("high nibble");
            let low = (pair[1] as char).to_digit(16).expect("low nibble");
            ((high << 4) | low) as u8
        })
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn assert_bytes_equal(actual: &[u8], expected: &[u8], label: &str) {
    if actual == expected {
        return;
    }
    let first = actual
        .iter()
        .zip(expected)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| actual.len().min(expected.len()));
    let start = first.saturating_sub(16);
    let actual_end = actual.len().min(first.saturating_add(17));
    let expected_end = expected.len().min(first.saturating_add(17));
    panic!(
        "{label} differs: actual_len={} expected_len={} first_offset={} actual_sha256={} expected_sha256={} actual_window={:02x?} expected_window={:02x?}",
        actual.len(),
        expected.len(),
        first,
        sha256_hex(actual),
        sha256_hex(expected),
        &actual[start..actual_end],
        &expected[start..expected_end],
    );
}

fn four_objects() -> Vec<ImmutableObjectInput> {
    vec![
        ImmutableObjectInput::new(1, 1, b"alpha".to_vec()),
        ImmutableObjectInput::new(2, 2, b"bravo".to_vec()),
        ImmutableObjectInput::new(3, 3, b"charlie".to_vec()),
        ImmutableObjectInput::new(4, 4, b"delta".to_vec()),
    ]
}

#[test]
fn validates_and_reproduces_the_pinned_genesis() {
    let pinned = decode_hex(include_str!(
        "../../../tests/vectors/exp-0002-immutable/genesis-four.hex"
    ));
    let report = validate(&pinned, ImmutableLimits::default()).expect("pinned vector validates");
    assert_eq!(report.sequence, 0);
    assert_eq!(report.object_count, 4);
    assert_eq!(report.page_count, 1);
    assert_eq!(report.root_level, 0);

    let generated = build_genesis(&four_objects(), ImmutableLimits::default())
        .expect("genesis generation succeeds");
    assert_bytes_equal(&generated, &pinned, "generated genesis");
    assert_eq!(
        sha256_hex(&generated),
        "94f9441339fb49ffef5b8c7b54307c20488bf2e09958fd805fd2addae65c2a23"
    );

    let mut reversed = four_objects();
    reversed.reverse();
    assert_eq!(
        build_genesis(&reversed, ImmutableLimits::default()).expect("order is canonicalized"),
        generated
    );
}

#[test]
fn reproduces_pinned_append_and_multi_level_recipes() {
    let genesis = build_genesis(&four_objects(), ImmutableLimits::default()).expect("genesis");
    let appended = append_replacement(
        &genesis,
        &ImmutableObjectInput::new(1, 9, b"alpha-v2".to_vec()),
        ImmutableLimits::default(),
    )
    .expect("append replacement");
    let append_report = validate(&appended, ImmutableLimits::default()).expect("append validates");
    assert_eq!(appended.len(), 33_550);
    assert_eq!(append_report.sequence, 1);
    assert_eq!(append_report.object_count, 4);
    assert_eq!(append_report.page_count, 1);
    assert_eq!(append_report.root_level, 0);
    assert_eq!(
        sha256_hex(&appended),
        "e058422145e12334934c86c51d29a480166e33d5b0d27538f6b26c9591db00bc"
    );

    let objects: Vec<_> = (1_u64..=400)
        .map(|object_id| {
            ImmutableObjectInput::new(
                object_id,
                u16::try_from(1 + object_id % 5).expect("kind"),
                format!("payload:{object_id}").into_bytes(),
            )
        })
        .collect();
    let multi = build_genesis(&objects, ImmutableLimits::default()).expect("multi-level genesis");
    let multi_report = validate(&multi, ImmutableLimits::default()).expect("multi-level validates");
    assert_eq!(multi.len(), 89_316);
    assert_eq!(multi_report.sequence, 0);
    assert_eq!(multi_report.object_count, 400);
    assert_eq!(multi_report.page_count, 4);
    assert_eq!(multi_report.root_level, 1);
    assert_eq!(
        sha256_hex(&multi),
        "d4cdc721028a8abad2f381328a0bcd605ef19d26fea30c1b214f094a16ba3f70"
    );
}

#[test]
fn enforces_writer_and_validator_limits_before_success() {
    let limits = ImmutableLimits {
        max_output_bytes: 64,
        ..ImmutableLimits::default()
    };
    assert_eq!(
        build_genesis(&four_objects(), limits),
        Err(ImmutableError::Limit("output"))
    );

    let genesis = build_genesis(&four_objects(), ImmutableLimits::default()).expect("genesis");
    let limits = ImmutableLimits {
        max_pages: 0,
        ..ImmutableLimits::default()
    };
    assert_eq!(
        validate(&genesis, limits),
        Err(ImmutableError::Limit("page count"))
    );
}

#[test]
fn rejects_reauthenticated_footer_page_count_mismatch() {
    let mut bytes = build_genesis(&four_objects(), ImmutableLimits::default()).expect("genesis");
    let footer_offset = bytes.len() - FOOTER_LEN;
    bytes[footer_offset + 40..footer_offset + 48].copy_from_slice(&2_u64.to_le_bytes());

    let mut hasher = Sha256::new();
    hasher.update(COMMIT_DOMAIN);
    hasher.update(&bytes[..footer_offset]);
    hasher.update(&bytes[footer_offset + 8..footer_offset + 80]);
    let digest: [u8; 32] = hasher.finalize().into();
    bytes[footer_offset + 80..footer_offset + 112].copy_from_slice(&digest);

    assert_eq!(
        validate(&bytes, ImmutableLimits::default()),
        Err(ImmutableError::Invalid("page count"))
    );
}
