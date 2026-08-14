#[test]
fn encrypted_spill_writer_preserves_canonical_bytes_and_reports() {
    const OBJECTS: u64 = 401;
    let limits = super::ImmutableLimits::default();
    let spill = super::spill_limits(17, 3);
    let original: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();

    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = super::write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        super::options(),
        limits,
    )
    .expect("baseline writer");

    let directory = super::TestDirectory::new("encrypted-spill-equivalence");
    let lease_size = OBJECTS.checked_mul(2).expect("nonce uses");

    let mut authority_a = DescriptorNonceAuthority::initial();
    let mut session_a = authority_a
        .activate_session(
            [0x91; 32],
            [0x71; 4],
            [0xc1; 16],
            lease_size,
            lease_size,
            true,
        )
        .expect("first encrypted spill lease");
    let mut sources_a = original.clone();
    let mut output_a = Vec::new();
    let evidence_a = write_genesis_sources_end_to_end_encrypted_spill_candidate(
        &mut output_a,
        &mut sources_a,
        &directory.0,
        super::options(),
        limits,
        spill,
        &mut session_a,
    )
    .expect("first encrypted spill writer");

    let mut authority_b = DescriptorNonceAuthority::initial();
    let mut session_b = authority_b
        .activate_session(
            [0x91; 32],
            [0x72; 4],
            [0xc1; 16],
            lease_size,
            lease_size,
            true,
        )
        .expect("second encrypted spill lease");
    let mut sources_b = original;
    let mut output_b = Vec::new();
    let evidence_b = write_genesis_sources_end_to_end_encrypted_spill_candidate(
        &mut output_b,
        &mut sources_b,
        &directory.0,
        super::options(),
        limits,
        spill,
        &mut session_b,
    )
    .expect("second encrypted spill writer");

    assert_eq!(output_a, baseline);
    assert_eq!(output_b, baseline);
    assert_eq!(evidence_a.output.output, baseline_report);
    assert_eq!(evidence_b.output.output, baseline_report);
    assert_eq!(
        evidence_a.sorted_spill_stage_bytes,
        OBJECTS * u64::try_from(ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES).expect("spill width")
    );
    assert_eq!(
        evidence_a.output.descriptor_stage_bytes,
        OBJECTS * u64::try_from(ENCRYPTED_DESCRIPTOR_STAGE_BYTES).expect("retained width")
    );
    assert_eq!(
        evidence_a.output.descriptor_spill.output_payload_bytes,
        evidence_a.sorted_spill_stage_bytes
    );
    assert_ne!(
        evidence_a.sorted_spill_ciphertext_sha256,
        evidence_b.sorted_spill_ciphertext_sha256
    );
    assert_ne!(
        evidence_a.output.descriptor_ciphertext_sha256,
        evidence_b.output.descriptor_ciphertext_sha256
    );
    assert_eq!(session_a.remaining(), 0);
    assert_eq!(session_b.remaining(), 0);
    assert_eq!(authority_a.next_unreserved(), Some(lease_size));
    assert_eq!(authority_b.next_unreserved(), Some(lease_size));
    directory.assert_empty();
}

#[test]
fn encrypted_spill_quota_prices_sort_transcode_and_emission_overlaps() {
    const OBJECTS: usize = 4;
    let mut spill = super::spill_limits(8, 2);
    spill.max_live_spill_bytes = 432;
    let plan = encrypted_spill_private_storage_plan(OBJECTS, spill).expect("encrypted spill plan");
    assert_eq!(plan.encrypted_spill_descriptor_bytes, 400);
    assert_eq!(plan.retained_encrypted_descriptor_bytes, 368);
    assert_eq!(plan.locator_bytes, 288);
    assert_eq!(plan.leaf_ref_bytes, 64);
    assert_eq!(plan.sorter_plus_encrypted_spill_bytes, 832);
    assert_eq!(plan.encrypted_spill_plus_retained_bytes, 768);
    assert_eq!(plan.retained_plus_locator_bytes, 656);
    assert_eq!(plan.locator_plus_leaf_ref_bytes, 352);
    assert_eq!(plan.max_adjacent_page_ref_bytes, 64);
    assert_eq!(plan.required_bytes, 832);

    let original: Vec<_> = (1..=u64::try_from(OBJECTS).unwrap())
        .rev()
        .map(super::TinySource::new)
        .collect();
    let directory = super::TestDirectory::new("encrypted-spill-quota");

    let mut exact_authority = DescriptorNonceAuthority::initial();
    let mut exact_session = exact_authority
        .activate_session([0xa1; 32], [0x81; 4], [0xd1; 16], 8, 8, true)
        .expect("exact spill session");
    let mut exact_sources = original.clone();
    let mut exact_output = Vec::new();
    let (observed, evidence) = write_genesis_sources_with_encrypted_spill_private_quota_candidate(
        &mut exact_output,
        &mut exact_sources,
        &directory.0,
        super::options(),
        super::ImmutableLimits::default(),
        spill,
        plan.required_bytes,
        &mut exact_session,
    )
    .expect("exact encrypted spill private quota");
    assert_eq!(observed, plan);
    assert!(!exact_output.is_empty());
    assert_eq!(evidence.sorted_spill_stage_bytes, 400);
    assert_eq!(exact_session.remaining(), 0);
    directory.assert_empty();

    let mut short_authority = DescriptorNonceAuthority::initial();
    let mut short_session = short_authority
        .activate_session([0xa1; 32], [0x82; 4], [0xd2; 16], 8, 8, true)
        .expect("short spill session");
    let remaining_before = short_session.remaining();
    let mut short_sources = original;
    let mut short_output = Vec::new();
    let error = write_genesis_sources_with_encrypted_spill_private_quota_candidate(
        &mut short_output,
        &mut short_sources,
        &directory.0,
        super::options(),
        super::ImmutableLimits::default(),
        spill,
        plan.required_bytes - 1,
        &mut short_session,
    )
    .expect_err("one byte short must fail");
    assert!(error.contains("private storage limit"));
    assert!(short_output.is_empty());
    assert_eq!(short_session.remaining(), remaining_before);
    directory.assert_empty();
}

fn prepared_three_object_encrypted_spill(
    label: &str,
) -> (
    super::TestDirectory,
    [super::TinySource; 3],
    DescriptorNonceAuthority,
    DescriptorEncryptionSession,
    EncryptedSpillPreflight,
) {
    let directory = super::TestDirectory::new(label);
    let mut sources = [
        super::TinySource::new(3),
        super::TinySource::new(2),
        super::TinySource::new(1),
    ];
    let mut authority = DescriptorNonceAuthority::initial();
    let mut session = authority
        .activate_session([0xb1; 32], [0x91; 4], [0xe1; 16], 6, 6, true)
        .expect("spill mutation session");
    let preflight = prepare_encrypted_spill_preflight(
        &directory.0,
        &mut sources,
        super::options(),
        super::ImmutableLimits::default(),
        super::spill_limits(3, 2),
        &mut session,
    )
    .expect("encrypted spill preflight");
    assert_eq!(session.remaining(), 3);
    (directory, sources, authority, session, preflight)
}

#[test]
fn encrypted_spill_ciphertext_tamper_fails_before_first_output_byte() {
    let (directory, mut sources, _authority, mut session, preflight) =
        prepared_three_object_encrypted_spill("encrypted-spill-tamper");
    let mut output = Vec::new();
    let error = write_prepared_encrypted_spill_candidate_with_stage_hook(
        &mut output,
        &mut sources,
        &directory.0,
        EncryptedSpillPreparedSettings {
            options: super::options(),
            limits: super::ImmutableLimits::default(),
        },
        preflight,
        &mut session,
        |stage| {
            let file = stage
                .file
                .as_mut()
                .ok_or_else(|| "closed encrypted spill stage".to_owned())?;
            file.seek(std::io::SeekFrom::Start(20))
                .map_err(|error| error.to_string())?;
            let mut byte = [0u8; 1];
            file.read_exact(&mut byte)
                .map_err(|error| error.to_string())?;
            byte[0] ^= 0x80;
            file.seek(std::io::SeekFrom::Start(20))
                .map_err(|error| error.to_string())?;
            file.write_all(&byte).map_err(|error| error.to_string())?;
            file.flush().map_err(|error| error.to_string())?;
            Ok(())
        },
    )
    .expect_err("spill ciphertext tamper must fail");
    assert!(error.contains("authentication"));
    assert!(output.is_empty());
    assert_eq!(session.remaining(), 3);
    directory.assert_empty();
}

#[test]
fn encrypted_spill_record_reorder_fails_before_first_output_byte() {
    let (directory, mut sources, _authority, mut session, preflight) =
        prepared_three_object_encrypted_spill("encrypted-spill-reorder");
    let mut output = Vec::new();
    let error = write_prepared_encrypted_spill_candidate_with_stage_hook(
        &mut output,
        &mut sources,
        &directory.0,
        EncryptedSpillPreparedSettings {
            options: super::options(),
            limits: super::ImmutableLimits::default(),
        },
        preflight,
        &mut session,
        |stage| {
            let file = stage
                .file
                .as_mut()
                .ok_or_else(|| "closed encrypted spill stage".to_owned())?;
            let mut first = [0u8; ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES];
            let mut second = [0u8; ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES];
            file.seek(std::io::SeekFrom::Start(0))
                .map_err(|error| error.to_string())?;
            file.read_exact(&mut first)
                .and_then(|_| file.read_exact(&mut second))
                .map_err(|error| error.to_string())?;
            file.seek(std::io::SeekFrom::Start(0))
                .map_err(|error| error.to_string())?;
            file.write_all(&second)
                .and_then(|_| file.write_all(&first))
                .map_err(|error| error.to_string())?;
            file.flush().map_err(|error| error.to_string())?;
            Ok(())
        },
    )
    .expect_err("spill record reorder must fail");
    assert!(error.contains("ordering"));
    assert!(output.is_empty());
    assert_eq!(session.remaining(), 2);
    directory.assert_empty();
}

#[test]
fn encrypted_spill_truncation_fails_before_first_output_byte() {
    let (directory, mut sources, _authority, mut session, preflight) =
        prepared_three_object_encrypted_spill("encrypted-spill-truncate");
    let mut output = Vec::new();
    let error = write_prepared_encrypted_spill_candidate_with_stage_hook(
        &mut output,
        &mut sources,
        &directory.0,
        EncryptedSpillPreparedSettings {
            options: super::options(),
            limits: super::ImmutableLimits::default(),
        },
        preflight,
        &mut session,
        |stage| {
            let file = stage
                .file
                .as_mut()
                .ok_or_else(|| "closed encrypted spill stage".to_owned())?;
            let length = file.metadata().map_err(|error| error.to_string())?.len();
            file.set_len(length - 1).map_err(|error| error.to_string())?;
            Ok(())
        },
    )
    .expect_err("spill truncation must fail");
    assert!(error.contains("stage byte length"));
    assert!(output.is_empty());
    assert_eq!(session.remaining(), 3);
    directory.assert_empty();
}
