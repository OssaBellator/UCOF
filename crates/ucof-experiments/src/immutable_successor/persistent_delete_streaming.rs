fn materialize_deletion_tail_node(
    tail: &mut Vec<u8>,
    base_len: usize,
    node: &PendingDeletionNode,
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<PageRef, ImmutableError> {
    match node {
        PendingDeletionNode::Leaf(entries) => append_persistent_tail_page(
            tail,
            base_len,
            &encode_leaf(entries)?,
            limits,
            pages_written,
        ),
        PendingDeletionNode::Internal { level, children } => append_persistent_tail_page(
            tail,
            base_len,
            &encode_internal(children, *level)?,
            limits,
            pages_written,
        ),
    }
}

fn delete_persistent_tail_node(
    data: &[u8],
    tail: &mut Vec<u8>,
    base_len: usize,
    reference: &PageRef,
    object_id: u64,
    limits: ImmutableLimits,
    pages_written: &mut usize,
    touched_original: &mut usize,
) -> Result<PendingDeletionNode, ImmutableError> {
    increment_touched(touched_original, limits)?;
    let mut node = load_deletion_node(data, reference, limits)?;
    if let PendingDeletionNode::Leaf(entries) = &mut node {
        let position = entries
            .binary_search_by_key(&object_id, |entry| entry.object_id)
            .map_err(|_| ImmutableError::MissingObject(object_id))?;
        entries.remove(position);
        return Ok(node);
    }

    let PendingDeletionNode::Internal { level, children } = &mut node else {
        return Err(ImmutableError::Invalid("deletion node"));
    };
    let child_index = children
        .iter()
        .position(|child| child.minimum <= object_id && object_id <= child.maximum)
        .ok_or(ImmutableError::MissingObject(object_id))?;
    let mut target = delete_persistent_tail_node(
        data,
        tail,
        base_len,
        &children[child_index],
        object_id,
        limits,
        pages_written,
        touched_original,
    )?;
    let minimum = deletion_minimum(target.level());

    if target.occupancy() >= minimum {
        children[child_index] = materialize_deletion_tail_node(
            tail,
            base_len,
            &target,
            limits,
            pages_written,
        )?;
        return Ok(node);
    }

    let mut left = if child_index > 0 {
        Some(load_deletion_node(data, &children[child_index - 1], limits)?)
    } else {
        None
    };
    if let Some(left_node) = &mut left {
        if left_node.occupancy() > minimum {
            increment_touched(touched_original, limits)?;
            borrow_deletion_from_left(left_node, &mut target, limits)?;
            children[child_index - 1] = materialize_deletion_tail_node(
                tail,
                base_len,
                left_node,
                limits,
                pages_written,
            )?;
            children[child_index] = materialize_deletion_tail_node(
                tail,
                base_len,
                &target,
                limits,
                pages_written,
            )?;
            return Ok(node);
        }
    }

    let mut right = if child_index + 1 < children.len() {
        Some(load_deletion_node(data, &children[child_index + 1], limits)?)
    } else {
        None
    };
    if let Some(right_node) = &mut right {
        if right_node.occupancy() > minimum {
            increment_touched(touched_original, limits)?;
            borrow_deletion_from_right(&mut target, right_node, limits)?;
            children[child_index] = materialize_deletion_tail_node(
                tail,
                base_len,
                &target,
                limits,
                pages_written,
            )?;
            children[child_index + 1] = materialize_deletion_tail_node(
                tail,
                base_len,
                right_node,
                limits,
                pages_written,
            )?;
            return Ok(node);
        }
    }

    if let Some(left_node) = left {
        increment_touched(touched_original, limits)?;
        let merged = merge_deletion_nodes(left_node, target, limits)?;
        children[child_index - 1] = materialize_deletion_tail_node(
            tail,
            base_len,
            &merged,
            limits,
            pages_written,
        )?;
        children.remove(child_index);
    } else if let Some(right_node) = right {
        increment_touched(touched_original, limits)?;
        let merged = merge_deletion_nodes(target, right_node, limits)?;
        children[child_index] = materialize_deletion_tail_node(
            tail,
            base_len,
            &merged,
            limits,
            pages_written,
        )?;
        children.remove(child_index + 1);
    } else {
        return Err(ImmutableError::Invalid("deletion sibling"));
    }

    if *level == 0 {
        return Err(ImmutableError::Invalid("deletion internal level"));
    }
    Ok(node)
}

/// Streams one persistent deletion as the verified base followed by one absolute-offset append tail.
///
/// Exact-end canonical validation, request checks, deterministic borrow/merge repair, recursive
/// underflow handling, root collapse, commit hashing, and output limits all complete before the first
/// sink write. Sink failure after output begins is terminal and returns no success report.
pub fn append_persistent_delete_to<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
    object_id: u64,
    limits: ImmutableLimits,
    options: PersistentMixedStreamingOptions,
) -> Result<PersistentMixedStreamingReport, PersistentMixedStreamingError> {
    if options.max_write_request_bytes == 0 {
        return Err(ImmutableError::Invalid("write request").into());
    }
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output").into());
    }
    if object_id == 0 {
        return Err(ImmutableError::Invalid("batch object id").into());
    }

    let previous = validate_canonical_internal(data, limits)?;
    if previous.locators.len() <= 1 {
        return Err(ImmutableError::Invalid("batch result").into());
    }
    if previous
        .locators
        .binary_search_by_key(&object_id, |locator| locator.object_id)
        .is_err()
    {
        return Err(ImmutableError::MissingObject(object_id).into());
    }

    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let base_len = data.len();
    let mut tail = Vec::new();
    let mut pages_written = 0_usize;
    let mut touched_original = 0_usize;
    let pending = delete_persistent_tail_node(
        data,
        &mut tail,
        base_len,
        &root,
        object_id,
        limits,
        &mut pages_written,
        &mut touched_original,
    )?;

    let next_root = match pending {
        PendingDeletionNode::Leaf(entries) => materialize_deletion_tail_node(
            &mut tail,
            base_len,
            &PendingDeletionNode::Leaf(entries),
            limits,
            &mut pages_written,
        )?,
        PendingDeletionNode::Internal { level: _, children } if children.len() == 1 => children
            .into_iter()
            .next()
            .ok_or(ImmutableError::Invalid("deletion root collapse"))?,
        PendingDeletionNode::Internal { level, children } => materialize_deletion_tail_node(
            &mut tail,
            base_len,
            &PendingDeletionNode::Internal { level, children },
            limits,
            &mut pages_written,
        )?,
    };

    let pages_reused = previous
        .public
        .page_count
        .checked_sub(touched_original)
        .ok_or(ImmutableError::Invalid("persistent page accounting"))?;
    let reachable_page_count = pages_reused
        .checked_add(pages_written)
        .ok_or(ImmutableError::Limit("page count"))?;
    let object_count = previous
        .public
        .object_count
        .checked_sub(1)
        .ok_or(ImmutableError::Invalid("batch result"))?;
    let publication = PersistentTailPublication {
        base_len,
        sequence: previous
            .public
            .sequence
            .checked_add(1)
            .ok_or(ImmutableError::Limit("sequence"))?,
        root: next_root,
        parent_snapshot_digest: previous.public.snapshot_digest,
        previous_footer_offset: u64_from_usize(previous.footer_offset)?,
        page_count: pages_written,
        object_count,
    };
    let mut report = publish_persistent_tail(&mut tail, publication, limits)?;
    report.page_count = reachable_page_count;
    let output_bytes = persistent_tail_total_len(base_len, tail.len(), limits)?;
    if output_bytes > limits.max_file_bytes {
        return Err(ImmutableError::Limit("output").into());
    }

    let mut largest_write_request = 0_usize;
    write_persistent_mixed_chunked(
        writer,
        data,
        options.max_write_request_bytes,
        &mut largest_write_request,
    )?;
    write_persistent_mixed_chunked(
        writer,
        &tail,
        options.max_write_request_bytes,
        &mut largest_write_request,
    )?;

    Ok(PersistentMixedStreamingReport {
        report,
        mode: PersistentBatchMode::CopyOnWriteDeletion,
        pages_written,
        pages_reused,
        base_bytes_written: u64_from_usize(base_len)?,
        tail_bytes_written: u64_from_usize(tail.len())?,
        largest_write_request,
        tail_allocation_bytes: tail.capacity(),
    })
}

#[cfg(test)]
mod persistent_delete_streaming_tests {
    use super::*;

    fn objects(count: usize) -> Vec<ImmutableObjectInput> {
        (1..=u64::try_from(count).expect("count"))
            .map(|object_id| ImmutableObjectInput::new(object_id, 1, vec![object_id as u8]))
            .collect()
    }

    fn assert_streamed_matches_owned(base: &[u8], object_id: u64, limits: ImmutableLimits) {
        let owned = append_persistent_delete(base, object_id, limits).expect("owned deletion");
        let mut streamed = Vec::new();
        let report = append_persistent_delete_to(
            &mut streamed,
            base,
            object_id,
            limits,
            PersistentMixedStreamingOptions {
                max_write_request_bytes: 37,
            },
        )
        .expect("streamed deletion");
        assert_eq!(streamed, owned.bytes);
        assert_eq!(report.report, owned.report);
        assert_eq!(report.mode, PersistentBatchMode::CopyOnWriteDeletion);
        assert_eq!(report.pages_written, owned.pages_written);
        assert_eq!(report.pages_reused, owned.pages_reused);
        assert_eq!(
            report.base_bytes_written,
            u64_from_usize(base.len()).expect("base bytes")
        );
        assert_eq!(
            report.tail_bytes_written,
            u64_from_usize(streamed.len() - base.len()).expect("tail bytes")
        );
        assert!(report.largest_write_request <= 37);
        assert!(report.tail_allocation_bytes < streamed.len());
    }

    #[test]
    fn streamed_root_leaf_deletion_matches_owned() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&objects(10), limits).expect("base");
        assert_streamed_matches_owned(&base, 5, limits);
    }

    #[test]
    fn streamed_no_underflow_and_left_borrow_match_owned() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&objects(400), limits).expect("base");
        assert_streamed_matches_owned(&base, 10, limits);

        let count = LEAF_CAPACITY + 2;
        let base = build_genesis(&objects(count), limits).expect("borrow base");
        assert_streamed_matches_owned(
            &base,
            u64::try_from(count).expect("count"),
            limits,
        );
    }

    #[test]
    fn streamed_merge_and_root_collapse_match_owned() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&objects(2 * LEAF_MIN_OCCUPANCY), limits).expect("merge base");
        assert_streamed_matches_owned(&base, 1, limits);
    }

    #[test]
    fn invalid_deletions_fail_before_output() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&objects(8), limits).expect("base");
        for object_id in [0, 99] {
            let mut sink = Vec::new();
            assert!(append_persistent_delete_to(
                &mut sink,
                &base,
                object_id,
                limits,
                PersistentMixedStreamingOptions::default(),
            )
            .is_err());
            assert!(sink.is_empty());
        }
        let one = build_genesis(&objects(1), limits).expect("one");
        let mut sink = Vec::new();
        assert!(append_persistent_delete_to(
            &mut sink,
            &one,
            1,
            limits,
            PersistentMixedStreamingOptions::default(),
        )
        .is_err());
        assert!(sink.is_empty());
    }
}
