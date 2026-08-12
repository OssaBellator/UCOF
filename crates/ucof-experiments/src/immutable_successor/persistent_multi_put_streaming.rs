fn append_leaf_tail_groups(
    tail: &mut Vec<u8>,
    base_len: usize,
    entries: &[Locator],
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<Vec<PageRef>, ImmutableError> {
    let sizes = canonical_group_sizes(
        entries.len(),
        LEAF_CAPACITY,
        LEAF_MIN_OCCUPANCY,
        limits,
    )?;
    allocation_check::<PageRef>(sizes.len(), limits)?;
    let mut references = Vec::with_capacity(sizes.len());
    let mut start = 0_usize;
    for size in sizes {
        let end = start
            .checked_add(size)
            .ok_or(ImmutableError::Limit("object count"))?;
        references.push(append_persistent_tail_page(
            tail,
            base_len,
            &encode_leaf(&entries[start..end])?,
            limits,
            pages_written,
        )?);
        start = end;
    }
    Ok(references)
}

fn append_internal_tail_groups(
    tail: &mut Vec<u8>,
    base_len: usize,
    children: &[PageRef],
    level: u8,
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<Vec<PageRef>, ImmutableError> {
    let sizes = canonical_group_sizes(
        children.len(),
        INTERNAL_FANOUT,
        INTERNAL_MIN_OCCUPANCY,
        limits,
    )?;
    allocation_check::<PageRef>(sizes.len(), limits)?;
    let mut references = Vec::with_capacity(sizes.len());
    let mut start = 0_usize;
    for size in sizes {
        let end = start
            .checked_add(size)
            .ok_or(ImmutableError::Limit("page count"))?;
        references.push(append_persistent_tail_page(
            tail,
            base_len,
            &encode_internal(&children[start..end], level)?,
            limits,
            pages_written,
        )?);
        start = end;
    }
    Ok(references)
}

#[allow(clippy::too_many_arguments)]
fn rewrite_put_tail_paths(
    data: &[u8],
    tail: &mut Vec<u8>,
    base_len: usize,
    reference: &PageRef,
    updates: &[Locator],
    limits: ImmutableLimits,
    pages_written: &mut usize,
    touched_original: &mut usize,
) -> Result<Vec<PageRef>, ImmutableError> {
    if updates.is_empty() {
        return Ok(vec![reference.clone()]);
    }
    increment_touched(touched_original, limits)?;
    let page = checked_persistent_page(data, reference)?;
    if reference.level == 0 {
        let existing = decode_persistent_leaf(page, reference, limits)?;
        let merged = merge_leaf_puts(existing, updates, limits)?;
        return append_leaf_tail_groups(tail, base_len, &merged, limits, pages_written);
    }

    let children = decode_persistent_children(page, reference, limits)?;
    let ranges = route_put_updates(&children, updates);
    let projected = children
        .len()
        .checked_add(updates.len())
        .ok_or(ImmutableError::Limit("page count"))?;
    allocation_check::<PageRef>(projected, limits)?;
    let mut rewritten = Vec::with_capacity(projected);
    for (index, child) in children.iter().enumerate() {
        let (start, end) = ranges[index];
        if start == end {
            rewritten.push(child.clone());
        } else {
            rewritten.extend(rewrite_put_tail_paths(
                data,
                tail,
                base_len,
                child,
                &updates[start..end],
                limits,
                pages_written,
                touched_original,
            )?);
        }
    }
    append_internal_tail_groups(
        tail,
        base_len,
        &rewritten,
        reference.level,
        limits,
        pages_written,
    )
}

fn finish_put_tail_roots(
    tail: &mut Vec<u8>,
    base_len: usize,
    mut roots: Vec<PageRef>,
    mut level: u8,
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<PageRef, ImmutableError> {
    while roots.len() > 1 {
        level = level
            .checked_add(1)
            .ok_or(ImmutableError::Limit("page depth"))?;
        if level > limits.max_depth {
            return Err(ImmutableError::Limit("page depth"));
        }
        roots = append_internal_tail_groups(
            tail,
            base_len,
            &roots,
            level,
            limits,
            pages_written,
        )?;
    }
    roots
        .pop()
        .ok_or(ImmutableError::Invalid("persistent put root"))
}

fn append_persistent_put_refs_to<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
    inputs: &[&ImmutableObjectInput],
    limits: ImmutableLimits,
    options: PersistentMixedStreamingOptions,
) -> Result<PersistentMixedStreamingReport, PersistentMixedStreamingError> {
    if options.max_write_request_bytes == 0 {
        return Err(ImmutableError::Invalid("write request").into());
    }
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output").into());
    }
    if inputs.is_empty() {
        return Err(ImmutableError::Invalid("batch operations").into());
    }

    let previous = validate_canonical_internal(data, limits)?;
    allocation_check::<usize>(inputs.len(), limits)?;
    let mut order: Vec<usize> = (0..inputs.len()).collect();
    order.sort_unstable_by_key(|index| inputs[*index].object_id);
    if let Some(pair) = order
        .windows(2)
        .find(|pair| inputs[pair[0]].object_id == inputs[pair[1]].object_id)
    {
        return Err(ImmutableError::DuplicateObject(inputs[pair[0]].object_id).into());
    }

    let mut absent = 0_usize;
    for index in &order {
        let input = inputs[*index];
        if input.object_id == 0 || input.kind == 0 {
            return Err(ImmutableError::Invalid("object input").into());
        }
        if previous
            .locators
            .binary_search_by_key(&input.object_id, |locator| locator.object_id)
            .is_err()
        {
            absent = absent
                .checked_add(1)
                .ok_or(ImmutableError::Limit("object count"))?;
        }
    }
    let next_object_count = previous
        .locators
        .len()
        .checked_add(absent)
        .ok_or(ImmutableError::Limit("object count"))?;
    if next_object_count > limits.max_objects {
        return Err(ImmutableError::Limit("object count").into());
    }

    let base_len = data.len();
    allocation_check::<Locator>(inputs.len(), limits)?;
    let mut tail = Vec::new();
    let mut updates = Vec::with_capacity(inputs.len());
    for index in order {
        updates.push(append_persistent_tail_object(
            &mut tail,
            base_len,
            inputs[index],
            limits,
        )?);
    }

    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut pages_written = 0_usize;
    let mut touched_original = 0_usize;
    let roots = rewrite_put_tail_paths(
        data,
        &mut tail,
        base_len,
        &root,
        &updates,
        limits,
        &mut pages_written,
        &mut touched_original,
    )?;
    let next_root = finish_put_tail_roots(
        &mut tail,
        base_len,
        roots,
        root.level,
        limits,
        &mut pages_written,
    )?;

    let pages_reused = previous
        .public
        .page_count
        .checked_sub(touched_original)
        .ok_or(ImmutableError::Invalid("persistent page accounting"))?;
    let reachable_page_count = pages_reused
        .checked_add(pages_written)
        .ok_or(ImmutableError::Limit("page count"))?;
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
        object_count: next_object_count,
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
        mode: PersistentBatchMode::CopyOnWritePutBatch,
        pages_written,
        pages_reused,
        base_bytes_written: u64_from_usize(base_len)?,
        tail_bytes_written: u64_from_usize(tail.len())?,
        largest_write_request,
        tail_allocation_bytes: tail.capacity(),
    })
}

/// Streams a canonical insertion/replacement batch as a verified base plus one append tail.
pub fn append_persistent_put_batch_to<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
    inputs: &[ImmutableObjectInput],
    limits: ImmutableLimits,
    options: PersistentMixedStreamingOptions,
) -> Result<PersistentMixedStreamingReport, PersistentMixedStreamingError> {
    allocation_check::<&ImmutableObjectInput>(inputs.len(), limits)?;
    let references: Vec<&ImmutableObjectInput> = inputs.iter().collect();
    append_persistent_put_refs_to(writer, data, &references, limits, options)
}

#[cfg(test)]
mod persistent_multi_put_streaming_tests {
    use super::*;

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

    fn assert_streamed_matches_owned(
        base: &[u8],
        inputs: &[ImmutableObjectInput],
        limits: ImmutableLimits,
    ) {
        let owned = append_persistent_put_batch(base, inputs, limits).expect("owned multi put");
        let mut streamed = Vec::new();
        let report = append_persistent_put_batch_to(
            &mut streamed,
            base,
            inputs,
            limits,
            PersistentMixedStreamingOptions {
                max_write_request_bytes: 41,
            },
        )
        .expect("streamed multi put");
        assert_eq!(streamed, owned.bytes);
        assert_eq!(report.report, owned.report);
        assert_eq!(report.mode, PersistentBatchMode::CopyOnWritePutBatch);
        assert_eq!(report.pages_written, owned.pages_written);
        assert_eq!(report.pages_reused, owned.pages_reused);
        assert!(report.largest_write_request <= 41);
        assert!(report.tail_allocation_bytes < streamed.len());
    }

    #[test]
    fn streamed_same_and_cross_leaf_puts_match_owned() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(400), limits).expect("base");
        assert_streamed_matches_owned(&base, &[object(617), object(619)], limits);
        assert_streamed_matches_owned(&base, &[object(371), object(617)], limits);
    }

    #[test]
    fn streamed_insert_and_replace_match_owned() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(400), limits).expect("base");
        let inputs = [
            ImmutableObjectInput::new(700, 31, b"replacement".to_vec()),
            object(701),
        ];
        assert_streamed_matches_owned(&base, &inputs, limits);
    }

    #[test]
    fn streamed_leaf_splits_and_root_growth_match_owned() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(400), limits).expect("base");
        assert_streamed_matches_owned(&base, &[object(1), object(3)], limits);

        let count = LEAF_CAPACITY
            .checked_mul(INTERNAL_FANOUT)
            .expect("full level-one tree");
        let base = build_genesis(&even_objects(count), limits).expect("full tree");
        let inputs = [
            object(1),
            object(u64::try_from(count).expect("count") * 2 + 1),
        ];
        assert_streamed_matches_owned(&base, &inputs, limits);
    }

    #[test]
    fn caller_order_is_canonical_and_invalid_batches_fail_before_output() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(400), limits).expect("base");
        let forward = [object(371), object(617), object(801)];
        let mut reverse = forward.clone();
        reverse.reverse();
        let mut first = Vec::new();
        let mut second = Vec::new();
        append_persistent_put_batch_to(
            &mut first,
            &base,
            &forward,
            limits,
            PersistentMixedStreamingOptions::default(),
        )
        .expect("forward");
        append_persistent_put_batch_to(
            &mut second,
            &base,
            &reverse,
            limits,
            PersistentMixedStreamingOptions::default(),
        )
        .expect("reverse");
        assert_eq!(first, second);

        for inputs in [Vec::new(), vec![object(801), object(801)]] {
            let mut sink = Vec::new();
            assert!(append_persistent_put_batch_to(
                &mut sink,
                &base,
                &inputs,
                limits,
                PersistentMixedStreamingOptions::default(),
            )
            .is_err());
            assert!(sink.is_empty());
        }
    }
}
