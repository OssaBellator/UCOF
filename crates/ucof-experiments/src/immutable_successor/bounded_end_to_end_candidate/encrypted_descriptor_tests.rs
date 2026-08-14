#[test]
fn encrypted_descriptor_writer_preserves_canonical_bytes_and_reports() {
    const OBJECTS: u64 = 401;
    let limits = ImmutableLimits::default();
    let spill = spill_limits(17, 3);
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

    let directory = TestDirectory::new("encrypted-equivalence");
    let mut plain_sources = original.clone();
    let mut plain = Vec::new();
    let plain_evidence = write_genesis_sources_end_to_end_bounded_candidate(
        &mut plain,
        &mut plain_sources,
        &directory.0,
        options(),
        limits,
        spill,
    )
    .expect("plaintext bounded writer");
    assert_eq!(plain, baseline);
    assert_eq!(plain_evidence.output, baseline_report);
    assert_eq!(plain_evidence.descriptor_ciphertext_sha256, None);
    directory.assert_empty();

    let mut authority_a = DescriptorNonceAuthority::initial();
    let mut session_a = authority_a
        .activate_session(
            [0x41; 32],
            [0x11; 4],
            [0x71; 16],
            OBJECTS,
            OBJECTS,
            true,
        )
        .expect("activate first encrypted lease");
    let mut encrypted_sources_a = original.clone();
    let mut encrypted_output_a = Vec::new();
    let evidence_a = write_genesis_sources_end_to_end_encrypted_descriptor_candidate(
        &mut encrypted_output_a,
        &mut encrypted_sources_a,
        &directory.0,
        options(),
        limits,
        spill,
        &mut session_a,
    )
    .expect("first encrypted writer");

    let mut authority_b = DescriptorNonceAuthority::initial();
    let mut session_b = authority_b
        .activate_session(
            [0x41; 32],
            [0x22; 4],
            [0x71; 16],
            OBJECTS,
            OBJECTS,
            true,
        )
        .expect("activate second encrypted lease");
    let mut encrypted_sources_b = original;
    let mut encrypted_output_b = Vec::new();
    let evidence_b = write_genesis_sources_end_to_end_encrypted_descriptor_candidate(
        &mut encrypted_output_b,
        &mut encrypted_sources_b,
        &directory.0,
        options(),
        limits,
        spill,
        &mut session_b,
    )
    .expect("second encrypted writer");

    assert_eq!(encrypted_output_a, baseline);
    assert_eq!(encrypted_output_b, baseline);
    assert_eq!(evidence_a.output, baseline_report);
    assert_eq!(evidence_b.output, baseline_report);
    assert_eq!(
        evidence_a.descriptor_stage_bytes,
        OBJECTS * u64::try_from(ENCRYPTED_DESCRIPTOR_STAGE_BYTES).expect("encrypted width")
    );
    assert_eq!(
        evidence_a.descriptor_stage_bytes,
        evidence_b.descriptor_stage_bytes
    );
    assert_ne!(
        evidence_a.descriptor_ciphertext_sha256,
        evidence_b.descriptor_ciphertext_sha256
    );
    assert!(evidence_a.descriptor_ciphertext_sha256.is_some());
    assert_eq!(session_a.remaining(), 0);
    assert_eq!(session_b.remaining(), 0);
    assert_eq!(authority_a.next_unreserved(), Some(OBJECTS));
    assert_eq!(authority_b.next_unreserved(), Some(OBJECTS));
    directory.assert_empty();
}

#[test]
fn encrypted_descriptor_quota_prices_transcode_and_emission_overlaps() {
    const OBJECTS: usize = 4;

    let mut arithmetic_spill = spill_limits(8, 2);
    arithmetic_spill.max_live_spill_bytes = 256;
    let arithmetic_plan =
        encrypted_private_storage_plan(OBJECTS, arithmetic_spill).expect("encrypted plan");
    assert_eq!(arithmetic_plan.base.descriptor_bytes, 256);
    assert_eq!(arithmetic_plan.encrypted_descriptor_bytes, 368);
    assert_eq!(
        arithmetic_plan.plaintext_plus_encrypted_descriptor_bytes,
        624
    );
    assert_eq!(
        arithmetic_plan.encrypted_descriptor_plus_locator_bytes,
        656
    );
    assert_eq!(arithmetic_plan.required_bytes, 656);

    let writer_spill = spill_limits(8, 2);
    let writer_plan =
        encrypted_private_storage_plan(OBJECTS, writer_spill).expect("writer encrypted plan");
    assert!(writer_plan.required_bytes >= arithmetic_plan.required_bytes);
    assert_eq!(
        writer_plan.encrypted_descriptor_bytes,
        arithmetic_plan.encrypted_descriptor_bytes
    );

    let original: Vec<_> = (1..=u64::try_from(OBJECTS).unwrap())
        .rev()
        .map(TinySource::new)
        .collect();
    let directory = TestDirectory::new("encrypted-quota");

    let mut exact_authority = DescriptorNonceAuthority::initial();
    let mut exact_session = exact_authority
        .activate_session([0x51; 32], [0x31; 4], [0x81; 16], 4, 4, true)
        .expect("exact session");
    let mut exact_sources = original.clone();
    let mut exact_output = Vec::new();
    let (observed, evidence) =
        write_genesis_sources_with_encrypted_descriptor_private_quota_candidate(
            &mut exact_output,
            &mut exact_sources,
            &directory.0,
            writer_spill,
            EncryptedPrivateWriterSettings {
                options: options(),
                limits: ImmutableLimits::default(),
                max_private_storage_bytes: writer_plan.required_bytes,
            },
            &mut exact_session,
        )
        .expect("exact private quota");
    assert_eq!(observed, writer_plan);
    assert!(!exact_output.is_empty());
    assert_eq!(
        evidence.descriptor_stage_bytes,
        writer_plan.encrypted_descriptor_bytes
    );
    assert_eq!(exact_session.remaining(), 0);
    directory.assert_empty();

    let mut short_authority = DescriptorNonceAuthority::initial();
    let mut short_session = short_authority
        .activate_session([0x51; 32], [0x32; 4], [0x82; 16], 4, 4, true)
        .expect("short session");
    let remaining_before = short_session.remaining();
    let mut short_sources = original;
    let mut short_output = Vec::new();
    let error = write_genesis_sources_with_encrypted_descriptor_private_quota_candidate(
        &mut short_output,
        &mut short_sources,
        &directory.0,
        writer_spill,
        EncryptedPrivateWriterSettings {
            options: options(),
            limits: ImmutableLimits::default(),
            max_private_storage_bytes: writer_plan.required_bytes - 1,
        },
        &mut short_session,
    )
    .expect_err("one byte short must fail");
    assert!(error.contains("private storage limit"));
    assert!(short_output.is_empty());
    assert_eq!(short_session.remaining(), remaining_before);
    directory.assert_empty();
}

#[test]
fn encrypted_descriptor_corruption_fails_before_first_output_byte() {
    let directory = TestDirectory::new("encrypted-tamper");
    let mut sources = [TinySource::new(3), TinySource::new(2), TinySource::new(1)];
    let preflight = prepare_bounded_preflight(
        &directory.0,
        &mut sources,
        options(),
        ImmutableLimits::default(),
        spill_limits(3, 2),
    )
    .expect("preflight");
    let mut authority = DescriptorNonceAuthority::initial();
    let mut session = authority
        .activate_session([0x61; 32], [0x41; 4], [0x91; 16], 3, 3, true)
        .expect("session");
    let mut output = Vec::new();
    let error = write_prepared_encrypted_bounded_candidate_with_stage_hook(
        &mut output,
        &mut sources,
        &directory.0,
        EncryptedPreparedSettings {
            options: options(),
            limits: ImmutableLimits::default(),
        },
        preflight,
        &mut session,
        |stage| stage.flip_byte_for_test(20),
    )
    .expect_err("tampered descriptor must fail");
    assert!(error.contains("authentication"));
    assert!(output.is_empty());
    assert_eq!(session.remaining(), 0);
    directory.assert_empty();
}

#[test]
fn insufficient_nonce_lease_fails_before_encrypted_stage_creation() {
    let directory = TestDirectory::new("encrypted-short-lease");
    let mut sources = [
        TinySource::new(4),
        TinySource::new(3),
        TinySource::new(2),
        TinySource::new(1),
    ];
    let mut authority = DescriptorNonceAuthority::initial();
    let mut session = authority
        .activate_session([0x71; 32], [0x51; 4], [0xa1; 16], 3, 3, true)
        .expect("short lease session");
    let remaining_before = session.remaining();
    let mut output = Vec::new();
    let error = write_genesis_sources_end_to_end_encrypted_descriptor_candidate(
        &mut output,
        &mut sources,
        &directory.0,
        options(),
        ImmutableLimits::default(),
        spill_limits(4, 2),
        &mut session,
    )
    .expect_err("short lease must fail");
    assert!(error.contains("lease capacity"));
    assert!(output.is_empty());
    assert_eq!(session.remaining(), remaining_before);
    directory.assert_empty();
}

#[test]
fn non_durable_nonce_reservation_cannot_reach_encryption_session() {
    let mut authority = DescriptorNonceAuthority::initial();
    let result = authority.activate_session(
        [0x81; 32],
        [0x61; 4],
        [0xb1; 16],
        4,
        4,
        false,
    );
    assert!(result.is_err());
    assert_eq!(authority.next_unreserved(), Some(0));
}
