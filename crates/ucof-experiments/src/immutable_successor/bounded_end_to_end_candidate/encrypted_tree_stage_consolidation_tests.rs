#[test]
fn encrypted_tree_stage_adapter_round_trips_locator_and_page_ref_frames() {
    let directory = super::TestDirectory::new("encrypted-tree-stage-consolidation");
    let mut authority = DescriptorNonceAuthority::initial();
    let mut session = authority
        .activate_session([0x31; 32], [0xa1; 4], [0xb1; 16], 2, 2, true)
        .expect("tree stage lease");

    let locator_plaintext = [0x51; super::LOCATOR_STAGE_BYTES];
    let mut locator = EncryptedRecordStage::create(
        &directory.0,
        "consolidated-encrypted-locator",
        super::LOCATOR_STAGE_BYTES,
        EncryptedTreeStageKind::Locator,
        0,
        &session,
    )
    .expect("create encrypted locator stage");
    {
        let mut writer = locator.writer(&mut session).expect("locator writer");
        writer
            .write_record(&locator_plaintext)
            .expect("write encrypted locator");
        writer.finish().expect("finish locator stage");
    }
    assert_eq!(locator.records(), 1);
    assert_eq!(
        locator.bytes().expect("locator bytes"),
        u64::try_from(ENCRYPTED_LOCATOR_STAGE_BYTES).expect("locator frame width")
    );
    locator.verify_all(&session).expect("verify locator stage");
    let mut locator_reader = locator.reader(&session).expect("locator reader");
    assert_eq!(
        locator_reader.read_record().expect("read locator"),
        locator_plaintext
    );
    locator_reader.finish().expect("finish locator read");
    drop(locator_reader);

    let page_ref_plaintext = [0x72; super::PAGE_REF_STAGE_BYTES];
    let mut page_ref = EncryptedRecordStage::create(
        &directory.0,
        "consolidated-encrypted-page-ref",
        super::PAGE_REF_STAGE_BYTES,
        EncryptedTreeStageKind::PageRef,
        1,
        &session,
    )
    .expect("create encrypted page-ref stage");
    {
        let mut writer = page_ref.writer(&mut session).expect("page-ref writer");
        writer
            .write_record(&page_ref_plaintext)
            .expect("write encrypted page ref");
        writer.finish().expect("finish page-ref stage");
    }
    assert_eq!(page_ref.records(), 1);
    assert_eq!(
        page_ref.bytes().expect("page-ref bytes"),
        u64::try_from(ENCRYPTED_PAGE_REF_STAGE_BYTES).expect("page-ref frame width")
    );
    page_ref.verify_all(&session).expect("verify page-ref stage");
    let mut page_ref_reader = page_ref.reader(&session).expect("page-ref reader");
    assert_eq!(
        page_ref_reader.read_record().expect("read page ref"),
        page_ref_plaintext
    );
    page_ref_reader.finish().expect("finish page-ref read");
    assert_eq!(session.remaining(), 0);

    drop(page_ref_reader);
    drop(page_ref);
    drop(locator);
    directory.assert_empty();
}

#[test]
fn encrypted_tree_stage_adapter_rejects_tamper_and_foreign_session() {
    let directory = super::TestDirectory::new("encrypted-tree-stage-auth");
    let mut authority = DescriptorNonceAuthority::initial();
    let mut session = authority
        .activate_session([0x41; 32], [0xa2; 4], [0xb2; 16], 1, 1, true)
        .expect("tree stage lease");
    let mut stage = EncryptedRecordStage::create(
        &directory.0,
        "consolidated-encrypted-tree-auth",
        super::LOCATOR_STAGE_BYTES,
        EncryptedTreeStageKind::Locator,
        7,
        &session,
    )
    .expect("create encrypted stage");
    {
        let mut writer = stage.writer(&mut session).expect("tree writer");
        writer
            .write_record(&[0x83; super::LOCATOR_STAGE_BYTES])
            .expect("write tree record");
        writer.finish().expect("finish tree stage");
    }
    stage.verify_all(&session).expect("valid encrypted stage");

    let mut foreign_authority = DescriptorNonceAuthority::initial();
    let foreign = foreign_authority
        .activate_session([0x41; 32], [0xa3; 4], [0xb2; 16], 1, 1, true)
        .expect("foreign tree stage lease");
    assert!(stage
        .verify_all(&foreign)
        .expect_err("foreign nonce namespace must fail")
        .contains("session mismatch"));

    stage.flip_byte_for_test(20).expect("tamper encrypted tree frame");
    assert!(stage
        .verify_all(&session)
        .expect_err("tampered tree stage must fail")
        .contains("authentication"));
    assert_eq!(session.remaining(), 0);
    drop(stage);
    directory.assert_empty();
}

#[test]
fn encrypted_tree_stage_frame_widths_match_confidentiality_branch_evidence() {
    assert_eq!(ENCRYPTED_LOCATOR_STAGE_BYTES, 100);
    assert_eq!(ENCRYPTED_PAGE_REF_STAGE_BYTES, 92);
    assert_eq!(
        encrypted_tree_frame_bytes(super::LOCATOR_STAGE_BYTES).expect("locator frame"),
        ENCRYPTED_LOCATOR_STAGE_BYTES
    );
    assert_eq!(
        encrypted_tree_frame_bytes(super::PAGE_REF_STAGE_BYTES).expect("page-ref frame"),
        ENCRYPTED_PAGE_REF_STAGE_BYTES
    );
}
