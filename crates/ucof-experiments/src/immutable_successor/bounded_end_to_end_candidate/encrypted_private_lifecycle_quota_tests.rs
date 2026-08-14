#[test]
fn unified_encrypted_private_storage_plan_prices_every_lifecycle_window() {
    let mut spill = super::spill_limits(31, 4);
    spill.max_live_spill_bytes = 100_000;
    let inventory = EncryptedPrivatePersistentInventory {
        nonce_journal_records: 2,
        retirement_records: 3,
    };
    let output_bytes = 1_000_000;
    let object_count = 2_003;
    let plan = unified_encrypted_private_storage_plan(
        object_count,
        output_bytes,
        spill,
        inventory,
    )
    .expect("unified encrypted storage plan");
    let working = encrypted_spill_private_storage_plan(object_count, spill)
        .expect("encrypted working plan");
    let journal_bytes = u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width");
    let retirement_bytes = u64::try_from(ENCRYPTED_RETIREMENT_BYTES).expect("retirement width");
    let manifest_bytes =
        u64::try_from(ENCRYPTED_STAGE_MANIFEST_BYTES).expect("manifest width");
    let normal_persistent = 3 * journal_bytes + 3 * retirement_bytes;
    let post_working = working
        .retained_plus_locator_bytes
        .max(working.locator_plus_leaf_ref_bytes)
        .max(working.max_adjacent_page_ref_bytes);

    assert_eq!(plan.normal.working, working);
    assert_eq!(plan.normal.output_bytes, output_bytes);
    assert_eq!(plan.normal.persistent_after_lease_bytes, normal_persistent);
    assert_eq!(
        plan.normal.durable_restart_stage_bytes,
        working.encrypted_spill_descriptor_bytes
    );
    assert_eq!(plan.normal.stage_manifest_bytes, manifest_bytes);
    assert_eq!(
        plan.normal.sort_window_bytes,
        normal_persistent + working.sorter_plus_encrypted_spill_bytes
    );
    assert_eq!(
        plan.normal.restart_copy_window_bytes,
        normal_persistent + 2 * working.encrypted_spill_descriptor_bytes
    );
    assert_eq!(
        plan.normal.restart_manifest_window_bytes,
        normal_persistent + 2 * working.encrypted_spill_descriptor_bytes + manifest_bytes
    );
    assert_eq!(
        plan.normal.restart_transcode_window_bytes,
        normal_persistent
            + manifest_bytes
            + working.encrypted_spill_descriptor_bytes
            + working.encrypted_spill_plus_retained_bytes
    );
    assert_eq!(
        plan.normal.output_window_bytes,
        normal_persistent
            + manifest_bytes
            + working.encrypted_spill_descriptor_bytes
            + output_bytes
            + post_working
    );

    let restart_before_persistent = 3 * journal_bytes + 3 * retirement_bytes;
    let restart_after_persistent = 4 * journal_bytes + 3 * retirement_bytes;
    assert_eq!(
        plan.crash_resume.persistent_before_fresh_lease_bytes,
        restart_before_persistent
    );
    assert_eq!(
        plan.crash_resume.persistent_after_fresh_lease_bytes,
        restart_after_persistent
    );
    assert_eq!(
        plan.crash_resume.restart_transcode_window_bytes,
        restart_after_persistent
            + working.encrypted_spill_descriptor_bytes
            + manifest_bytes
            + working.retained_encrypted_descriptor_bytes
    );
    assert_eq!(
        plan.crash_resume.output_window_bytes,
        restart_after_persistent
            + working.encrypted_spill_descriptor_bytes
            + manifest_bytes
            + output_bytes
            + post_working
    );
    assert_eq!(
        plan.crash_resume.retirement_prepared_window_bytes,
        4 * journal_bytes
            + 4 * retirement_bytes
            + working.encrypted_spill_descriptor_bytes
            + manifest_bytes
            + output_bytes
    );
    assert_eq!(
        plan.crash_resume.retirement_terminal_window_bytes,
        4 * journal_bytes + 5 * retirement_bytes + output_bytes
    );
    assert_eq!(
        plan.required_bytes,
        plan.normal.required_bytes.max(plan.crash_resume.required_bytes)
    );
}

#[test]
fn unified_encrypted_private_storage_exact_cap_succeeds_and_one_byte_short_fails() {
    let spill = super::spill_limits(17, 3);
    let inventory = EncryptedPrivatePersistentInventory::default();
    let plan = unified_encrypted_private_storage_plan(401, 2_000_000, spill, inventory)
        .expect("unified encrypted storage plan");

    assert_eq!(
        enforce_unified_encrypted_private_storage_limit(
            401,
            2_000_000,
            spill,
            inventory,
            plan.required_bytes,
        )
        .expect("exact unified quota"),
        plan
    );
    let error = enforce_unified_encrypted_private_storage_limit(
        401,
        2_000_000,
        spill,
        inventory,
        plan.required_bytes - 1,
    )
    .expect_err("one-byte-short unified quota must fail");
    assert!(error.contains("unified encrypted private storage limit"));
}

#[test]
fn crash_resume_plan_budgets_one_fresh_nonce_record_and_two_retirement_records() {
    let spill = super::spill_limits(7, 2);
    let inventory = EncryptedPrivatePersistentInventory {
        nonce_journal_records: 9,
        retirement_records: 11,
    };
    let plan = encrypted_crash_resume_storage_plan(17, 500_000, spill, inventory)
        .expect("crash-resume storage plan");
    let journal_bytes = u64::try_from(LINUX_NONCE_JOURNAL_BYTES).expect("journal width");
    let retirement_bytes = u64::try_from(ENCRYPTED_RETIREMENT_BYTES).expect("retirement width");

    assert_eq!(
        plan.persistent_before_fresh_lease_bytes,
        9 * journal_bytes + 11 * retirement_bytes
    );
    assert_eq!(
        plan.persistent_after_fresh_lease_bytes,
        10 * journal_bytes + 11 * retirement_bytes
    );
    assert_eq!(
        plan.retirement_terminal_window_bytes,
        10 * journal_bytes + 13 * retirement_bytes + 500_000
    );
}

#[test]
fn unified_encrypted_private_storage_rejects_zero_output_and_overflowing_inventory() {
    let spill = super::spill_limits(7, 2);
    assert!(unified_encrypted_private_storage_plan(
        17,
        0,
        spill,
        EncryptedPrivatePersistentInventory::default(),
    )
    .expect_err("zero output must fail")
    .contains("output bytes"));

    let error = unified_encrypted_private_storage_plan(
        17,
        1,
        spill,
        EncryptedPrivatePersistentInventory {
            nonce_journal_records: usize::MAX,
            retirement_records: 0,
        },
    )
    .expect_err("overflowing persistent inventory must fail");
    assert!(error.contains("overflow") || error.contains("record"));
}
