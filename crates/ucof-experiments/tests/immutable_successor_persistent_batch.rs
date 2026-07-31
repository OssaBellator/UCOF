use ucof_experiments::immutable_successor::{
    append_persistent_batch, build_genesis, validate, ImmutableBatchOperation, ImmutableLimits,
    ImmutableObjectInput, PersistentBatchMode, INTERNAL_FANOUT, LEAF_CAPACITY,
};

fn object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::try_from(1 + object_id % 7).expect("kind"),
        format!("payload:{object_id}").into_bytes(),
    )
}

fn objects(count: usize) -> Vec<ImmutableObjectInput> {
    (1..=u64::try_from(count).expect("count"))
        .map(object)
        .collect()
}

#[test]
fn replacement_batch_reuses_untouched_pages() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(400), limits).expect("genesis");
    let original = validate(&genesis, limits).expect("original validates");
    assert_eq!(original.page_count, 4);
    assert_eq!(original.root_level, 1);

    let result = append_persistent_batch(
        &genesis,
        &[ImmutableBatchOperation::Put(ImmutableObjectInput::new(
            1,
            9,
            b"replacement-one".to_vec(),
        ))],
        limits,
    )
    .expect("persistent replacement");
    assert_eq!(result.mode, PersistentBatchMode::CopyOnWriteReplacements);
    assert_eq!(result.report.sequence, 1);
    assert_eq!(result.report.page_count, 4);
    assert_eq!(result.pages_written, 2);
    assert_eq!(result.pages_reused, 2);
}

#[test]
fn multiple_replacement_paths_share_rewritten_ancestors() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(400), limits).expect("genesis");
    let operations = vec![
        ImmutableBatchOperation::Put(ImmutableObjectInput::new(
            400,
            8,
            b"replacement-last".to_vec(),
        )),
        ImmutableBatchOperation::Put(ImmutableObjectInput::new(
            1,
            9,
            b"replacement-first".to_vec(),
        )),
    ];
    let result = append_persistent_batch(&genesis, &operations, limits)
        .expect("persistent replacements");
    assert_eq!(result.mode, PersistentBatchMode::CopyOnWriteReplacements);
    assert_eq!(result.pages_written, 3);
    assert_eq!(result.pages_reused, 1);

    let mut reversed = operations;
    reversed.reverse();
    assert_eq!(
        append_persistent_batch(&genesis, &reversed, limits)
            .expect("reordered persistent replacements")
            .bytes,
        result.bytes
    );
}

#[test]
fn replacement_fast_path_operates_above_one_internal_level() {
    let limits = ImmutableLimits::default();
    let count = LEAF_CAPACITY
        .checked_mul(INTERNAL_FANOUT)
        .and_then(|value| value.checked_add(1))
        .expect("modeled count");
    let genesis = build_genesis(&objects(count), limits).expect("deep genesis");
    let original = validate(&genesis, limits).expect("deep genesis validates");
    assert_eq!(original.root_level, 2);

    let result = append_persistent_batch(
        &genesis,
        &[ImmutableBatchOperation::Put(ImmutableObjectInput::new(
            u64::try_from(count).expect("object id"),
            11,
            b"deep-replacement".to_vec(),
        ))],
        limits,
    )
    .expect("deep persistent replacement");
    assert_eq!(result.mode, PersistentBatchMode::CopyOnWriteReplacements);
    assert_eq!(result.report.root_level, 2);
    assert_eq!(result.pages_written, 3);
    assert_eq!(
        result.pages_reused,
        original.page_count - result.pages_written
    );
}

#[test]
fn insertions_and_deletions_report_full_rebuild_fallback() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(400), limits).expect("genesis");
    let result = append_persistent_batch(
        &genesis,
        &[
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                401,
                10,
                b"inserted".to_vec(),
            )),
        ],
        limits,
    )
    .expect("shape-changing batch");
    assert_eq!(result.mode, PersistentBatchMode::FullRebuildShapeChange);
    assert_eq!(result.pages_reused, 0);
    assert_eq!(result.pages_written, result.report.page_count);
    assert_eq!(result.report.object_count, 400);
}
