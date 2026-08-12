use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersistentBatchMode {
    /// Existing identifiers only: append new objects and rewrite affected leaf-to-root paths.
    CopyOnWriteReplacements,
    /// One absent identifier: append its object and propagate deterministic splits through one
    /// leaf-to-root path.
    CopyOnWriteInsertion,
    /// Multiple insertions and replacements: rewrite each affected page once and share ancestors.
    CopyOnWritePutBatch,
    /// One active identifier: rewrite its path, repair underflow, and collapse the root when needed.
    CopyOnWriteDeletion,
    /// A batch containing deletion plus another operation: apply the complete operation set,
    /// canonically regroup final pages, and reuse only exact current page bodies.
    CopyOnWriteCanonicalMixed,
    /// Reserved deterministic full-rebuild fallback for unsupported future operation shapes.
    FullRebuildShapeChange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentBatchResult {
    pub bytes: Vec<u8>,
    pub report: ImmutableReport,
    pub mode: PersistentBatchMode,
    pub pages_written: usize,
    pub pages_reused: usize,
}

fn page_has_replacement(
    replacements: &BTreeMap<u64, Locator>,
    minimum: u64,
    maximum: u64,
) -> bool {
    replacements.range(minimum..=maximum).next().is_some()
}

fn append_cow_page(
    output: &mut Vec<u8>,
    page: &[u8],
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<PageRef, ImmutableError> {
    if *pages_written >= limits.max_pages {
        return Err(ImmutableError::Limit("page count"));
    }
    let reference = append_page(output, page, limits)?;
    *pages_written += 1;
    Ok(reference)
}

fn rewrite_replacement_path(
    data: &[u8],
    output: &mut Vec<u8>,
    reference: &PageRef,
    replacements: &BTreeMap<u64, Locator>,
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<(PageRef, bool), ImmutableError> {
    if !page_has_replacement(replacements, reference.minimum, reference.maximum) {
        return Ok((reference.clone(), false));
    }

    let offset = usize_from_u64(reference.offset, "persistent page")?;
    let page = checked_range(data, offset, PAGE_SIZE, "persistent page")?;
    if digest(&[PAGE_DOMAIN, page]) != reference.digest
        || &page[..8] != PAGE_MAGIC
        || page[9] != reference.level
        || u64_at(page, 20, "persistent page")? != reference.minimum
        || u64_at(page, 28, "persistent page")? != reference.maximum
    {
        return Err(ImmutableError::Invalid("persistent page reference"));
    }

    let count = usize::try_from(u32_at(page, 12, "persistent page")?)
        .map_err(|_| ImmutableError::Invalid("persistent page count"))?;
    match page[8] {
        1 => {
            if reference.level != 0
                || count == 0
                || count > LEAF_CAPACITY
                || usize::try_from(u32_at(page, 16, "persistent leaf")?)
                    .map_err(|_| ImmutableError::Invalid("persistent leaf"))?
                    != LEAF_ENTRY_LEN
            {
                return Err(ImmutableError::Invalid("persistent leaf"));
            }
            allocation_check::<Locator>(count, limits)?;
            let mut entries = Vec::with_capacity(count);
            let mut changed = false;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                let locator = Locator {
                    object_id: u64_at(page, entry, "persistent leaf entry")?,
                    kind: u16_at(page, entry + 8, "persistent leaf entry")?,
                    record_offset: u64_at(page, entry + 16, "persistent leaf entry")?,
                    record_len: u64_at(page, entry + 24, "persistent leaf entry")?,
                    logical_len: u64_at(page, entry + 32, "persistent leaf entry")?,
                    digest: array(page, entry + 40, "persistent leaf entry")?,
                };
                if let Some(replacement) = replacements.get(&locator.object_id) {
                    entries.push(replacement.clone());
                    changed = true;
                } else {
                    entries.push(locator);
                }
            }
            if !changed {
                return Err(ImmutableError::Invalid("persistent replacement routing"));
            }
            let rewritten = encode_leaf(&entries)?;
            Ok((
                append_cow_page(output, &rewritten, limits, pages_written)?,
                true,
            ))
        }
        2 => {
            if reference.level == 0
                || count == 0
                || count > INTERNAL_FANOUT
                || usize::try_from(u32_at(page, 16, "persistent internal")?)
                    .map_err(|_| ImmutableError::Invalid("persistent internal"))?
                    != INTERNAL_ENTRY_LEN
            {
                return Err(ImmutableError::Invalid("persistent internal"));
            }
            allocation_check::<PageRef>(count, limits)?;
            let mut children = Vec::with_capacity(count);
            let mut changed = false;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                let child = PageRef {
                    minimum: u64_at(page, entry, "persistent child")?,
                    maximum: u64_at(page, entry + 8, "persistent child")?,
                    offset: u64_at(page, entry + 16, "persistent child")?,
                    level: reference.level - 1,
                    digest: array(page, entry + 32, "persistent child")?,
                };
                let (next, child_changed) = rewrite_replacement_path(
                    data,
                    output,
                    &child,
                    replacements,
                    limits,
                    pages_written,
                )?;
                children.push(next);
                changed |= child_changed;
            }
            if !changed {
                return Err(ImmutableError::Invalid("persistent replacement routing"));
            }
            let rewritten = encode_internal(&children, reference.level)?;
            Ok((
                append_cow_page(output, &rewritten, limits, pages_written)?,
                true,
            ))
        }
        _ => Err(ImmutableError::Invalid("persistent page kind")),
    }
}

/// Appends a deterministic batch while preserving unchanged page identities where the selected
/// persistent algorithm supports the operation shape.
///
/// Replacement-only batches use copy-on-write leaf-to-root updates at arbitrary depth. One absent
/// `Put` uses persistent insertion and split propagation. Multi-operation `Put` batches use one
/// shared path planner. One `Delete` uses persistent underflow repair and root collapse. Batches
/// combining deletion with another operation apply all changes canonically and reuse only exact
/// current leaf or internal page bodies.
pub fn append_persistent_batch(
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
) -> Result<PersistentBatchResult, ImmutableError> {
    if operations.is_empty() {
        return Err(ImmutableError::Invalid("batch operations"));
    }
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
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
        ));
    }

    if operations.len() > 1
        && operations
            .iter()
            .any(|operation| matches!(operation, ImmutableBatchOperation::Delete(_)))
    {
        return append_persistent_mixed_from_previous(data, operations, previous, limits);
    }

    if operations.len() == 1 {
        match &operations[order[0]] {
            ImmutableBatchOperation::Put(input)
                if previous
                    .locators
                    .binary_search_by_key(&input.object_id, |locator| locator.object_id)
                    .is_err() =>
            {
                return append_persistent_insert_from_previous(data, input, previous, limits);
            }
            ImmutableBatchOperation::Delete(object_id) => {
                return append_persistent_delete_from_previous(
                    data,
                    *object_id,
                    previous,
                    limits,
                );
            }
            ImmutableBatchOperation::Put(_) => {}
        }
    }

    let all_puts = order
        .iter()
        .all(|index| matches!(operations[*index], ImmutableBatchOperation::Put(_)));
    let any_insertion = all_puts
        && order.iter().any(|index| match &operations[*index] {
            ImmutableBatchOperation::Put(input) => previous
                .locators
                .binary_search_by_key(&input.object_id, |locator| locator.object_id)
                .is_err(),
            ImmutableBatchOperation::Delete(_) => false,
        });
    if any_insertion {
        allocation_check::<&ImmutableObjectInput>(operations.len(), limits)?;
        let inputs: Vec<&ImmutableObjectInput> = order
            .iter()
            .filter_map(|index| match &operations[*index] {
                ImmutableBatchOperation::Put(input) => Some(input),
                ImmutableBatchOperation::Delete(_) => None,
            })
            .collect();
        return append_persistent_put_refs_from_previous(data, &inputs, previous, limits);
    }

    let replacement_only = order.iter().all(|index| match &operations[*index] {
        ImmutableBatchOperation::Put(input) => {
            input.object_id != 0
                && input.kind != 0
                && previous
                    .locators
                    .binary_search_by_key(&input.object_id, |locator| locator.object_id)
                    .is_ok()
        }
        ImmutableBatchOperation::Delete(_) => false,
    });

    if !replacement_only {
        let bytes = append_batch(data, operations, limits)?;
        let report = validate_canonical_occupancy(&bytes, limits)?;
        return Ok(PersistentBatchResult {
            pages_written: report.page_count,
            pages_reused: 0,
            bytes,
            report,
            mode: PersistentBatchMode::FullRebuildShapeChange,
        });
    }

    allocation_check::<Locator>(operations.len(), limits)?;
    let mut output = data.to_vec();
    let mut replacements = BTreeMap::new();
    for index in order {
        let ImmutableBatchOperation::Put(input) = &operations[index] else {
            return Err(ImmutableError::Invalid("persistent batch state"));
        };
        let locator = append_object(&mut output, input, limits)?;
        replacements.insert(input.object_id, locator);
    }

    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut pages_written = 0_usize;
    let (next_root, changed) = rewrite_replacement_path(
        data,
        &mut output,
        &root,
        &replacements,
        limits,
        &mut pages_written,
    )?;
    if !changed || pages_written == 0 {
        return Err(ImmutableError::Invalid("persistent batch state"));
    }

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
        .checked_sub(pages_written)
        .ok_or(ImmutableError::Invalid("persistent page accounting"))?;
    Ok(PersistentBatchResult {
        bytes: output,
        report,
        mode: PersistentBatchMode::CopyOnWriteReplacements,
        pages_written,
        pages_reused,
    })
}
