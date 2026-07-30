use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, rewrite_all, rewrite_selected, validate, validate_history,
    ImmutableError, ImmutableLimits, ImmutableObjectInput, OBJECT_HEADER_LEN,
};

fn objects() -> Vec<ImmutableObjectInput> {
    vec![
        ImmutableObjectInput::new(1, 1, b"alpha".to_vec()),
        ImmutableObjectInput::new(2, 2, b"bravo".to_vec()),
        ImmutableObjectInput::new(3, 3, b"charlie".to_vec()),
        ImmutableObjectInput::new(4, 1, b"delta".to_vec()),
    ]
}

fn active_objects() -> Vec<ImmutableObjectInput> {
    vec![
        ImmutableObjectInput::new(1, 9, b"alpha-v2".to_vec()),
        ImmutableObjectInput::new(2, 2, b"bravo".to_vec()),
        ImmutableObjectInput::new(3, 3, b"charlie".to_vec()),
        ImmutableObjectInput::new(4, 1, b"delta".to_vec()),
    ]
}

fn active_append() -> (Vec<u8>, Vec<u8>) {
    let genesis = build_genesis(&objects(), ImmutableLimits::default()).expect("genesis");
    let appended = append_replacement(
        &genesis,
        &ImmutableObjectInput::new(1, 9, b"alpha-v2".to_vec()),
        ImmutableLimits::default(),
    )
    .expect("append");
    (genesis, appended)
}

#[test]
fn rewrite_all_publishes_one_new_genesis_with_active_payloads() {
    let (_, appended) = active_append();
    let rewritten = rewrite_all(&appended, ImmutableLimits::default()).expect("rewrite all");
    let expected = build_genesis(&active_objects(), ImmutableLimits::default()).expect("expected");
    assert_eq!(rewritten.source.sequence, 1);
    assert_eq!(rewritten.output.sequence, 0);
    assert_eq!(rewritten.output.object_count, 4);
    assert_eq!(rewritten.retained_object_ids, vec![1, 2, 3, 4]);
    assert!(!rewritten.byte_scoped_signatures_preserved);
    assert_ne!(rewritten.bytes, appended);
    assert_eq!(rewritten.bytes, expected);
    assert_eq!(
        validate(&rewritten.bytes, ImmutableLimits::default()).expect("output validates"),
        rewritten.output
    );
    assert_eq!(
        validate_history(&rewritten.bytes, ImmutableLimits::default())
            .expect("rewritten history")
            .entries
            .len(),
        1
    );
}

#[test]
fn caller_selected_rewrite_is_sorted_deterministic_and_bounded() {
    let (_, appended) = active_append();
    let first =
        rewrite_selected(&appended, &[3, 1], ImmutableLimits::default()).expect("selected rewrite");
    let second = rewrite_selected(&appended, &[1, 3], ImmutableLimits::default())
        .expect("canonical selected rewrite");
    let expected = build_genesis(
        &[
            ImmutableObjectInput::new(1, 9, b"alpha-v2".to_vec()),
            ImmutableObjectInput::new(3, 3, b"charlie".to_vec()),
        ],
        ImmutableLimits::default(),
    )
    .expect("expected selected output");
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.bytes, expected);
    assert_eq!(first.retained_object_ids, vec![1, 3]);
    assert_eq!(first.output.sequence, 0);
    assert_eq!(first.output.object_count, 2);

    let low_output = ImmutableLimits {
        max_output_bytes: 64,
        ..ImmutableLimits::default()
    };
    assert_eq!(
        rewrite_selected(&appended, &[1], low_output),
        Err(ImmutableError::Limit("output"))
    );

    let low_allocation = ImmutableLimits {
        max_allocation_bytes: 32,
        ..ImmutableLimits::default()
    };
    assert_eq!(
        rewrite_selected(&appended, &[1, 3], low_allocation),
        Err(ImmutableError::Limit("allocation"))
    );
}

#[test]
fn caller_selected_rewrite_rejects_missing_duplicate_and_empty_sets() {
    let (_, appended) = active_append();
    assert_eq!(
        rewrite_selected(&appended, &[], ImmutableLimits::default()),
        Err(ImmutableError::Invalid("rewrite selection"))
    );
    assert_eq!(
        rewrite_selected(&appended, &[1, 1], ImmutableLimits::default()),
        Err(ImmutableError::Invalid("rewrite selection"))
    );
    assert_eq!(
        rewrite_selected(&appended, &[99], ImmutableLimits::default()),
        Err(ImmutableError::MissingObject(99))
    );
}

#[test]
fn rewrite_requires_strictly_valid_active_records_and_current_commit() {
    let (genesis, mut appended) = active_append();
    let second_payload = 64 + (OBJECT_HEADER_LEN + b"alpha".len()) + OBJECT_HEADER_LEN;
    appended[second_payload] ^= 0x01;
    assert_eq!(
        rewrite_all(&appended, ImmutableLimits::default()),
        Err(ImmutableError::Invalid("object digest"))
    );

    let mut damaged_current = active_append().1;
    damaged_current[genesis.len() + OBJECT_HEADER_LEN] ^= 0x01;
    assert_eq!(
        rewrite_all(&damaged_current, ImmutableLimits::default()),
        Err(ImmutableError::Invalid("commit digest"))
    );
}
