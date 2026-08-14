fn consolidated_tree_sessions(
    authority: &mut DescriptorNonceAuthority,
    key: [u8; 32],
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    objects: u64,
) -> (DescriptorEncryptionSession, DescriptorEncryptionSession) {
    let object_count = usize::try_from(objects).expect("object count fits usize");
    let descriptor_nonces = objects.checked_mul(2).expect("descriptor nonce count");
    let tree_nonces =
        consolidated_encrypted_tree_stage_record_count(object_count).expect("tree nonce count");
    let descriptor = authority
        .activate_session(
            key,
            nonce_prefix,
            operation_id,
            descriptor_nonces,
            descriptor_nonces,
            true,
        )
        .expect("descriptor spill/retained lease");
    let tree = authority
        .activate_session(
            key,
            nonce_prefix,
            operation_id,
            tree_nonces,
            tree_nonces,
            true,
        )
        .expect("tree lease");
    (descriptor, tree)
}

#[test]
fn consolidated_encrypted_tree_pipeline_preserves_canonical_bytes_and_report() {
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

    let directory = super::TestDirectory::new("consolidated-encrypted-tree-equivalence");
    let mut authority_a = DescriptorNonceAuthority::initial();
    let (mut descriptor_a, mut tree_a) = consolidated_tree_sessions(
        &mut authority_a,
        [0xd4; 32],
        [0x64; 4],
        [0x34; 16],
        OBJECTS,
    );
    let mut sources_a = original.clone();
    let mut output_a = Vec::new();
    let evidence_a = write_genesis_sources_end_to_end_encrypted_tree_on_restart_spine(
        &mut output_a,
        &mut sources_a,
        &directory.0,
        super::options(),
        limits,
        spill,
        &mut descriptor_a,
        &mut tree_a,
    )
    .expect("first consolidated encrypted tree writer");
    directory.assert_empty();

    let mut authority_b = DescriptorNonceAuthority::initial();
    let (mut descriptor_b, mut tree_b) = consolidated_tree_sessions(
        &mut authority_b,
        [0xd4; 32],
        [0x65; 4],
        [0x34; 16],
        OBJECTS,
    );
    let mut sources_b = original;
    let mut output_b = Vec::new();
    let evidence_b = write_genesis_sources_end_to_end_encrypted_tree_on_restart_spine(
        &mut output_b,
        &mut sources_b,
        &directory.0,
        super::options(),
        limits,
        spill,
        &mut descriptor_b,
        &mut tree_b,
    )
    .expect("second consolidated encrypted tree writer");
    directory.assert_empty();

    assert_eq!(output_a, baseline);
    assert_eq!(output_b, baseline);
    assert_eq!(evidence_a.base.output, baseline_report);
    assert_eq!(evidence_b.base.output, baseline_report);
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
    assert_eq!(descriptor_a.remaining(), 0);
    assert_eq!(descriptor_b.remaining(), 0);
    assert_eq!(tree_a.remaining(), 0);
    assert_eq!(tree_b.remaining(), 0);
}

#[test]
fn consolidated_encrypted_tree_short_lease_fails_before_sorter_or_output() {
    const OBJECTS: u64 = 17;
    let object_count = usize::try_from(OBJECTS).expect("object count fits usize");
    let required_tree =
        consolidated_encrypted_tree_stage_record_count(object_count).expect("tree nonce count");
    assert!(required_tree > 1);
    let directory = super::TestDirectory::new("consolidated-encrypted-tree-short-lease");
    let mut sources: Vec<_> = (1..=OBJECTS).rev().map(super::TinySource::new).collect();
    let mut authority = DescriptorNonceAuthority::initial();
    let descriptor_nonces = OBJECTS.checked_mul(2).expect("descriptor nonce count");
    let mut descriptor = authority
        .activate_session(
            [0xe4; 32],
            [0x74; 4],
            [0x44; 16],
            descriptor_nonces,
            descriptor_nonces,
            true,
        )
        .expect("descriptor lease");
    let mut tree = authority
        .activate_session(
            [0xe4; 32],
            [0x74; 4],
            [0x44; 16],
            required_tree - 1,
            required_tree - 1,
            true,
        )
        .expect("short tree lease");
    let descriptor_remaining = descriptor.remaining();
    let tree_remaining = tree.remaining();
    let mut output = Vec::new();
    let error = write_genesis_sources_end_to_end_encrypted_tree_on_restart_spine(
        &mut output,
        &mut sources,
        &directory.0,
        super::options(),
        super::ImmutableLimits::default(),
        super::spill_limits(7, 2),
        &mut descriptor,
        &mut tree,
    )
    .expect_err("short tree lease must fail before private work");
    assert!(error.contains("tree nonce lease capacity"));
    assert!(output.is_empty());
    assert_eq!(descriptor.remaining(), descriptor_remaining);
    assert_eq!(tree.remaining(), tree_remaining);
    directory.assert_empty();
}
