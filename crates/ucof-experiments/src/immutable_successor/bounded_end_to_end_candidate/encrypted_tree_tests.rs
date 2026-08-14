fn encrypted_tree_sessions(
    authority: &mut DescriptorNonceAuthority,
    key: [u8; 32],
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    objects: u64,
) -> (
    DescriptorEncryptionSession,
    DescriptorEncryptionSession,
    DescriptorEncryptionSession,
) {
    let object_count = usize::try_from(objects).expect("object count fits usize");
    let tree_records = encrypted_tree_stage_record_count(object_count).expect("tree record count");
    let spill = authority
        .activate_session(key, nonce_prefix, operation_id, objects, objects, true)
        .expect("spill lease");
    let retained = authority
        .activate_session(key, nonce_prefix, operation_id, objects, objects, true)
        .expect("retained lease");
    let tree = authority
        .activate_session(
            key,
            nonce_prefix,
            operation_id,
            tree_records,
            tree_records,
            true,
        )
        .expect("tree lease");
    (spill, retained, tree)
}

#[test]
fn encrypted_tree_stages_preserve_canonical_output_and_report() {
    const OBJECTS: u64 = 401;
    let limits = ImmutableLimits::default();
    let spill_limits = spill_limits(17, 3);
    let pipeline = EncryptedSorterPipelineSettings {
        options: options(),
        limits,
        spill_limits,
    };
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

    let directory = TestDirectory::new("encrypted-tree-equivalence");
    let mut authority_a = DescriptorNonceAuthority::initial();
    let (mut spill_a, mut retained_a, mut tree_a) = encrypted_tree_sessions(
        &mut authority_a,
        [0xd1; 32],
        [0x61; 4],
        [0x31; 16],
        OBJECTS,
    );
    let mut sources_a = original.clone();
    let mut output_a = Vec::new();
    let evidence_a = write_genesis_sources_end_to_end_encrypted_tree_candidate(
        &mut output_a,
        &mut sources_a,
        &directory.0,
        pipeline,
        &mut spill_a,
        &mut retained_a,
        &mut tree_a,
    )
    .expect("first encrypted tree writer");
    directory.assert_empty();

    let mut authority_b = DescriptorNonceAuthority::initial();
    let (mut spill_b, mut retained_b, mut tree_b) = encrypted_tree_sessions(
        &mut authority_b,
        [0xd1; 32],
        [0x62; 4],
        [0x31; 16],
        OBJECTS,
    );
    let mut sources_b = original;
    let mut output_b = Vec::new();
    let evidence_b = write_genesis_sources_end_to_end_encrypted_tree_candidate(
        &mut output_b,
        &mut sources_b,
        &directory.0,
        pipeline,
        &mut spill_b,
        &mut retained_b,
        &mut tree_b,
    )
    .expect("second encrypted tree writer");
    directory.assert_empty();

    assert_eq!(output_a, baseline);
    assert_eq!(output_b, baseline);
    assert_eq!(evidence_a.base.output, baseline_report);
    assert_eq!(evidence_b.base.output, baseline_report);
    assert_eq!(
        evidence_a.base.descriptor_stage_bytes,
        OBJECTS * u64::try_from(ENCRYPTED_DESCRIPTOR_STAGE_BYTES).expect("retained width")
    );
    assert_ne!(
        evidence_a.base.descriptor_spill.output_sha256,
        evidence_b.base.descriptor_spill.output_sha256
    );
    assert_ne!(
        evidence_a.base.descriptor_ciphertext_sha256,
        evidence_b.base.descriptor_ciphertext_sha256
    );
    assert_ne!(
        evidence_a.tree_stage_ciphertext_sha256,
        evidence_b.tree_stage_ciphertext_sha256
    );
    assert_eq!(spill_a.remaining(), 0);
    assert_eq!(retained_a.remaining(), 0);
    assert_eq!(tree_a.remaining(), 0);
    assert_eq!(spill_b.remaining(), 0);
    assert_eq!(retained_b.remaining(), 0);
    assert_eq!(tree_b.remaining(), 0);
    let tree_records = encrypted_tree_stage_record_count(
        usize::try_from(OBJECTS).expect("object count fits usize"),
    )
    .expect("tree record count");
    assert_eq!(
        authority_a.next_unreserved(),
        Some(OBJECTS * 2 + tree_records)
    );
    assert_eq!(
        authority_b.next_unreserved(),
        Some(OBJECTS * 2 + tree_records)
    );
}

#[test]
fn encrypted_tree_private_quota_prices_real_stage_lifetimes() {
    const OBJECTS: usize = 4;
    const RECORDS: u64 = 4;
    let mut spill_limits = spill_limits(8, 2);
    spill_limits.max_live_spill_bytes = RECORDS
        * u64::try_from(ENCRYPTED_SORTER_FRAME_BYTES).expect("sorter frame width");
    let plan = encrypted_tree_private_storage_plan(OBJECTS, spill_limits).expect("tree plan");
    assert_eq!(plan.encrypted_sorter_frame_bytes, 108);
    assert_eq!(plan.retained_descriptor_bytes, 368);
    assert_eq!(plan.encrypted_locator_bytes, 400);
    assert_eq!(plan.first_page_ref_bytes, 92);
    assert_eq!(plan.sorter_plus_retained_descriptor_bytes, 800);
    assert_eq!(plan.retained_descriptor_plus_locator_bytes, 768);
    assert_eq!(plan.locator_plus_leaf_ref_bytes, 492);
    assert_eq!(plan.max_adjacent_page_ref_bytes, 92);
    assert_eq!(plan.required_bytes, 800);

    let pipeline = EncryptedSorterPipelineSettings {
        options: options(),
        limits: ImmutableLimits::default(),
        spill_limits,
    };
    let original: Vec<_> = (1..=RECORDS).rev().map(TinySource::new).collect();
    let directory = TestDirectory::new("encrypted-tree-quota");

    let mut exact_authority = DescriptorNonceAuthority::initial();
    let (mut exact_spill, mut exact_retained, mut exact_tree) = encrypted_tree_sessions(
        &mut exact_authority,
        [0xe1; 32],
        [0x71; 4],
        [0x41; 16],
        RECORDS,
    );
    let mut exact_sources = original.clone();
    let mut exact_output = Vec::new();
    let (observed, evidence) = write_genesis_sources_with_encrypted_tree_private_quota_candidate(
        &mut exact_output,
        &mut exact_sources,
        &directory.0,
        EncryptedTreeWriterSettings {
            pipeline,
            max_private_storage_bytes: plan.required_bytes,
        },
        &mut exact_spill,
        &mut exact_retained,
        &mut exact_tree,
    )
    .expect("exact encrypted tree private quota");
    assert_eq!(observed, plan);
    assert!(!exact_output.is_empty());
    assert_eq!(evidence.base.peak_live_retained_stage_bytes, 768);
    assert_eq!(exact_spill.remaining(), 0);
    assert_eq!(exact_retained.remaining(), 0);
    assert_eq!(exact_tree.remaining(), 0);
    directory.assert_empty();

    let mut short_authority = DescriptorNonceAuthority::initial();
    let (mut short_spill, mut short_retained, mut short_tree) = encrypted_tree_sessions(
        &mut short_authority,
        [0xe1; 32],
        [0x72; 4],
        [0x42; 16],
        RECORDS,
    );
    let spill_remaining = short_spill.remaining();
    let retained_remaining = short_retained.remaining();
    let tree_remaining = short_tree.remaining();
    let mut short_sources = original;
    let mut short_output = Vec::new();
    let error = write_genesis_sources_with_encrypted_tree_private_quota_candidate(
        &mut short_output,
        &mut short_sources,
        &directory.0,
        EncryptedTreeWriterSettings {
            pipeline,
            max_private_storage_bytes: plan.required_bytes - 1,
        },
        &mut short_spill,
        &mut short_retained,
        &mut short_tree,
    )
    .expect_err("one byte short");
    assert!(error.contains("private storage limit"));
    assert!(short_output.is_empty());
    assert_eq!(short_spill.remaining(), spill_remaining);
    assert_eq!(short_retained.remaining(), retained_remaining);
    assert_eq!(short_tree.remaining(), tree_remaining);
    directory.assert_empty();
}

#[test]
fn short_tree_lease_fails_before_sorter_or_private_stage_creation() {
    const OBJECTS: u64 = 4;
    let object_count = usize::try_from(OBJECTS).expect("object count fits usize");
    let tree_records = encrypted_tree_stage_record_count(object_count).expect("tree record count");
    assert!(tree_records > 1);

    let directory = TestDirectory::new("encrypted-tree-short-lease");
    let mut sources: Vec<_> = (1..=OBJECTS).rev().map(TinySource::new).collect();
    let mut authority = DescriptorNonceAuthority::initial();
    let mut spill = authority
        .activate_session([0xf1; 32], [0x81; 4], [0x51; 16], OBJECTS, OBJECTS, true)
        .expect("spill lease");
    let mut retained = authority
        .activate_session([0xf1; 32], [0x81; 4], [0x51; 16], OBJECTS, OBJECTS, true)
        .expect("retained lease");
    let mut tree = authority
        .activate_session(
            [0xf1; 32],
            [0x81; 4],
            [0x51; 16],
            tree_records - 1,
            tree_records - 1,
            true,
        )
        .expect("short tree lease");
    let spill_remaining = spill.remaining();
    let retained_remaining = retained.remaining();
    let tree_remaining = tree.remaining();
    let mut output = Vec::new();
    let error = write_genesis_sources_end_to_end_encrypted_tree_candidate(
        &mut output,
        &mut sources,
        &directory.0,
        EncryptedSorterPipelineSettings {
            options: options(),
            limits: ImmutableLimits::default(),
            spill_limits: spill_limits(4, 2),
        },
        &mut spill,
        &mut retained,
        &mut tree,
    )
    .expect_err("short tree lease must fail");
    assert!(error.contains("tree nonce lease capacity"));
    assert!(output.is_empty());
    assert_eq!(spill.remaining(), spill_remaining);
    assert_eq!(retained.remaining(), retained_remaining);
    assert_eq!(tree.remaining(), tree_remaining);
    directory.assert_empty();
}

#[test]
fn encrypted_tree_stage_tamper_fails_authentication() {
    let directory = TestDirectory::new("encrypted-tree-tamper");
    let mut authority = DescriptorNonceAuthority::initial();
    let mut session = authority
        .activate_session([0x19; 32], [0x91; 4], [0x61; 16], 1, 1, true)
        .expect("tree stage lease");
    let mut stage = EncryptedRecordStage::create(
        &directory.0,
        "encrypted-tree-tamper-stage",
        LOCATOR_STAGE_BYTES,
        EncryptedTreeStageKind::Locator,
        0,
        &session,
    )
    .expect("create encrypted stage");
    {
        let mut writer = stage.writer(&mut session).expect("stage writer");
        writer
            .write_record(&[0xa5; LOCATOR_STAGE_BYTES])
            .expect("write encrypted locator-sized record");
        writer.finish().expect("finish encrypted stage");
    }
    assert_eq!(stage.bytes().expect("stage bytes"), 100);
    stage.verify_all(&session).expect("valid encrypted stage");
    stage.flip_byte_for_test(20).expect("tamper ciphertext");
    let error = stage
        .verify_all(&session)
        .expect_err("tampered tree stage must fail");
    assert!(error.contains("authentication"));
    assert_eq!(session.remaining(), 0);
    drop(stage);
    directory.assert_empty();
}
