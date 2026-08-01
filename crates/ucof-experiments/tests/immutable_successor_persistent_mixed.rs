use ucof_experiments::immutable_successor::{
    append_persistent_batch, append_persistent_mixed_batch, build_genesis, rewrite_selected,
    validate_canonical_occupancy, ImmutableBatchOperation, ImmutableLimits, ImmutableObjectInput,
    PersistentBatchMode,
};

fn object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::try_from(1 + object_id % 19).expect("kind"),
        format!("payload:{object_id}").into_bytes(),
    )
}

fn even_objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(|index| object(index * 2))
        .collect()
}

#[test]
fn stable_shape_reuses_exact_untouched_leaves() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(400), limits).expect("genesis");
    let replacement = ImmutableObjectInput::new(702, 77, b"replacement-702".to_vec());
    let inserted = ImmutableObjectInput::new(701, 78, b"inserted-701".to_vec());
    let operations = vec![
        ImmutableBatchOperation::Delete(700),
        ImmutableBatchOperation::Put(inserted.clone()),
        ImmutableBatchOperation::Put(replacement.clone()),
    ];

    let result = append_persistent_mixed_batch(&genesis, &operations, limits)
        .expect("persistent mixed batch");
    assert_eq!(result.mode, PersistentBatchMode::CopyOnWriteCanonicalMixed);
    assert_eq!(result.report.sequence, 1);
    assert_eq!(result.report.object_count, 400);
    assert_eq!(result.report.page_count, 4);
    assert_eq!(result.report.root_level, 1);
    assert_eq!(result.pages_written, 2);
    assert_eq!(result.pages_reused, 2);
    assert_eq!(
        validate_canonical_occupancy(&result.bytes, limits).expect("canonical"),
        result.report
    );

    assert_eq!(
        append_persistent_batch(&genesis, &operations, limits).expect("general route"),
        result
    );
    let mut reversed = operations.clone();
    reversed.reverse();
    assert_eq!(
        append_persistent_mixed_batch(&genesis, &reversed, limits)
            .expect("reversed")
            .bytes,
        result.bytes
    );

    let selected = rewrite_selected(&result.bytes, &[701, 702], limits).expect("selected rewrite");
    assert_eq!(
        selected.bytes,
        build_genesis(&[inserted, replacement], limits).expect("expected selected bytes")
    );
    assert!(rewrite_selected(&result.bytes, &[700], limits).is_err());
}

#[test]
fn canonical_regrouping_can_collapse_the_root() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(186), limits).expect("two-leaf genesis");
    let result = append_persistent_mixed_batch(
        &genesis,
        &[
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                4,
                91,
                b"replacement-four".to_vec(),
            )),
        ],
        limits,
    )
    .expect("root collapse");
    assert_eq!(result.report.object_count, 185);
    assert_eq!(result.report.root_level, 0);
    assert_eq!(result.report.page_count, 1);
    assert_eq!(result.pages_written, 1);
    assert_eq!(result.pages_reused, 0);
}

#[test]
fn canonical_regrouping_can_grow_the_root() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(185), limits).expect("root leaf genesis");
    let operations = [
        ImmutableBatchOperation::Delete(2),
        ImmutableBatchOperation::Put(object(1)),
        ImmutableBatchOperation::Put(object(371)),
    ];
    let result = append_persistent_mixed_batch(&genesis, &operations, limits).expect("root growth");
    assert_eq!(result.report.object_count, 186);
    assert_eq!(result.report.root_level, 1);
    assert_eq!(result.report.page_count, 3);
    assert_eq!(result.pages_written, 3);
    assert_eq!(result.pages_reused, 0);
}

#[test]
fn invalid_mixed_requests_fail_before_publication() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&even_objects(4), limits).expect("genesis");
    assert!(append_persistent_mixed_batch(
        &genesis,
        &[
            ImmutableBatchOperation::Delete(99),
            ImmutableBatchOperation::Put(object(9)),
        ],
        limits,
    )
    .is_err());
    assert!(append_persistent_mixed_batch(
        &genesis,
        &[
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(object(2)),
        ],
        limits,
    )
    .is_err());
}
