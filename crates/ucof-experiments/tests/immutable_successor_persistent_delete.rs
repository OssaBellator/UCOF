use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_delete, append_persistent_delete_experimental,
    append_persistent_insert, build_genesis, rewrite_all, validate_canonical_occupancy,
    ExperimentalDeleteBorrowPolicy, ImmutableBatchOperation, ImmutableError, ImmutableLimits,
    ImmutableObjectInput, PersistentBatchMode, INTERNAL_FANOUT, INTERNAL_MIN_OCCUPANCY,
    LEAF_CAPACITY, LEAF_MIN_OCCUPANCY,
};

fn objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(|object_id| ImmutableObjectInput::new(object_id, 1, vec![object_id as u8]))
        .collect()
}

#[test]
fn root_leaf_deletion_rewrites_only_the_root() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(10), limits).expect("genesis");
    let result = append_persistent_delete(&genesis, 5, limits).expect("delete");
    assert_eq!(result.mode, PersistentBatchMode::CopyOnWriteDeletion);
    assert_eq!(result.report.sequence, 1);
    assert_eq!(result.report.object_count, 9);
    assert_eq!(result.report.root_level, 0);
    assert_eq!(result.pages_written, 1);
    assert_eq!(result.pages_reused, 0);
    assert_eq!(
        validate_canonical_occupancy(&result.bytes, limits).expect("canonical"),
        result.report
    );

    let general = append_persistent_batch(&genesis, &[ImmutableBatchOperation::Delete(5)], limits)
        .expect("general deletion");
    assert_eq!(general, result);
}

#[test]
fn deletion_without_underflow_reuses_unrelated_pages() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(400), limits).expect("genesis");
    let result = append_persistent_delete(&genesis, 10, limits).expect("delete");
    assert_eq!(result.report.object_count, 399);
    assert_eq!(result.report.root_level, 1);
    assert_eq!(result.report.page_count, 4);
    assert_eq!(result.pages_written, 2);
    assert_eq!(result.pages_reused, 2);
}

#[test]
fn underflow_borrows_from_left_before_other_repairs() {
    let limits = ImmutableLimits::default();
    let count = LEAF_CAPACITY + 2;
    assert_eq!(count, 187);
    let genesis = build_genesis(&objects(count), limits).expect("94-93 leaves");
    let result = append_persistent_delete(&genesis, u64::try_from(count).expect("count"), limits)
        .expect("left borrow");
    assert_eq!(result.report.root_level, 1);
    assert_eq!(result.report.page_count, 3);
    assert_eq!(result.pages_written, 3);
    assert_eq!(result.pages_reused, 0);
    assert_eq!(result.report.object_count, count - 1);
}

#[test]
fn leftmost_underflow_borrows_from_right() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(2 * LEAF_MIN_OCCUPANCY), limits).expect("93-93 leaves");
    let inserted = append_persistent_insert(
        &genesis,
        &ImmutableObjectInput::new(10_000, 1, b"right".to_vec()),
        limits,
    )
    .expect("make right sibling larger");
    let result = append_persistent_delete(&inserted.bytes, 1, limits).expect("right borrow");
    assert_eq!(result.report.root_level, 1);
    assert_eq!(result.report.page_count, 3);
    assert_eq!(result.pages_written, 3);
    assert_eq!(result.pages_reused, 0);
    assert_eq!(result.report.object_count, 2 * LEAF_MIN_OCCUPANCY);
}

#[test]
fn minimum_siblings_merge_and_collapse_the_root() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(2 * LEAF_MIN_OCCUPANCY), limits).expect("93-93 leaves");
    let result = append_persistent_delete(&genesis, 1, limits).expect("merge and collapse");
    assert_eq!(result.report.root_level, 0);
    assert_eq!(result.report.page_count, 1);
    assert_eq!(result.report.object_count, 2 * LEAF_MIN_OCCUPANCY - 1);
    assert_eq!(result.pages_written, 1);
    assert_eq!(result.pages_reused, 0);
}

#[test]
fn recursive_internal_underflow_borrows_at_level_two() {
    let limits = ImmutableLimits::default();
    let full_prefix_leaves = INTERNAL_FANOUT;
    let count = full_prefix_leaves
        .checked_mul(LEAF_CAPACITY)
        .and_then(|value| value.checked_add(2 * LEAF_MIN_OCCUPANCY))
        .expect("modeled object count");
    let genesis = build_genesis(&objects(count), limits).expect("129-128 internal children");
    let original = validate_canonical_occupancy(&genesis, limits).expect("canonical level two");
    assert_eq!(original.root_level, 2);
    assert_eq!(INTERNAL_MIN_OCCUPANCY, 128);

    let result = append_persistent_delete(&genesis, u64::try_from(count).expect("count"), limits)
        .expect("recursive internal borrow");
    assert_eq!(result.report.root_level, 2);
    assert_eq!(result.report.object_count, count - 1);
    assert_eq!(result.report.page_count, original.page_count - 1);
    assert_eq!(result.pages_written, 4);
    assert_eq!(result.pages_reused, original.page_count - 5);
}

#[test]
fn experimental_fuller_sibling_changes_transition_bytes_not_logical_state() {
    let limits = ImmutableLimits::default();
    assert_eq!(LEAF_CAPACITY, 185);
    assert_eq!(LEAF_MIN_OCCUPANCY, 93);

    // Start with two full leaves. Inserting 371 splits the right leaf into 93/93,
    // giving three leaves [185, 93, 93]. Grow the right leaf to 101 and shrink the
    // left leaf to 94 without causing either to underflow. Deleting the first key
    // from the middle leaf then leaves the target at 92 with both siblings eligible:
    // left=94, right=101.
    let mut state = build_genesis(&objects(2 * LEAF_CAPACITY), limits).expect("two full leaves");
    for object_id in u64::try_from(2 * LEAF_CAPACITY + 1).expect("first insertion")..=379 {
        state = append_persistent_insert(
            &state,
            &ImmutableObjectInput::new(object_id, 1, vec![object_id as u8]),
            limits,
        )
        .expect("grow right sibling")
        .bytes;
    }
    let left_deletions = LEAF_CAPACITY - (LEAF_MIN_OCCUPANCY + 1);
    assert_eq!(left_deletions, 91);
    for object_id in 1..=u64::try_from(left_deletions).expect("left deletions") {
        state = append_persistent_delete(&state, object_id, limits)
            .expect("shrink left sibling")
            .bytes;
    }

    let target = u64::try_from(LEAF_CAPACITY + 1).expect("middle first id");
    assert_eq!(target, 186);
    let default = append_persistent_delete(&state, target, limits).expect("default left-first");
    let explicit_left = append_persistent_delete_experimental(
        &state,
        target,
        limits,
        ExperimentalDeleteBorrowPolicy::LeftFirst,
    )
    .expect("explicit left-first");
    assert_eq!(default, explicit_left, "default bytes must remain left-first");

    let fuller = append_persistent_delete_experimental(
        &state,
        target,
        limits,
        ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
    )
    .expect("fuller sibling");

    assert_ne!(default.bytes, fuller.bytes, "borrow direction must affect bytes");
    assert_ne!(
        default.report.snapshot_digest, fuller.report.snapshot_digest,
        "borrow direction must affect authenticated root/snapshot identity"
    );
    assert_eq!(default.report.object_count, fuller.report.object_count);
    assert_eq!(default.report.page_count, fuller.report.page_count);
    assert_eq!(default.report.root_level, fuller.report.root_level);
    assert_eq!(default.pages_written, fuller.pages_written);
    assert_eq!(default.pages_reused, fuller.pages_reused);
    assert_eq!(
        validate_canonical_occupancy(&default.bytes, limits).expect("left-first canonical"),
        default.report
    );
    assert_eq!(
        validate_canonical_occupancy(&fuller.bytes, limits).expect("fuller canonical"),
        fuller.report
    );

    // Canonical fresh rewrite removes the transition-history difference. Equal
    // canonical bytes prove both persistent paths expose the same active objects and
    // payloads even though their authenticated persistent roots differ.
    let default_fresh = rewrite_all(&default.bytes, limits).expect("rewrite left-first");
    let fuller_fresh = rewrite_all(&fuller.bytes, limits).expect("rewrite fuller");
    assert_eq!(default_fresh.retained_object_ids, fuller_fresh.retained_object_ids);
    assert_eq!(default_fresh.bytes, fuller_fresh.bytes);
}

#[test]
fn deletion_is_deterministic_and_rejects_invalid_requests() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(400), limits).expect("genesis");
    assert_eq!(
        append_persistent_delete(&genesis, 200, limits)
            .expect("first")
            .bytes,
        append_persistent_delete(&genesis, 200, limits)
            .expect("second")
            .bytes
    );
    assert_eq!(
        append_persistent_delete(&genesis, 999, limits),
        Err(ImmutableError::MissingObject(999))
    );
    let one = build_genesis(&objects(1), limits).expect("one object");
    assert_eq!(
        append_persistent_delete(&one, 1, limits),
        Err(ImmutableError::Invalid("batch result"))
    );
}
