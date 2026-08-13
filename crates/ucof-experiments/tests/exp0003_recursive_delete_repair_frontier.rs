use ucof_experiments::immutable_successor::{
    append_persistent_delete_experimental, build_genesis,
    inspect_persistent_delete_leaf_frontier_experimental,
    inspect_persistent_delete_repair_path_experimental, validate_canonical_occupancy,
    ExperimentalDeleteBorrowDirection, ExperimentalDeleteBorrowPolicy, ImmutableError,
    ImmutableLimits, ImmutableObjectInput, INTERNAL_FANOUT, INTERNAL_MIN_OCCUPANCY, LEAF_CAPACITY,
    LEAF_MIN_OCCUPANCY,
};

fn objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(|object_id| ImmutableObjectInput::new(object_id, 1, vec![object_id as u8]))
        .collect()
}

#[test]
fn recursive_leaf_merge_propagates_to_internal_donor_cliff_borrow() {
    let limits = ImmutableLimits::default();
    assert_eq!(LEAF_CAPACITY, 185);
    assert_eq!(LEAF_MIN_OCCUPANCY, 93);
    assert_eq!(INTERNAL_FANOUT, 255);
    assert_eq!(INTERNAL_MIN_OCCUPANCY, 128);

    // Canonical grouping produces two level-1 children with 129 and 128 leaf
    // children. The final two leaves are both at the leaf minimum. Deleting the
    // final object therefore merges the final leaf, taking the right level-1
    // child from 128 to 127. The left level-1 sibling has 129 children and must
    // lend one, so the recursive repair itself lands on the internal M+1 donor cliff.
    let count = INTERNAL_FANOUT
        .checked_mul(LEAF_CAPACITY)
        .and_then(|value| value.checked_add(2 * LEAF_MIN_OCCUPANCY))
        .expect("modeled object count");
    assert_eq!(count, 47_361);
    let genesis = build_genesis(&objects(count), limits).expect("129-128 internal children");
    let original = validate_canonical_occupancy(&genesis, limits).expect("canonical level two");
    assert_eq!(original.root_level, 2);

    let object_id = u64::try_from(count).expect("count");
    let mut outputs = Vec::new();
    for policy in [
        ExperimentalDeleteBorrowPolicy::LeftFirst,
        ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
    ] {
        let path = inspect_persistent_delete_repair_path_experimental(
            &genesis, object_id, limits, policy,
        )
        .expect("recursive repair path");
        assert_eq!(path.root_level, 2);
        assert_eq!(path.levels.len(), 2);
        assert!(!path.root_child_removed);
        assert!(!path.root_would_collapse);

        let leaf = &path.levels[0];
        assert_eq!(leaf.level, 0);
        assert!(!leaf.is_root);
        assert!(!leaf.triggered_by_child_removal);
        assert_eq!(leaf.target_occupancy_before, LEAF_MIN_OCCUPANCY);
        assert_eq!(leaf.target_occupancy_after_local_change, LEAF_MIN_OCCUPANCY - 1);
        assert_eq!(leaf.left_occupancy, Some(LEAF_MIN_OCCUPANCY));
        assert_eq!(leaf.right_occupancy, None);
        assert!(leaf.would_underflow);
        assert_eq!(leaf.selected_donor_direction, None);
        assert_eq!(leaf.selected_donor_occupancy, None);
        assert!(!leaf.donor_cliff);
        assert!(!leaf.strictly_fuller_eligible_alternative);
        assert!(leaf.would_merge);

        let internal = &path.levels[1];
        assert_eq!(internal.level, 1);
        assert!(!internal.is_root);
        assert!(internal.triggered_by_child_removal);
        assert_eq!(internal.target_occupancy_before, INTERNAL_MIN_OCCUPANCY);
        assert_eq!(
            internal.target_occupancy_after_local_change,
            INTERNAL_MIN_OCCUPANCY - 1
        );
        assert_eq!(internal.left_occupancy, Some(INTERNAL_MIN_OCCUPANCY + 1));
        assert_eq!(internal.right_occupancy, None);
        assert!(internal.would_underflow);
        assert_eq!(
            internal.selected_donor_direction,
            Some(ExperimentalDeleteBorrowDirection::Left)
        );
        assert_eq!(
            internal.selected_donor_occupancy,
            Some(INTERNAL_MIN_OCCUPANCY + 1)
        );
        assert!(internal.donor_cliff);
        assert!(!internal.strictly_fuller_eligible_alternative);
        assert!(!internal.would_merge);

        let result = append_persistent_delete_experimental(
            &genesis, object_id, limits, policy,
        )
        .expect("recursive persistent delete");
        assert_eq!(result.report.root_level, 2);
        assert_eq!(result.report.object_count, count - 1);
        assert_eq!(result.report.page_count, original.page_count - 1);
        assert_eq!(result.pages_written, 4);
        assert_eq!(result.pages_reused, original.page_count - 5);
        assert_eq!(
            validate_canonical_occupancy(&result.bytes, limits).expect("canonical result"),
            result.report
        );
        outputs.push(result.bytes);
    }

    assert_eq!(outputs[0], outputs[1], "only one internal donor is eligible");
}

#[test]
fn inspectors_match_writer_root_leaf_and_final_object_request_boundaries() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(10), limits).expect("root leaf");
    let path = inspect_persistent_delete_repair_path_experimental(
        &genesis,
        5,
        limits,
        ExperimentalDeleteBorrowPolicy::LeftFirst,
    )
    .expect("root-leaf path");
    assert_eq!(path.root_level, 0);
    assert_eq!(path.levels.len(), 1);
    let root = &path.levels[0];
    assert!(root.is_root);
    assert_eq!(root.target_occupancy_before, 10);
    assert_eq!(root.target_occupancy_after_local_change, 9);
    assert!(!root.would_underflow);
    assert!(!root.would_merge);

    let one = build_genesis(&objects(1), limits).expect("one object");
    assert_eq!(
        inspect_persistent_delete_leaf_frontier_experimental(
            &one,
            1,
            limits,
            ExperimentalDeleteBorrowPolicy::LeftFirst,
        ),
        Err(ImmutableError::Invalid("batch result"))
    );
    assert_eq!(
        inspect_persistent_delete_repair_path_experimental(
            &one,
            1,
            limits,
            ExperimentalDeleteBorrowPolicy::LeftFirst,
        ),
        Err(ImmutableError::Invalid("batch result"))
    );
}
