#[derive(Clone, Debug)]
enum OriginalMixedPageBody {
    Leaf(Vec<Locator>),
    Internal(Vec<PageRef>),
}

#[derive(Clone, Debug)]
struct OriginalMixedPage {
    reference: PageRef,
    body: OriginalMixedPageBody,
}

fn collect_original_mixed_pages(
    data: &[u8],
    reference: &PageRef,
    levels: &mut [Vec<OriginalMixedPage>],
    limits: ImmutableLimits,
    visited: &mut usize,
) -> Result<(), ImmutableError> {
    if *visited >= limits.max_pages {
        return Err(ImmutableError::Limit("page count"));
    }
    *visited += 1;
    let page = checked_persistent_page(data, reference)?;
    let level = usize::from(reference.level);
    let target = levels
        .get_mut(level)
        .ok_or(ImmutableError::Invalid("mixed page level"))?;
    if reference.level == 0 {
        target.push(OriginalMixedPage {
            reference: reference.clone(),
            body: OriginalMixedPageBody::Leaf(decode_persistent_leaf(
                page, reference, limits,
            )?),
        });
        return Ok(());
    }

    let children = decode_persistent_children(page, reference, limits)?;
    target.push(OriginalMixedPage {
        reference: reference.clone(),
        body: OriginalMixedPageBody::Internal(children.clone()),
    });
    for child in &children {
        collect_original_mixed_pages(data, child, levels, limits, visited)?;
    }
    Ok(())
}

fn reusable_mixed_leaf(
    originals: &[Vec<OriginalMixedPage>],
    entries: &[Locator],
) -> Option<PageRef> {
    originals.first()?.iter().find_map(|page| match &page.body {
        OriginalMixedPageBody::Leaf(original) if original.as_slice() == entries => {
            Some(page.reference.clone())
        }
        _ => None,
    })
}

fn reusable_mixed_internal(
    originals: &[Vec<OriginalMixedPage>],
    level: u8,
    children: &[PageRef],
) -> Option<PageRef> {
    originals
        .get(usize::from(level))?
        .iter()
        .find_map(|page| match &page.body {
            OriginalMixedPageBody::Internal(original) if original.as_slice() == children => {
                Some(page.reference.clone())
            }
            _ => None,
        })
}

fn canonical_mixed_operation_order(
    operations: &[ImmutableBatchOperation],
    previous: &InternalReport,
    limits: ImmutableLimits,
) -> Result<Vec<usize>, ImmutableError> {
    if operations.len() < 2
        || !operations
            .iter()
            .any(|operation| matches!(operation, ImmutableBatchOperation::Delete(_)))
    {
        return Err(ImmutableError::Invalid("persistent mixed batch"));
    }
    allocation_check::<usize>(operations.len(), limits)?;
    let mut order: Vec<usize> = (0..operations.len()).collect();
    order.sort_unstable_by_key(|index| operations[*index].object_id());
    if let Some(pair) = order.windows(2).find(|pair| {
        operations[pair[0]].object_id() == operations[pair[1]].object_id()
    }) {
        return Err(ImmutableError::DuplicateObject(
            operations[pair[0]].object_id(),
        ));
    }

    let mut insertions = 0_usize;
    let mut deletions = 0_usize;
    for index in &order {
        match &operations[*index] {
            ImmutableBatchOperation::Put(input) => {
                if input.object_id == 0 || input.kind == 0 {
                    return Err(ImmutableError::Invalid("object input"));
                }
                if previous
                    .locators
                    .binary_search_by_key(&input.object_id, |locator| locator.object_id)
                    .is_err()
                {
                    insertions = insertions
                        .checked_add(1)
                        .ok_or(ImmutableError::Limit("object count"))?;
                }
            }
            ImmutableBatchOperation::Delete(object_id) => {
                if *object_id == 0
                    || previous
                        .locators
                        .binary_search_by_key(object_id, |locator| locator.object_id)
                        .is_err()
                {
                    return Err(ImmutableError::MissingObject(*object_id));
                }
                deletions = deletions
                    .checked_add(1)
                    .ok_or(ImmutableError::Limit("object count"))?;
            }
        }
    }
    let next_count = previous
        .locators
        .len()
        .checked_add(insertions)
        .and_then(|count| count.checked_sub(deletions))
        .ok_or(ImmutableError::Invalid("empty directory"))?;
    if next_count == 0 {
        return Err(ImmutableError::Invalid("empty directory"));
    }
    if next_count > limits.max_objects {
        return Err(ImmutableError::Limit("object count"));
    }
    allocation_check::<Locator>(next_count, limits)?;
    Ok(order)
}

fn apply_canonical_mixed_operations(
    output: &mut Vec<u8>,
    operations: &[ImmutableBatchOperation],
    order: &[usize],
    previous: &InternalReport,
    limits: ImmutableLimits,
) -> Result<Vec<Locator>, ImmutableError> {
    let mut locators = previous.locators.clone();
    for index in order {
        match &operations[*index] {
            ImmutableBatchOperation::Put(input) => {
                let replacement = append_object(output, input, limits)?;
                match locators
                    .binary_search_by_key(&input.object_id, |locator| locator.object_id)
                {
                    Ok(position) => locators[position] = replacement,
                    Err(position) => locators.insert(position, replacement),
                }
            }
            ImmutableBatchOperation::Delete(object_id) => {
                let position = locators
                    .binary_search_by_key(object_id, |locator| locator.object_id)
                    .map_err(|_| ImmutableError::MissingObject(*object_id))?;
                locators.remove(position);
            }
        }
    }
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableError::Invalid("persistent mixed locator order"));
    }
    Ok(locators)
}

fn materialize_canonical_mixed_tree(
    output: &mut Vec<u8>,
    locators: &[Locator],
    originals: &[Vec<OriginalMixedPage>],
    limits: ImmutableLimits,
) -> Result<(PageRef, usize, usize), ImmutableError> {
    let leaf_sizes = canonical_group_sizes(
        locators.len(),
        LEAF_CAPACITY,
        LEAF_MIN_OCCUPANCY,
        limits,
    )?;
    allocation_check::<PageRef>(leaf_sizes.len(), limits)?;
    let mut pages_written = 0_usize;
    let mut pages_reused = 0_usize;
    let mut level = Vec::with_capacity(leaf_sizes.len());
    let mut start = 0_usize;
    for size in leaf_sizes {
        let end = start
            .checked_add(size)
            .ok_or(ImmutableError::Limit("object count"))?;
        let entries = &locators[start..end];
        if let Some(reference) = reusable_mixed_leaf(originals, entries) {
            pages_reused += 1;
            level.push(reference);
        } else {
            level.push(append_cow_page(
                output,
                &encode_leaf(entries)?,
                limits,
                &mut pages_written,
            )?);
        }
        start = end;
    }

    while level.len() > 1 {
        let parent_level = level[0]
            .level
            .checked_add(1)
            .ok_or(ImmutableError::Limit("page depth"))?;
        if parent_level > limits.max_depth {
            return Err(ImmutableError::Limit("page depth"));
        }
        let group_sizes = canonical_group_sizes(
            level.len(),
            INTERNAL_FANOUT,
            INTERNAL_MIN_OCCUPANCY,
            limits,
        )?;
        allocation_check::<PageRef>(group_sizes.len(), limits)?;
        let mut next = Vec::with_capacity(group_sizes.len());
        let mut start = 0_usize;
        for size in group_sizes {
            let end = start
                .checked_add(size)
                .ok_or(ImmutableError::Limit("page count"))?;
            let children = &level[start..end];
            if let Some(reference) = reusable_mixed_internal(originals, parent_level, children) {
                pages_reused += 1;
                next.push(reference);
            } else {
                next.push(append_cow_page(
                    output,
                    &encode_internal(children, parent_level)?,
                    limits,
                    &mut pages_written,
                )?);
            }
            start = end;
        }
        level = next;
    }

    Ok((
        level
            .pop()
            .ok_or(ImmutableError::Invalid("persistent mixed root"))?,
        pages_written,
        pages_reused,
    ))
}

fn append_persistent_mixed_from_previous(
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    previous: InternalReport,
    limits: ImmutableLimits,
) -> Result<PersistentBatchResult, ImmutableError> {
    let order = canonical_mixed_operation_order(operations, &previous, limits)?;
    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut originals = vec![Vec::new(); usize::from(root.level) + 1];
    let mut visited = 0_usize;
    collect_original_mixed_pages(
        data,
        &root,
        &mut originals,
        limits,
        &mut visited,
    )?;
    if visited != previous.public.page_count {
        return Err(ImmutableError::Invalid("persistent mixed page inventory"));
    }

    let mut output = data.to_vec();
    let locators = apply_canonical_mixed_operations(
        &mut output,
        operations,
        &order,
        &previous,
        limits,
    )?;
    let (next_root, pages_written, pages_reused) = materialize_canonical_mixed_tree(
        &mut output,
        &locators,
        &originals,
        limits,
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
    if report.object_count != locators.len()
        || report.page_count
            != pages_written
                .checked_add(pages_reused)
                .ok_or(ImmutableError::Limit("page count"))?
    {
        return Err(ImmutableError::Invalid("persistent mixed accounting"));
    }
    Ok(PersistentBatchResult {
        bytes: output,
        report,
        mode: PersistentBatchMode::CopyOnWriteCanonicalMixed,
        pages_written,
        pages_reused,
    })
}

/// Appends a deterministic persistent batch containing at least one deletion and at least one other
/// operation.
///
/// The complete operation set is applied in canonical identifier order. Final leaves and internal
/// levels use the canonical full-construction grouping rule. A current page is reused only when its
/// complete locator or ordered child-reference sequence is exactly equal to the final page body.
/// This path therefore avoids false reuse while moving mixed deletion batches off the object-and-page
/// full rebuild baseline.
pub fn append_persistent_mixed_batch(
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
) -> Result<PersistentBatchResult, ImmutableError> {
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
    let previous = validate_canonical_internal(data, limits)?;
    append_persistent_mixed_from_previous(data, operations, previous, limits)
}
