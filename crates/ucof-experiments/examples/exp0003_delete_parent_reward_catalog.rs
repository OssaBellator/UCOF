use ucof_experiments::immutable_successor::{
    append_persistent_delete, append_persistent_insert, build_genesis, validate_canonical_occupancy,
    FOOTER_LEN, INTERNAL_FANOUT, ImmutableLimits, ImmutableObjectInput, LEAF_CAPACITY,
    LEAF_MIN_OCCUPANCY, PAGE_SIZE, SNAPSHOT_LEN,
};

#[derive(Clone, Copy)]
struct Case<'a> {
    name: &'a str,
    repair_class: &'a str,
    object_id: u64,
}

fn object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, 1, vec![object_id as u8])
}

fn objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(object)
        .collect()
}

fn comparison_fixture(limits: ImmutableLimits) -> Vec<u8> {
    let mut state = build_genesis(&objects(2 * LEAF_CAPACITY), limits).expect("two full leaves");
    for object_id in u64::try_from(2 * LEAF_CAPACITY + 1).expect("first insertion")..=379 {
        state = append_persistent_insert(&state, &object(object_id), limits)
            .expect("grow right sibling")
            .bytes;
    }

    let left_deletions = LEAF_CAPACITY - (LEAF_MIN_OCCUPANCY + 1);
    for object_id in 1..=u64::try_from(left_deletions).expect("left deletions") {
        state = append_persistent_delete(&state, object_id, limits)
            .expect("shrink left sibling")
            .bytes;
    }
    state
}

fn three_minimum_leaves(limits: ImmutableLimits) -> Vec<u8> {
    let mut state = comparison_fixture(limits);

    // The comparison fixture has leaves [94, 93, 101].  Shrink the outer leaves
    // without underflow so deleting 186 forces a leaf merge while the level-one
    // root still retains two children.
    state = append_persistent_delete(&state, 92, limits)
        .expect("left leaf to minimum")
        .bytes;
    for object_id in 372..=379 {
        state = append_persistent_delete(&state, object_id, limits)
            .expect("right leaf to minimum")
            .bytes;
    }
    state
}

fn emit_case(case: Case<'_>, source: &[u8], limits: ImmutableLimits) {
    let source_report = validate_canonical_occupancy(source, limits).expect("source canonical");
    let result = append_persistent_delete(source, case.object_id, limits).expect("delete case");
    let appended_bytes = result.bytes.len() - source.len();
    let expected_appended_bytes = result
        .pages_written
        .checked_mul(PAGE_SIZE)
        .and_then(|value| value.checked_add(SNAPSHOT_LEN + FOOTER_LEN))
        .expect("expected appended bytes");
    assert_eq!(
        appended_bytes, expected_appended_bytes,
        "deletion append bytes must be page reward plus snapshot/footer tail"
    );

    let touched_original = source_report
        .page_count
        .checked_sub(result.pages_reused)
        .expect("touched original pages");
    let page_count_delta = i64::try_from(result.report.page_count).expect("result pages")
        - i64::try_from(source_report.page_count).expect("source pages");
    let root_level_delta = i16::from(result.report.root_level) - i16::from(source_report.root_level);

    println!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        case.name,
        case.repair_class,
        source_report.root_level,
        result.report.root_level,
        root_level_delta,
        source_report.page_count,
        result.report.page_count,
        page_count_delta,
        touched_original,
        result.pages_reused,
        result.pages_written,
        appended_bytes,
        expected_appended_bytes,
        source_report.object_count,
        result.report.object_count,
    );
}

fn main() {
    let limits = ImmutableLimits::default();
    assert_eq!(PAGE_SIZE, 16_384);
    assert_eq!(SNAPSHOT_LEN + FOOTER_LEN, 224);
    assert_eq!(LEAF_CAPACITY, 185);
    assert_eq!(LEAF_MIN_OCCUPANCY, 93);

    println!(
        "case,repair_class,source_root_level,result_root_level,root_level_delta,source_page_count,result_page_count,page_count_delta,touched_original,pages_reused,pages_written,bytes_appended,expected_affine_bytes,source_objects,result_objects"
    );

    let root_leaf = build_genesis(&objects(10), limits).expect("root leaf");
    emit_case(
        Case {
            name: "root-leaf",
            repair_class: "root-leaf-rewrite",
            object_id: 5,
        },
        &root_leaf,
        limits,
    );

    let stable_depth_one = build_genesis(&objects(400), limits).expect("stable depth one");
    emit_case(
        Case {
            name: "depth1-no-underflow",
            repair_class: "path-copy-no-underflow",
            object_id: 10,
        },
        &stable_depth_one,
        limits,
    );

    let borrow_left = build_genesis(&objects(LEAF_CAPACITY + 2), limits).expect("left borrow");
    emit_case(
        Case {
            name: "depth1-borrow-left",
            repair_class: "leaf-borrow-parent-rewrite",
            object_id: u64::try_from(LEAF_CAPACITY + 2).expect("borrow target"),
        },
        &borrow_left,
        limits,
    );

    let borrow_right_base =
        build_genesis(&objects(2 * LEAF_MIN_OCCUPANCY), limits).expect("right borrow base");
    let borrow_right = append_persistent_insert(
        &borrow_right_base,
        &ImmutableObjectInput::new(10_000, 1, b"right".to_vec()),
        limits,
    )
    .expect("grow right sibling")
    .bytes;
    emit_case(
        Case {
            name: "depth1-borrow-right",
            repair_class: "leaf-borrow-parent-rewrite",
            object_id: 1,
        },
        &borrow_right,
        limits,
    );

    let collapse =
        build_genesis(&objects(2 * LEAF_MIN_OCCUPANCY), limits).expect("collapse base");
    emit_case(
        Case {
            name: "depth1-merge-root-collapse",
            repair_class: "leaf-merge-root-collapse",
            object_id: 1,
        },
        &collapse,
        limits,
    );

    let keep_root = three_minimum_leaves(limits);
    emit_case(
        Case {
            name: "depth1-merge-keep-root",
            repair_class: "leaf-merge-parent-rewrite",
            object_id: 186,
        },
        &keep_root,
        limits,
    );

    let count = INTERNAL_FANOUT
        .checked_mul(LEAF_CAPACITY)
        .and_then(|value| value.checked_add(2 * LEAF_MIN_OCCUPANCY))
        .expect("level-two object count");
    let recursive = build_genesis(&objects(count), limits).expect("level-two recursive case");
    emit_case(
        Case {
            name: "depth2-recursive-internal-borrow",
            repair_class: "leaf-merge-internal-borrow-root-rewrite",
            object_id: u64::try_from(count).expect("recursive target"),
        },
        &recursive,
        limits,
    );
}
