fn decode_persistent_leaf(
    page: &[u8],
    reference: &PageRef,
    limits: ImmutableLimits,
) -> Result<Vec<Locator>, ImmutableError> {
    let count = usize::try_from(u32_at(page, 12, "persistent leaf count")?)
        .map_err(|_| ImmutableError::Invalid("persistent leaf count"))?;
    if page[8] != 1
        || reference.level != 0
        || page[9] != 0
        || count == 0
        || count > LEAF_CAPACITY
        || usize::try_from(u32_at(page, 16, "persistent leaf entry size")?)
            .map_err(|_| ImmutableError::Invalid("persistent leaf entry size"))?
            != LEAF_ENTRY_LEN
    {
        return Err(ImmutableError::Invalid("persistent leaf"));
    }
    allocation_check::<Locator>(count, limits)?;
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
        entries.push(Locator {
            object_id: u64_at(page, entry, "persistent leaf entry")?,
            kind: u16_at(page, entry + 8, "persistent leaf entry")?,
            record_offset: u64_at(page, entry + 16, "persistent leaf entry")?,
            record_len: u64_at(page, entry + 24, "persistent leaf entry")?,
            logical_len: u64_at(page, entry + 32, "persistent leaf entry")?,
            digest: array(page, entry + 40, "persistent leaf entry")?,
        });
    }
    if entries
        .windows(2)
        .any(|pair| pair[0].object_id >= pair[1].object_id)
        || entries.first().map(|entry| entry.object_id) != Some(reference.minimum)
        || entries.last().map(|entry| entry.object_id) != Some(reference.maximum)
    {
        return Err(ImmutableError::Invalid("persistent leaf order"));
    }
    Ok(entries)
}

fn decode_persistent_children(
    page: &[u8],
    reference: &PageRef,
    limits: ImmutableLimits,
) -> Result<Vec<PageRef>, ImmutableError> {
    let count = usize::try_from(u32_at(page, 12, "persistent internal count")?)
        .map_err(|_| ImmutableError::Invalid("persistent internal count"))?;
    if page[8] != 2
        || reference.level == 0
        || page[9] != reference.level
        || count == 0
        || count > INTERNAL_FANOUT
        || usize::try_from(u32_at(page, 16, "persistent internal entry size")?)
            .map_err(|_| ImmutableError::Invalid("persistent internal entry size"))?
            != INTERNAL_ENTRY_LEN
    {
        return Err(ImmutableError::Invalid("persistent internal"));
    }
    allocation_check::<PageRef>(count, limits)?;
    let mut children = Vec::with_capacity(count);
    for index in 0..count {
        let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
        children.push(PageRef {
            minimum: u64_at(page, entry, "persistent child")?,
            maximum: u64_at(page, entry + 8, "persistent child")?,
            offset: u64_at(page, entry + 16, "persistent child")?,
            level: reference.level - 1,
            digest: array(page, entry + 32, "persistent child")?,
        });
    }
    if children
        .windows(2)
        .any(|pair| pair[0].maximum >= pair[1].minimum)
        || children.first().map(|child| child.minimum) != Some(reference.minimum)
        || children.last().map(|child| child.maximum) != Some(reference.maximum)
    {
        return Err(ImmutableError::Invalid("persistent child order"));
    }
    Ok(children)
}

fn checked_persistent_page<'a>(
    data: &'a [u8],
    reference: &PageRef,
) -> Result<&'a [u8], ImmutableError> {
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
    Ok(page)
}

fn insert_persistent_path(
    data: &[u8],
    output: &mut Vec<u8>,
    reference: &PageRef,
    inserted: &Locator,
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<Vec<PageRef>, ImmutableError> {
    let page = checked_persistent_page(data, reference)?;
    if reference.level == 0 {
        let mut entries = decode_persistent_leaf(page, reference, limits)?;
        let position = entries
            .binary_search_by_key(&inserted.object_id, |entry| entry.object_id)
            .unwrap_or_else(|position| position);
        if entries
            .get(position)
            .is_some_and(|entry| entry.object_id == inserted.object_id)
        {
            return Err(ImmutableError::DuplicateObject(inserted.object_id));
        }
        entries.insert(position, inserted.clone());
        if entries.len() <= LEAF_CAPACITY {
            return Ok(vec![append_cow_page(
                output,
                &encode_leaf(&entries)?,
                limits,
                pages_written,
            )?]);
        }
        let split = (entries.len() + 1) / 2;
        let left = append_cow_page(
            output,
            &encode_leaf(&entries[..split])?,
            limits,
            pages_written,
        )?;
        let right = append_cow_page(
            output,
            &encode_leaf(&entries[split..])?,
            limits,
            pages_written,
        )?;
        return Ok(vec![left, right]);
    }

    let children = decode_persistent_children(page, reference, limits)?;
    let child_index = children
        .iter()
        .position(|child| inserted.object_id <= child.maximum)
        .unwrap_or(children.len() - 1);
    let replacements = insert_persistent_path(
        data,
        output,
        &children[child_index],
        inserted,
        limits,
        pages_written,
    )?;
    let updated_len = children
        .len()
        .checked_sub(1)
        .and_then(|count| count.checked_add(replacements.len()))
        .ok_or(ImmutableError::Limit("page count"))?;
    allocation_check::<PageRef>(updated_len, limits)?;
    let mut updated = Vec::with_capacity(updated_len);
    updated.extend_from_slice(&children[..child_index]);
    updated.extend(replacements);
    updated.extend_from_slice(&children[child_index + 1..]);

    if updated.len() <= INTERNAL_FANOUT {
        return Ok(vec![append_cow_page(
            output,
            &encode_internal(&updated, reference.level)?,
            limits,
            pages_written,
        )?]);
    }
    let split = (updated.len() + 1) / 2;
    let left = append_cow_page(
        output,
        &encode_internal(&updated[..split], reference.level)?,
        limits,
        pages_written,
    )?;
    let right = append_cow_page(
        output,
        &encode_internal(&updated[split..], reference.level)?,
        limits,
        pages_written,
    )?;
    Ok(vec![left, right])
}

fn append_persistent_insert_from_previous(
    data: &[u8],
    input: &ImmutableObjectInput,
    previous: InternalReport,
    limits: ImmutableLimits,
) -> Result<PersistentBatchResult, ImmutableError> {
    if input.object_id == 0 || input.kind == 0 {
        return Err(ImmutableError::Invalid("object input"));
    }
    if previous.locators.len() >= limits.max_objects {
        return Err(ImmutableError::Limit("object count"));
    }
    if previous
        .locators
        .binary_search_by_key(&input.object_id, |locator| locator.object_id)
        .is_ok()
    {
        return Err(ImmutableError::DuplicateObject(input.object_id));
    }

    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let touched_pages = usize::from(root.level)
        .checked_add(1)
        .ok_or(ImmutableError::Limit("page depth"))?;
    let pages_reused = previous
        .public
        .page_count
        .checked_sub(touched_pages)
        .ok_or(ImmutableError::Invalid("persistent page accounting"))?;

    let mut output = data.to_vec();
    let inserted = append_object(&mut output, input, limits)?;
    let mut pages_written = 0_usize;
    let mut roots = insert_persistent_path(
        data,
        &mut output,
        &root,
        &inserted,
        limits,
        &mut pages_written,
    )?;
    let next_root = match roots.len() {
        1 => roots
            .pop()
            .ok_or(ImmutableError::Invalid("persistent insertion root"))?,
        2 => {
            let next_level = root
                .level
                .checked_add(1)
                .ok_or(ImmutableError::Limit("page depth"))?;
            if next_level > limits.max_depth {
                return Err(ImmutableError::Limit("page depth"));
            }
            append_cow_page(
                &mut output,
                &encode_internal(&roots, next_level)?,
                limits,
                &mut pages_written,
            )?
        }
        _ => return Err(ImmutableError::Invalid("persistent insertion root")),
    };

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
    let report = validate(&output, limits)?;
    Ok(PersistentBatchResult {
        bytes: output,
        report,
        mode: PersistentBatchMode::CopyOnWriteInsertion,
        pages_written,
        pages_reused,
    })
}

/// Appends one absent object through a persistent leaf-to-root path, including deterministic split
/// propagation and root-height increase.
pub fn append_persistent_insert(
    data: &[u8],
    input: &ImmutableObjectInput,
    limits: ImmutableLimits,
) -> Result<PersistentBatchResult, ImmutableError> {
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
    let previous = validate_internal(data, limits)?;
    append_persistent_insert_from_previous(data, input, previous, limits)
}
