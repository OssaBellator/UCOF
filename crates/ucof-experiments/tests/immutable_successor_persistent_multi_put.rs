use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_put_batch, build_genesis,
    validate_canonical_occupancy, ImmutableBatchOperation, ImmutableError, ImmutableLimits,
    ImmutableObjectInput, PersistentBatchMode, INTERNAL_FANOUT, LEAF_CAPACITY,
};

fn object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::try_from(1 + object_id % 13).expect("kind"),
        format!("payload:{object_id}").into_bytes(),
    )
}

fn even_objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(|index| object(index * 2))
        .collect()
}

#[test]
fn same_leaf_insertions_share_one_leaf_and_root_rewrite() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let inputs = vec![object(617), object(619)];
    let result = append_persistent_put_batch(&genesis, &inputs, limits).expect("multi put");
    assert_eq!(result.mode, PersistentBatchMode::CopyOnWritePutBatch);
    assert_eq!(result.report.sequence, 1);
    assert_eq!(result.report.object_count, 402);
    assert_eq!(result.report.root_level, 1);
    assert_eq!(result.report.page_count, 4);
    assert_eq!(result.pages_written, 2);
    assert_eq!(result.pages_reused, 2);
    assert_eq!(
        validate_canonical_occupancy(&result.bytes, limits).expect("canonical"),
        result.report
    );
}

#[test]
fn insertions_in_different_leaves_share_the_root_rewrite() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let inputs = vec![object(371), object(617)];
    let result = append_persistent_put_batch(&genesis, &inputs, limits).expect("multi put");
    assert_eq!(result.report.object_count, 402);
    assert_eq!(result.report.page_count, 4);
    assert_eq!(result.pages_written, 3);
    assert_eq!(result.pages_reused, 1);
}

#[test]
fn insert_and_replace_in_one_leaf_are_planned_together() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let inputs = vec![
        ImmutableObjectInput::new(700, 31, b"replacement".to_vec()),
        object(701),
    ];
    let result = append_persistent_put_batch(&genesis, &inputs, limits).expect("mixed puts");
    assert_eq!(result.report.object_count, 401);
    assert_eq!(result.pages_written, 2);
    assert_eq!(result.pages_reused, 2);

    let operations: Vec<_> = inputs
        .iter()
        .cloned()
        .map(ImmutableBatchOperation::Put)
        .collect();
    assert_eq!(
        append_persistent_batch(&genesis, &operations, limits).expect("general route"),
        result
    );
}

#[test]
fn multiple_insertions_can_split_one_leaf_once() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let inputs = vec![object(1), object(3)];
    let result = append_persistent_put_batch(&genesis, &inputs, limits).expect("leaf split");
    assert_eq!(result.report.object_count, 402);
    assert_eq!(result.report.root_level, 1);
    assert_eq!(result.report.page_count, 5);
    assert_eq!(result.pages_written, 3);
    assert_eq!(result.pages_reused, 2);
}

#[test]
fn two_full_leaf_splits_propagate_through_one_new_root() {
    let limits = ImmutableLimits::default();
    let count = LEAF_CAPACITY
        .checked_mul(INTERNAL_FANOUT)
        .expect("full level-one tree");
    let genesis = build_genesis(&even_objects(count), limits).expect("full tree");
    let original = validate_canonical_occupancy(&genesis, limits).expect("canonical");
    assert_eq!(original.root_level, 1);
    assert_eq!(original.page_count, INTERNAL_FANOUT + 1);

    let inputs = vec![
        object(1),
        object(u64::try_from(count).expect("count") * 2 + 1),
    ];
    let result = append_persistent_put_batch(&genesis, &inputs, limits).expect("double split");
    assert_eq!(result.report.root_level, 2);
    assert_eq!(result.report.object_count, count + 2);
    assert_eq!(result.pages_written, 7);
    assert_eq!(result.pages_reused, original.page_count - 3);
    assert_eq!(
        result.report.page_count,
        result.pages_reused + result.pages_written
    );
}

#[test]
fn caller_order_is_canonical_and_duplicate_operations_fail() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let forward = vec![object(371), object(617), object(801)];
    let mut reverse = forward.clone();
    reverse.reverse();
    assert_eq!(
        append_persistent_put_batch(&genesis, &forward, limits)
            .expect("forward")
            .bytes,
        append_persistent_put_batch(&genesis, &reverse, limits)
            .expect("reverse")
            .bytes
    );
    assert_eq!(
        append_persistent_put_batch(&genesis, &[object(801), object(801)], limits),
        Err(ImmutableError::DuplicateObject(801))
    );
}

#[test]
fn mixed_deletion_batches_remain_an_explicit_fallback() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let result = append_persistent_batch(
        &genesis,
        &[
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(object(801)),
        ],
        limits,
    )
    .expect("mixed fallback");
    assert_eq!(result.mode, PersistentBatchMode::FullRebuildShapeChange);
}
