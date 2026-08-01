/// Streams any currently supported persistent batch shape through its specialized append-tail path.
///
/// Classification, exact-end canonical validation, duplicate detection, and mode-specific preflight
/// all complete before output begins. Payloads remain borrowed while dispatching multi-`Put` batches.
pub fn append_persistent_batch_to<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
    options: PersistentMixedStreamingOptions,
) -> Result<PersistentMixedStreamingReport, PersistentMixedStreamingError> {
    if operations.is_empty() {
        return Err(ImmutableError::Invalid("batch operations").into());
    }
    let previous = validate_canonical_internal(data, limits)?;
    allocation_check::<usize>(operations.len(), limits)?;
    let mut order: Vec<usize> = (0..operations.len()).collect();
    order.sort_unstable_by_key(|index| operations[*index].object_id());
    if let Some(pair) = order.windows(2).find(|pair| {
        operations[pair[0]].object_id() == operations[pair[1]].object_id()
    }) {
        return Err(ImmutableError::DuplicateObject(
            operations[pair[0]].object_id(),
        )
        .into());
    }

    if operations.len() > 1
        && operations
            .iter()
            .any(|operation| matches!(operation, ImmutableBatchOperation::Delete(_)))
    {
        return append_persistent_mixed_batch_to(writer, data, operations, limits, options);
    }

    if operations.len() == 1 {
        return match &operations[order[0]] {
            ImmutableBatchOperation::Delete(object_id) => {
                append_persistent_delete_to(writer, data, *object_id, limits, options)
            }
            ImmutableBatchOperation::Put(input)
                if previous
                    .locators
                    .binary_search_by_key(&input.object_id, |locator| locator.object_id)
                    .is_err() =>
            {
                append_persistent_insert_to(writer, data, input, limits, options)
            }
            ImmutableBatchOperation::Put(_) => append_persistent_replacement_batch_to(
                writer, data, operations, limits, options,
            ),
        };
    }

    allocation_check::<&ImmutableObjectInput>(operations.len(), limits)?;
    let inputs: Vec<&ImmutableObjectInput> = order
        .iter()
        .filter_map(|index| match &operations[*index] {
            ImmutableBatchOperation::Put(input) => Some(input),
            ImmutableBatchOperation::Delete(_) => None,
        })
        .collect();
    if inputs.len() != operations.len() {
        return Err(ImmutableError::Invalid("persistent streaming dispatch").into());
    }
    let any_insertion = inputs.iter().any(|input| {
        previous
            .locators
            .binary_search_by_key(&input.object_id, |locator| locator.object_id)
            .is_err()
    });
    if any_insertion {
        append_persistent_put_refs_to(writer, data, &inputs, limits, options)
    } else {
        append_persistent_replacement_batch_to(writer, data, operations, limits, options)
    }
}

#[cfg(test)]
mod persistent_streaming_dispatch_tests {
    use super::*;

    fn object(object_id: u64, seed: u8) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::from(1 + seed % 31),
            vec![seed; 1 + usize::from(seed % 23)],
        )
    }

    fn even_objects(count: usize) -> Vec<ImmutableObjectInput> {
        (1..=count)
            .map(|index| {
                object(
                    u64::try_from(index * 2).expect("identifier"),
                    u8::try_from(index % 251).expect("seed"),
                )
            })
            .collect()
    }

    fn base(limits: ImmutableLimits) -> Vec<u8> {
        build_genesis(&even_objects(400), limits).expect("base")
    }

    fn assert_dispatch_matches_owned(
        label: &str,
        base: &[u8],
        operations: &[ImmutableBatchOperation],
        expected_mode: PersistentBatchMode,
        limits: ImmutableLimits,
    ) {
        let owned = append_persistent_batch(base, operations, limits).expect("owned batch");
        let mut streamed = Vec::new();
        let report = append_persistent_batch_to(
            &mut streamed,
            base,
            operations,
            limits,
            PersistentMixedStreamingOptions {
                max_write_request_bytes: 29,
            },
        )
        .expect("streamed batch");
        assert_eq!(streamed, owned.bytes, "{label}: bytes");
        assert_eq!(report.report, owned.report, "{label}: report");
        assert_eq!(report.mode, expected_mode, "{label}: expected mode");
        assert_eq!(report.mode, owned.mode, "{label}: owned mode");
        assert_eq!(
            report.pages_written, owned.pages_written,
            "{label}: pages written"
        );
        assert_eq!(
            report.pages_reused, owned.pages_reused,
            "{label}: pages reused"
        );
        assert!(report.largest_write_request <= 29, "{label}: write bound");
        assert!(
            report.tail_allocation_bytes < streamed.len(),
            "{label}: tail allocation"
        );
    }

    #[test]
    fn dispatcher_selects_replacement_tail() {
        let limits = ImmutableLimits::default();
        let base = base(limits);
        assert_dispatch_matches_owned(
            "replacement",
            &base,
            &[ImmutableBatchOperation::Put(object(2, 201))],
            PersistentBatchMode::CopyOnWriteReplacements,
            limits,
        );
    }

    #[test]
    fn dispatcher_selects_insertion_tail() {
        let limits = ImmutableLimits::default();
        let base = base(limits);
        assert_dispatch_matches_owned(
            "insertion",
            &base,
            &[ImmutableBatchOperation::Put(object(801, 202))],
            PersistentBatchMode::CopyOnWriteInsertion,
            limits,
        );
    }

    #[test]
    fn dispatcher_selects_multi_put_tail() {
        let limits = ImmutableLimits::default();
        let base = base(limits);
        assert_dispatch_matches_owned(
            "multi put",
            &base,
            &[
                ImmutableBatchOperation::Put(object(2, 203)),
                ImmutableBatchOperation::Put(object(801, 204)),
            ],
            PersistentBatchMode::CopyOnWritePutBatch,
            limits,
        );
    }

    #[test]
    fn dispatcher_selects_deletion_tail() {
        let limits = ImmutableLimits::default();
        let base = base(limits);
        assert_dispatch_matches_owned(
            "deletion",
            &base,
            &[ImmutableBatchOperation::Delete(2)],
            PersistentBatchMode::CopyOnWriteDeletion,
            limits,
        );
    }

    #[test]
    fn dispatcher_selects_mixed_tail() {
        let limits = ImmutableLimits::default();
        let base = base(limits);
        assert_dispatch_matches_owned(
            "mixed",
            &base,
            &[
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(801, 205)),
            ],
            PersistentBatchMode::CopyOnWriteCanonicalMixed,
            limits,
        );
    }

    #[test]
    fn dispatcher_is_order_independent_and_rejects_before_output() {
        let limits = ImmutableLimits::default();
        let base = base(limits);
        let forward = vec![
            ImmutableBatchOperation::Put(object(2, 211)),
            ImmutableBatchOperation::Put(object(401, 212)),
            ImmutableBatchOperation::Put(object(801, 213)),
        ];
        let mut reverse = forward.clone();
        reverse.reverse();
        let mut first = Vec::new();
        let mut second = Vec::new();
        let first_report = append_persistent_batch_to(
            &mut first,
            &base,
            &forward,
            limits,
            PersistentMixedStreamingOptions::default(),
        )
        .expect("forward");
        let second_report = append_persistent_batch_to(
            &mut second,
            &base,
            &reverse,
            limits,
            PersistentMixedStreamingOptions::default(),
        )
        .expect("reverse");
        assert_eq!(first, second);
        assert_eq!(first_report, second_report);

        for operations in [
            Vec::new(),
            vec![
                ImmutableBatchOperation::Put(object(801, 1)),
                ImmutableBatchOperation::Delete(801),
            ],
        ] {
            let mut sink = Vec::new();
            assert!(append_persistent_batch_to(
                &mut sink,
                &base,
                &operations,
                limits,
                PersistentMixedStreamingOptions::default(),
            )
            .is_err());
            assert!(sink.is_empty());
        }
    }
}
