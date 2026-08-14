fn encrypted_sorter_sessions(
    authority: &mut DescriptorNonceAuthority,
    key: [u8; 32],
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    records: u64,
) -> (DescriptorEncryptionSession, DescriptorEncryptionSession) {
    let spill = authority
        .activate_session(key, nonce_prefix, operation_id, records, records, true)
        .expect("spill lease");
    let retained = authority
        .activate_session(key, nonce_prefix, operation_id, records, records, true)
        .expect("retained lease");
    (spill, retained)
}

#[test]
fn encrypted_sorter_runs_preserve_canonical_output_and_report() {
    const OBJECTS: u64 = 401;
    let limits = ImmutableLimits::default();
    let spill_limits = spill_limits(17, 3);
    let original: Vec<_> = (1..=OBJECTS).rev().map(TinySource::new).collect();

    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        options(),
        limits,
    )
    .expect("baseline writer");

    let directory = TestDirectory::new("encrypted-sorter-equivalence");

    let mut authority_a = DescriptorNonceAuthority::initial();
    let (mut spill_a, mut retained_a) = encrypted_sorter_sessions(
        &mut authority_a,
        [0x91; 32],
        [0x21; 4],
        [0xc1; 16],
        OBJECTS,
    );
    let mut sources_a = original.clone();
    let mut output_a = Vec::new();
    let evidence_a = write_genesis_sources_end_to_end_encrypted_sorter_candidate(
        &mut output_a,
        &mut sources_a,
        &directory.0,
        options(),
        limits,
        spill_limits,
        &mut spill_a,
        &mut retained_a,
    )
    .expect("first encrypted sorter writer");
    directory.assert_empty();

    let mut authority_b = DescriptorNonceAuthority::initial();
    let (mut spill_b, mut retained_b) = encrypted_sorter_sessions(
        &mut authority_b,
        [0x91; 32],
        [0x22; 4],
        [0xc1; 16],
        OBJECTS,
    );
    let mut sources_b = original;
    let mut output_b = Vec::new();
    let evidence_b = write_genesis_sources_end_to_end_encrypted_sorter_candidate(
        &mut output_b,
        &mut sources_b,
        &directory.0,
        options(),
        limits,
        spill_limits,
        &mut spill_b,
        &mut retained_b,
    )
    .expect("second encrypted sorter writer");
    directory.assert_empty();

    assert_eq!(output_a, baseline);
    assert_eq!(output_b, baseline);
    assert_eq!(evidence_a.output, baseline_report);
    assert_eq!(evidence_b.output, baseline_report);
    assert_eq!(
        evidence_a.descriptor_spill.output_payload_bytes,
        OBJECTS * u64::try_from(ENCRYPTED_SORTER_PAYLOAD_BYTES).expect("sorter payload width")
    );
    assert_eq!(
        evidence_a.descriptor_spill.final_run_bytes_read,
        OBJECTS * u64::try_from(ENCRYPTED_SORTER_FRAME_BYTES).expect("sorter frame width")
    );
    assert_eq!(
        evidence_a.descriptor_stage_bytes,
        OBJECTS * u64::try_from(ENCRYPTED_DESCRIPTOR_STAGE_BYTES).expect("retained frame width")
    );
    assert_ne!(
        evidence_a.descriptor_spill.output_sha256,
        evidence_b.descriptor_spill.output_sha256
    );
    assert_ne!(
        evidence_a.descriptor_ciphertext_sha256,
        evidence_b.descriptor_ciphertext_sha256
    );
    assert_eq!(spill_a.remaining(), 0);
    assert_eq!(retained_a.remaining(), 0);
    assert_eq!(spill_b.remaining(), 0);
    assert_eq!(retained_b.remaining(), 0);
    assert_eq!(authority_a.next_unreserved(), Some(OBJECTS * 2));
    assert_eq!(authority_b.next_unreserved(), Some(OBJECTS * 2));
}

#[test]
fn encrypted_sorter_private_quota_prices_final_run_and_retained_stage_overlap() {
    const OBJECTS: usize = 4;
    const RECORDS: u64 = 4;
    let mut sorter_limits = spill_limits(8, 2);
    sorter_limits.max_live_spill_bytes = RECORDS
        * u64::try_from(ENCRYPTED_SORTER_FRAME_BYTES).expect("encrypted sorter frame width");
    let plan =
        encrypted_sorter_private_storage_plan(OBJECTS, sorter_limits).expect("private plan");
    assert_eq!(plan.encrypted_sorter_frame_bytes, 108);
    assert_eq!(sorter_limits.max_live_spill_bytes, 432);
    assert_eq!(plan.retained_descriptor_bytes, 368);
    assert_eq!(plan.locator_bytes, 288);
    assert_eq!(plan.sorter_plus_retained_descriptor_bytes, 800);
    assert_eq!(plan.retained_descriptor_plus_locator_bytes, 656);
    assert_eq!(plan.required_bytes, 800);

    let original: Vec<_> = (1..=RECORDS).rev().map(TinySource::new).collect();
    let directory = TestDirectory::new("encrypted-sorter-quota");

    let mut exact_authority = DescriptorNonceAuthority::initial();
    let (mut exact_spill, mut exact_retained) = encrypted_sorter_sessions(
        &mut exact_authority,
        [0xa1; 32],
        [0x31; 4],
        [0xd1; 16],
        RECORDS,
    );
    let mut exact_sources = original.clone();
    let mut exact_output = Vec::new();
    let (observed, evidence) = write_genesis_sources_with_encrypted_sorter_private_quota_candidate(
        &mut exact_output,
        &mut exact_sources,
        &directory.0,
        sorter_limits,
        EncryptedSorterWriterSettings {
            options: options(),
            limits: ImmutableLimits::default(),
            max_private_storage_bytes: plan.required_bytes,
        },
        &mut exact_spill,
        &mut exact_retained,
    )
    .expect("exact encrypted sorter private quota");
    assert_eq!(observed, plan);
    assert!(!exact_output.is_empty());
    assert_eq!(evidence.descriptor_spill.peak_live_spill_bytes, 432);
    assert_eq!(evidence.descriptor_stage_bytes, 368);
    assert_eq!(exact_spill.remaining(), 0);
    assert_eq!(exact_retained.remaining(), 0);
    directory.assert_empty();

    let mut short_authority = DescriptorNonceAuthority::initial();
    let (mut short_spill, mut short_retained) = encrypted_sorter_sessions(
        &mut short_authority,
        [0xa1; 32],
        [0x32; 4],
        [0xd2; 16],
        RECORDS,
    );
    let spill_remaining = short_spill.remaining();
    let retained_remaining = short_retained.remaining();
    let mut short_sources = original;
    let mut short_output = Vec::new();
    let error = write_genesis_sources_with_encrypted_sorter_private_quota_candidate(
        &mut short_output,
        &mut short_sources,
        &directory.0,
        sorter_limits,
        EncryptedSorterWriterSettings {
            options: options(),
            limits: ImmutableLimits::default(),
            max_private_storage_bytes: plan.required_bytes - 1,
        },
        &mut short_spill,
        &mut short_retained,
    )
    .expect_err("one byte short");
    assert!(error.contains("private storage limit"));
    assert!(short_output.is_empty());
    assert_eq!(short_spill.remaining(), spill_remaining);
    assert_eq!(short_retained.remaining(), retained_remaining);
    directory.assert_empty();
}

#[test]
fn encrypted_sorter_short_lease_fails_before_private_files() {
    let directory = TestDirectory::new("encrypted-sorter-short-lease");
    let mut sources = [
        TinySource::new(4),
        TinySource::new(3),
        TinySource::new(2),
        TinySource::new(1),
    ];
    let mut authority = DescriptorNonceAuthority::initial();
    let mut spill_session = authority
        .activate_session([0xb1; 32], [0x41; 4], [0xe1; 16], 3, 3, true)
        .expect("short spill lease");
    let mut retained_session = authority
        .activate_session([0xb1; 32], [0x41; 4], [0xe1; 16], 4, 4, true)
        .expect("retained lease");
    let spill_remaining = spill_session.remaining();
    let retained_remaining = retained_session.remaining();
    let mut output = Vec::new();
    let error = write_genesis_sources_end_to_end_encrypted_sorter_candidate(
        &mut output,
        &mut sources,
        &directory.0,
        options(),
        ImmutableLimits::default(),
        spill_limits(4, 2),
        &mut spill_session,
        &mut retained_session,
    )
    .expect_err("short spill lease must fail");
    assert!(error.contains("sorter nonce lease capacity"));
    assert!(output.is_empty());
    assert_eq!(spill_session.remaining(), spill_remaining);
    assert_eq!(retained_session.remaining(), retained_remaining);
    directory.assert_empty();
}

#[test]
fn encrypted_sorter_keeps_duplicate_detection_in_existing_merge_logic() {
    let directory = TestDirectory::new("encrypted-sorter-duplicate");
    let mut sources = [
        TinySource::new(1),
        TinySource::new(3),
        TinySource::new(2),
        TinySource::new(4),
        TinySource::new(2),
    ];
    let mut authority = DescriptorNonceAuthority::initial();
    let (mut spill_session, mut retained_session) = encrypted_sorter_sessions(
        &mut authority,
        [0xc1; 32],
        [0x51; 4],
        [0xf1; 16],
        5,
    );
    let mut output = Vec::new();
    let error = write_genesis_sources_end_to_end_encrypted_sorter_candidate(
        &mut output,
        &mut sources,
        &directory.0,
        options(),
        ImmutableLimits::default(),
        spill_limits(2, 2),
        &mut spill_session,
        &mut retained_session,
    )
    .expect_err("duplicate across encrypted runs");
    assert!(error.contains("duplicate spill key 2"));
    assert!(output.is_empty());
    assert_eq!(spill_session.remaining(), 0);
    assert_eq!(retained_session.remaining(), 5);
    directory.assert_empty();
}
