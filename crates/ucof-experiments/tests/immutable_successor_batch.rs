use ucof_experiments::immutable_successor::{
    append_batch, build_genesis, rewrite_selected, validate, ImmutableBatchOperation,
    ImmutableError, ImmutableLimits, ImmutableObjectInput,
};

fn modeled_object(object_id: u64) -> ImmutableObjectInput {
    ImmutableObjectInput::new(
        object_id,
        u16::try_from(1 + object_id % 5).expect("modeled kind"),
        format!("payload:{object_id}").into_bytes(),
    )
}

fn modeled_objects(count: u64) -> Vec<ImmutableObjectInput> {
    (1..=count).map(modeled_object).collect()
}

#[test]
fn mixed_batch_is_canonical_across_caller_order() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&modeled_objects(400), limits).expect("genesis");
    let operations = vec![
        ImmutableBatchOperation::Put(ImmutableObjectInput::new(401, 7, b"inserted".to_vec())),
        ImmutableBatchOperation::Delete(2),
        ImmutableBatchOperation::Put(ImmutableObjectInput::new(200, 9, b"replacement".to_vec())),
        ImmutableBatchOperation::Delete(399),
    ];

    let appended = append_batch(&genesis, &operations, limits).expect("mixed append");
    let report = validate(&appended, limits).expect("mixed append validates");
    assert_eq!(report.sequence, 1);
    assert_eq!(report.object_count, 399);
    assert_eq!(report.page_count, 4);
    assert_eq!(report.root_level, 1);

    let mut reversed = operations.clone();
    reversed.reverse();
    assert_eq!(
        append_batch(&genesis, &reversed, limits).expect("reordered mixed append"),
        appended
    );
}

#[test]
fn mixed_batch_applies_insert_replace_and_delete_semantics() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&modeled_objects(400), limits).expect("genesis");
    let operations = vec![
        ImmutableBatchOperation::Delete(2),
        ImmutableBatchOperation::Put(ImmutableObjectInput::new(200, 9, b"replacement".to_vec())),
        ImmutableBatchOperation::Put(ImmutableObjectInput::new(401, 7, b"inserted".to_vec())),
        ImmutableBatchOperation::Delete(399),
    ];
    let appended = append_batch(&genesis, &operations, limits).expect("mixed append");

    let selected = rewrite_selected(&appended, &[1, 200, 401], limits)
        .expect("selected active objects rewrite");
    let expected = build_genesis(
        &[
            modeled_object(1),
            ImmutableObjectInput::new(200, 9, b"replacement".to_vec()),
            ImmutableObjectInput::new(401, 7, b"inserted".to_vec()),
        ],
        limits,
    )
    .expect("expected selected genesis");
    assert_eq!(selected.bytes, expected);

    assert_eq!(
        rewrite_selected(&appended, &[2], limits),
        Err(ImmutableError::MissingObject(2))
    );
    assert_eq!(
        rewrite_selected(&appended, &[399], limits),
        Err(ImmutableError::MissingObject(399))
    );
}

#[test]
fn mixed_batch_rejects_ambiguous_or_invalid_changes() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&modeled_objects(4), limits).expect("genesis");

    assert_eq!(
        append_batch(&genesis, &[], limits),
        Err(ImmutableError::Invalid("batch operations"))
    );
    assert_eq!(
        append_batch(
            &genesis,
            &[
                ImmutableBatchOperation::Delete(1),
                ImmutableBatchOperation::Put(modeled_object(1)),
            ],
            limits,
        ),
        Err(ImmutableError::DuplicateObject(1))
    );
    assert_eq!(
        append_batch(&genesis, &[ImmutableBatchOperation::Delete(99)], limits),
        Err(ImmutableError::MissingObject(99))
    );
    assert_eq!(
        append_batch(
            &genesis,
            &[ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                5,
                0,
                b"invalid".to_vec(),
            ))],
            limits,
        ),
        Err(ImmutableError::Invalid("object input"))
    );
    assert_eq!(
        append_batch(
            &genesis,
            &[
                ImmutableBatchOperation::Delete(1),
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Delete(3),
                ImmutableBatchOperation::Delete(4),
            ],
            limits,
        ),
        Err(ImmutableError::Invalid("batch result"))
    );

    let object_limited = ImmutableLimits {
        max_objects: 4,
        ..limits
    };
    assert_eq!(
        append_batch(
            &genesis,
            &[ImmutableBatchOperation::Put(modeled_object(5))],
            object_limited,
        ),
        Err(ImmutableError::Limit("object count"))
    );
}
