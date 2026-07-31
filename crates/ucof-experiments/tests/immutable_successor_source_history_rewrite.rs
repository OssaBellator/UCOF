use ucof_experiments::immutable_successor::{
    append_batch, build_genesis, rewrite_source_selected_history, validate_history,
    ImmutableBatchOperation, ImmutableError, ImmutableLimits, ImmutableObjectInput,
    ImmutableSliceSource, ImmutableSourceError, ImmutableSourceLimits,
};

fn object(object_id: u64, payload: &str) -> ImmutableObjectInput {
    ImmutableObjectInput::new(object_id, 1, payload.as_bytes().to_vec())
}

fn source_history() -> Vec<u8> {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(
        &[object(1, "alpha"), object(2, "bravo"), object(3, "charlie")],
        limits,
    )
    .expect("genesis");
    let first = append_batch(
        &genesis,
        &[
            ImmutableBatchOperation::Put(object(1, "alpha-v2")),
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(object(4, "delta")),
        ],
        limits,
    )
    .expect("first append");
    let second = append_batch(
        &first,
        &[
            ImmutableBatchOperation::Put(object(4, "delta-v2")),
            ImmutableBatchOperation::Put(object(5, "echo")),
        ],
        limits,
    )
    .expect("second append");
    append_batch(
        &second,
        &[ImmutableBatchOperation::Put(object(1, "alpha-v2"))],
        limits,
    )
    .expect("semantic no-op source append")
}

#[test]
fn sparse_selected_history_is_reissued_in_chronological_order() {
    let source = source_history();
    let limits = ImmutableSourceLimits::default();
    let mut reader = ImmutableSliceSource::new(&source);
    let result = rewrite_source_selected_history(&mut reader, &[3, 0, 2], limits)
        .expect("selected history rewrite");

    assert_eq!(result.retained.len(), 3);
    assert_eq!(
        result
            .retained
            .iter()
            .map(|entry| entry.source.sequence)
            .collect::<Vec<_>>(),
        vec![0, 2, 3]
    );
    assert_eq!(
        result
            .retained
            .iter()
            .map(|entry| entry.output.sequence)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(result.retained[0].source.object_count, 3);
    assert_eq!(result.retained[1].source.object_count, 4);
    assert_eq!(result.retained[2].source.object_count, 4);
    assert!(!result.byte_scoped_signatures_preserved);
    assert!(result.stats.read_operations > 0);
    assert!(result.stats.bytes_read > 0);

    let output_history = validate_history(&result.bytes, limits.format).expect("output history");
    assert_eq!(output_history.entries.len(), 3);
    assert_eq!(output_history.entries[0].report.sequence, 2);
    assert_eq!(output_history.entries[2].report.sequence, 0);
}

#[test]
fn selection_order_is_canonical_and_deterministic() {
    let source = source_history();
    let limits = ImmutableSourceLimits::default();
    let mut forward = ImmutableSliceSource::new(&source);
    let mut reverse = ImmutableSliceSource::new(&source);
    assert_eq!(
        rewrite_source_selected_history(&mut forward, &[0, 2, 3], limits)
            .expect("forward")
            .bytes,
        rewrite_source_selected_history(&mut reverse, &[3, 2, 0], limits)
            .expect("reverse")
            .bytes
    );
}

#[test]
fn singleton_selection_becomes_a_new_genesis() {
    let source = source_history();
    let limits = ImmutableSourceLimits::default();
    let mut reader = ImmutableSliceSource::new(&source);
    let result =
        rewrite_source_selected_history(&mut reader, &[1], limits).expect("singleton rewrite");
    assert_eq!(result.retained.len(), 1);
    assert_eq!(result.retained[0].source.sequence, 1);
    assert_eq!(result.retained[0].output.sequence, 0);
    assert_eq!(result.retained[0].source.object_count, 3);
    assert_eq!(result.retained[0].output.object_count, 3);
    assert_eq!(
        validate_history(&result.bytes, limits.format)
            .expect("history")
            .entries
            .len(),
        1
    );
}

#[test]
fn identical_selected_states_keep_a_distinct_history_boundary() {
    let source = source_history();
    let limits = ImmutableSourceLimits::default();
    let mut reader = ImmutableSliceSource::new(&source);
    let result = rewrite_source_selected_history(&mut reader, &[2, 3], limits)
        .expect("identical state boundary");
    assert_eq!(result.retained.len(), 2);
    assert_eq!(result.retained[0].source.object_count, 4);
    assert_eq!(result.retained[1].source.object_count, 4);
    assert_eq!(result.retained[0].output.sequence, 0);
    assert_eq!(result.retained[1].output.sequence, 1);
    assert_eq!(
        validate_history(&result.bytes, limits.format)
            .expect("history")
            .entries
            .len(),
        2
    );
}

#[test]
fn invalid_or_unavailable_selections_fail_closed() {
    let source = source_history();
    let limits = ImmutableSourceLimits::default();
    let mut reader = ImmutableSliceSource::new(&source);
    assert_eq!(
        rewrite_source_selected_history(&mut reader, &[], limits),
        Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "history selection"
        )))
    );

    let mut reader = ImmutableSliceSource::new(&source);
    assert_eq!(
        rewrite_source_selected_history(&mut reader, &[1, 1], limits),
        Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "history selection"
        )))
    );

    let mut reader = ImmutableSliceSource::new(&source);
    assert_eq!(
        rewrite_source_selected_history(&mut reader, &[99], limits),
        Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "history selection"
        )))
    );
}

#[test]
fn cumulative_read_budget_applies_across_history_and_snapshot_rereads() {
    let source = source_history();
    let limits = ImmutableSourceLimits {
        max_total_bytes_read: 64,
        ..ImmutableSourceLimits::default()
    };
    let mut reader = ImmutableSliceSource::new(&source);
    assert_eq!(
        rewrite_source_selected_history(&mut reader, &[0, 3], limits),
        Err(ImmutableSourceError::Limit("read bytes"))
    );
}
