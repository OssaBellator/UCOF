use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_insert, build_genesis, validate,
    ImmutableBatchOperation, ImmutableError, ImmutableLimits, ImmutableObjectInput,
    PersistentBatchMode, INTERNAL_FANOUT, LEAF_CAPACITY,
};

fn object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::try_from(1 + object_id % 7).expect("kind"),
        format!("payload:{object_id}").into_bytes(),
    )
}

fn even_objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(|index| object(index * 2))
        .collect()
}

#[test]
fn gap_insertion_rewrites_one_path_and_reuses_other_pages() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let original = validate(&genesis, limits).expect("original validates");
    assert_eq!(original.root_level, 1);
    assert_eq!(original.page_count, 4);

    // The first two leaves are full (2..=370 and 372..=740). Identifier 741 routes into the
    // sparse final leaf (742..=800), so this case exercises insertion without a split.
    let inserted = ImmutableObjectInput::new(741, 11, b"gap-insert".to_vec());
    let result =
        append_persistent_insert(&genesis, &inserted, limits).expect("persistent gap insertion");
    assert_eq!(result.mode, PersistentBatchMode::CopyOnWriteInsertion);
    assert_eq!(result.report.sequence, 1);
    assert_eq!(result.report.object_count, 401);
    assert_eq!(result.report.root_level, 1);
    assert_eq!(result.pages_written, 2);
    assert_eq!(result.pages_reused, 2);
    assert_eq!(result.report.page_count, 4);

    let general =
        append_persistent_batch(&genesis, &[ImmutableBatchOperation::Put(inserted)], limits)
            .expect("general persistent insertion");
    assert_eq!(general, result);
}

#[test]
fn full_root_leaf_splits_and_increases_height() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(LEAF_CAPACITY), limits).expect("full leaf genesis");
    let original = validate(&genesis, limits).expect("full leaf validates");
    assert_eq!(original.root_level, 0);
    assert_eq!(original.page_count, 1);

    let result = append_persistent_insert(
        &genesis,
        &ImmutableObjectInput::new(1, 3, b"root-split".to_vec()),
        limits,
    )
    .expect("root split insertion");
    assert_eq!(result.mode, PersistentBatchMode::CopyOnWriteInsertion);
    assert_eq!(result.report.root_level, 1);
    assert_eq!(result.report.page_count, 3);
    assert_eq!(result.report.object_count, LEAF_CAPACITY + 1);
    assert_eq!(result.pages_written, 3);
    assert_eq!(result.pages_reused, 0);
}

#[test]
fn internal_split_propagates_to_a_new_level_two_root() {
    let limits = ImmutableLimits::default();
    let count = LEAF_CAPACITY
        .checked_mul(INTERNAL_FANOUT)
        .expect("modeled full level-one tree");
    let genesis = build_genesis(&even_objects(count), limits).expect("full level-one genesis");
    let original = validate(&genesis, limits).expect("full level-one validates");
    assert_eq!(original.root_level, 1);
    assert_eq!(original.page_count, INTERNAL_FANOUT + 1);

    let result = append_persistent_insert(
        &genesis,
        &ImmutableObjectInput::new(1, 5, b"internal-split".to_vec()),
        limits,
    )
    .expect("internal split insertion");
    assert_eq!(result.mode, PersistentBatchMode::CopyOnWriteInsertion);
    assert_eq!(result.report.root_level, 2);
    assert_eq!(result.report.object_count, count + 1);
    assert_eq!(result.pages_written, 5);
    assert_eq!(result.pages_reused, original.page_count - 2);
    assert_eq!(
        result.report.page_count,
        result.pages_reused + result.pages_written
    );
}

#[test]
fn insertion_is_deterministic_and_rejects_existing_identifiers() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let inserted = ImmutableObjectInput::new(801, 9, b"right-edge".to_vec());
    assert_eq!(
        append_persistent_insert(&genesis, &inserted, limits)
            .expect("first insertion")
            .bytes,
        append_persistent_insert(&genesis, &inserted, limits)
            .expect("second insertion")
            .bytes
    );
    assert_eq!(
        append_persistent_insert(&genesis, &object(200), limits),
        Err(ImmutableError::DuplicateObject(200))
    );
}

#[test]
fn deletions_and_multi_operation_insertions_remain_explicit_fallbacks() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let deletion = append_persistent_batch(&genesis, &[ImmutableBatchOperation::Delete(2)], limits)
        .expect("deletion fallback");
    assert_eq!(deletion.mode, PersistentBatchMode::FullRebuildShapeChange);

    let mixed = append_persistent_batch(
        &genesis,
        &[
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(801, 9, b"insert-one".to_vec())),
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(803, 9, b"insert-two".to_vec())),
        ],
        limits,
    )
    .expect("multi-insertion fallback");
    assert_eq!(mixed.mode, PersistentBatchMode::FullRebuildShapeChange);
}
