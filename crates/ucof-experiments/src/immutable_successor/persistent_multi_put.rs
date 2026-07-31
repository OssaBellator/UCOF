fn merge_leaf_puts(
    existing: Vec<Locator>,
    updates: &[Locator],
    limits: ImmutableLimits,
) -> Result<Vec<Locator>, ImmutableError> {
    let capacity = existing
        .len()
        .checked_add(updates.len())
        .ok_or(ImmutableError::Limit("object count"))?;
    allocation_check::<Locator>(capacity, limits)?;
    let mut merged = Vec::with_capacity(capacity);
    let mut existing_index = 0_usize;
    let mut update_index = 0_usize;

    while existing_index < existing.len() || update_index < updates.len() {
        match (existing.get(existing_index), updates.get(update_index)) {
            (Some(current), Some(update)) if current.object_id < update.object_id => {
                merged.push(current.clone());
                existing_index += 1;
            }
            (Some(current), Some(update)) if current.object_id == update.object_id => {
                merged.push(update.clone());
                existing_index += 1;
                update_index += 1;
            }
            (Some(_), Some(update)) => {
                merged.push(update.clone());
                update_index += 1;
            }
            (Some(current), None) => {
                merged.push(current.clone());
                existing_index += 1;
            }
            (None, Some(update)) => {
                merged.push(update.clone());
                update_index += 1;
            }
            (None, None) => break,
        }
    }

    if merged.is_empty()
        || merged
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableError::Invalid("persistent put merge"));
    }
    Ok(merged)
}

fn append_leaf_groups(
    output: &mut Vec<u8>,
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
        references.push(append_cow_page(
            output,
            &encode_leaf(&entries[start..end])?,
            limits,
            pages_written,
        )?);
        start = end;
    }
    Ok(references)
}

fn append_internal_groups(
    output: &mut Vec<u8>,
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
        references.push(append_cow_page(
            output,
            &encode_internal(&children[start..end], level)?,
            limits,
            pages_written,
        )?);
        start = end;
    }
    Ok(references)
}

fn route_put_updates(children: &[PageRef], updates: &[Locator]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::with_capacity(children.len());
    let mut start = 0_usize;
    for (child_index, child) in children.iter().enumerate() {
        let mut end = start;
        while end < updates.len() {
            let is_last = child_index + 1 == children.len();
            if !is_last && updates[end].object_id > child.maximum {
                break;
            }
            end += 1;
        }
        ranges.push((start, end));
        start = end;
    }
    ranges
}

fn rewrite_put_paths(
    data: &[u8],
    output: &mut Vec<u8>,
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
        return append_leaf_groups(output, &merged, limits, pages_written);
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
            rewritten.extend(rewrite_put_paths(
                data,
                output,
                child,
                &updates[start..end],
                limits,
                pages_written,
                touched_original,
            )?);
        }
    }
    append_internal_groups(
        output,
        &rewritten,
        reference.level,
        limits,
        pages_written,
    )
}

fn finish_put_roots(
    output: &mut Vec<u8>,
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
        roots = append_internal_groups(output, &roots, level, limits, pages_written)?;
    }
    roots
        .pop()
        .ok_or(ImmutableError::Invalid("persistent put root"))
}

fn append_persistent_put_refs_from_previous(
    data: &[u8],
    inputs: &[&ImmutableObjectInput],
    previous: InternalReport,
    limits: ImmutableLimits,
) -> Result<PersistentBatchResult, ImmutableError> {
    if inputs.is_empty() {
        return Err(ImmutableError::Invalid("batch operations"));
    }
    allocation_check::<usize>(inputs.len(), limits)?;
    let mut order: Vec<usize> = (0..inputs.len()).collect();
    order.sort_unstable_by_key(|index| inputs[*index].object_id);
    if let Some(pair) = order
        .windows(2)
        .find(|pair| inputs[pair[0]].object_id == inputs[pair[1]].object_id)
    {
        return Err(ImmutableError::DuplicateObject(inputs[pair[0]].object_id));
    }

    let mut absent = 0_usize;
    for index in &order {
        let input = inputs[*index];
        if input.object_id == 0 || input.kind == 0 {
            return Err(ImmutableError::Invalid("object input"));
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
        return Err(ImmutableError::Limit("object count"));
    }

    allocation_check::<Locator>(inputs.len(), limits)?;
    let mut output = data.to_vec();
    let mut updates = Vec::with_capacity(inputs.len());
    for index in order {
        updates.push(append_object(&mut output, inputs[index], limits)?);
    }

    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut pages_written = 0_usize;
    let mut touched_original = 0_usize;
    let roots = rewrite_put_paths(
        data,
        &mut output,
        &root,
        &updates,
        limits,
        &mut pages_written,
        &mut touched_original,
    )?;
    let next_root = finish_put_roots(
        &mut output,
        roots,
        root.level,
        limits,
        &mut pages_written,
    )?;

    publish(
        &mut output,
        previous
            .public
            .sequence
            .checked_add(1)
            .ok_or(ImmutableError::Limit("sequence"))?,
        &next_root,
        previous.public.snapshot_digest,
        u64_from_usize(previous.footer_offset)?,
        pages_written,
        limits,
    )?;
    let report = validate_canonical_occupancy(&output, limits)?;
    let pages_reused = previous
        .public
        .page_count
        .checked_sub(touched_original)
        .ok_or(ImmutableError::Invalid("persistent page accounting"))?;
    Ok(PersistentBatchResult {
        bytes: output,
        report,
        mode: PersistentBatchMode::CopyOnWritePutBatch,
        pages_written,
        pages_reused,
    })
}

/// Appends a canonical batch of insertions and replacements through shared copy-on-write paths.
pub fn append_persistent_put_batch(
    data: &[u8],
    inputs: &[ImmutableObjectInput],
    limits: ImmutableLimits,
) -> Result<PersistentBatchResult, ImmutableError> {
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
    allocation_check::<&ImmutableObjectInput>(inputs.len(), limits)?;
    let input_refs: Vec<&ImmutableObjectInput> = inputs.iter().collect();
    let previous = validate_canonical_internal(data, limits)?;
    append_persistent_put_refs_from_previous(data, &input_refs, previous, limits)
}
