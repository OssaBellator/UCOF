#[test]
fn consolidated_encrypted_tree_storage_prices_real_frame_lifetimes() {
    let plan = consolidated_encrypted_tree_storage_plan(4).expect("encrypted tree storage plan");
    assert_eq!(plan.retained_descriptor_bytes, 368);
    assert_eq!(plan.encrypted_locator_bytes, 400);
    assert_eq!(plan.first_page_ref_bytes, 92);
    assert_eq!(plan.retained_descriptor_plus_locator_bytes, 768);
    assert_eq!(plan.locator_plus_leaf_ref_bytes, 492);
    assert_eq!(plan.max_adjacent_page_ref_bytes, 92);
    assert_eq!(plan.required_post_preflight_bytes, 768);
}

#[test]
fn consolidated_tree_lifecycle_replaces_plain_tree_output_window_with_encrypted_widths() {
    let mut spill = super::spill_limits(8, 2);
    spill.max_live_spill_bytes = 10_000;
    let inventory = EncryptedPrivatePersistentInventory {
        nonce_journal_records: 2,
        retirement_records: 1,
    };
    let output_bytes = 750_000;
    let object_count = 401;

    let plain_normal = encrypted_normal_publication_storage_plan(
        object_count,
        output_bytes,
        spill,
        inventory,
    )
    .expect("plain-tree lifecycle plan");
    let (encrypted_normal, tree) = consolidated_encrypted_tree_normal_lifecycle_plan(
        object_count,
        output_bytes,
        spill,
        inventory,
    )
    .expect("encrypted-tree lifecycle plan");
    assert!(tree.required_post_preflight_bytes > 0);
    assert_eq!(
        encrypted_normal.output_window_bytes,
        encrypted_normal.persistent_after_lease_bytes
            + encrypted_normal.stage_manifest_bytes
            + encrypted_normal.durable_restart_stage_bytes
            + output_bytes
            + tree.required_post_preflight_bytes
    );
    assert!(encrypted_normal.output_window_bytes >= plain_normal.output_window_bytes);

    let plain_restart = encrypted_crash_resume_storage_plan(
        object_count,
        output_bytes,
        spill,
        inventory,
    )
    .expect("plain-tree restart plan");
    let (encrypted_restart, restart_tree) = consolidated_encrypted_tree_crash_resume_lifecycle_plan(
        object_count,
        output_bytes,
        spill,
        inventory,
    )
    .expect("encrypted-tree restart plan");
    assert_eq!(tree, restart_tree);
    assert_eq!(
        encrypted_restart.post_preflight_working_bytes,
        tree.required_post_preflight_bytes
    );
    assert!(encrypted_restart.output_window_bytes >= plain_restart.output_window_bytes);
}

#[test]
fn consolidated_tree_nonce_count_covers_locator_and_every_page_ref_stage() {
    let object_count = 401usize;
    let count = consolidated_encrypted_tree_stage_record_count(object_count)
        .expect("encrypted tree nonce count");
    let leaf_records = super::groups(
        object_count,
        super::LEAF_CAPACITY,
        super::LEAF_MIN_OCCUPANCY,
    )
    .expect("leaf groups")
    .len();
    let mut expected = u64::try_from(object_count).expect("object count");
    let mut current = leaf_records;
    loop {
        expected += u64::try_from(current).expect("page-ref count");
        if current == 1 {
            break;
        }
        current = super::groups(
            current,
            super::INTERNAL_FANOUT,
            super::INTERNAL_MIN_OCCUPANCY,
        )
        .expect("internal groups")
        .len();
    }
    assert_eq!(count, expected);
}
