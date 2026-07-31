fn encode_object(input: &ImmutableObjectInput) -> Result<Vec<u8>, ImmutableError> {
    if input.object_id == 0 || input.kind == 0 {
        return Err(ImmutableError::Invalid("object input"));
    }
    let length = OBJECT_HEADER_LEN
        .checked_add(input.payload.len())
        .ok_or(ImmutableError::Limit("object size"))?;
    let mut record = vec![0_u8; length];
    record[..8].copy_from_slice(OBJECT_MAGIC);
    put_u16(
        &mut record,
        8,
        u16::try_from(OBJECT_HEADER_LEN).map_err(|_| ImmutableError::Limit("object header"))?,
    );
    put_u16(&mut record, 10, input.kind);
    put_u64(&mut record, 16, input.object_id);
    put_u64(&mut record, 24, u64_from_usize(input.payload.len())?);
    put_u64(&mut record, 32, u64_from_usize(input.payload.len())?);
    record[OBJECT_HEADER_LEN..].copy_from_slice(&input.payload);
    Ok(record)
}

fn append_object(
    output: &mut Vec<u8>,
    input: &ImmutableObjectInput,
    limits: ImmutableLimits,
) -> Result<Locator, ImmutableError> {
    let record = encode_object(input)?;
    if output
        .len()
        .checked_add(record.len())
        .ok_or(ImmutableError::Limit("output"))?
        > limits.max_output_bytes
    {
        return Err(ImmutableError::Limit("output"));
    }
    let offset = u64_from_usize(output.len())?;
    output.extend_from_slice(&record);
    Ok(Locator {
        object_id: input.object_id,
        kind: input.kind,
        record_offset: offset,
        record_len: u64_from_usize(record.len())?,
        logical_len: u64_from_usize(input.payload.len())?,
        digest: digest(&[OBJECT_DOMAIN, &record]),
    })
}

fn encode_leaf(entries: &[Locator]) -> Result<Vec<u8>, ImmutableError> {
    if entries.is_empty() || entries.len() > LEAF_CAPACITY {
        return Err(ImmutableError::Invalid("leaf input"));
    }
    if entries
        .windows(2)
        .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableError::Invalid("leaf input order"));
    }
    let mut page = vec![0_u8; PAGE_SIZE];
    page[..8].copy_from_slice(PAGE_MAGIC);
    page[8] = 1;
    put_u32(&mut page, 12, u32_from_usize(entries.len())?);
    put_u32(&mut page, 16, u32_from_usize(LEAF_ENTRY_LEN)?);
    put_u64(&mut page, 20, entries[0].object_id);
    put_u64(
        &mut page,
        28,
        entries
            .last()
            .ok_or(ImmutableError::Invalid("leaf input"))?
            .object_id,
    );
    for (index, entry) in entries.iter().enumerate() {
        let offset = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
        put_u64(&mut page, offset, entry.object_id);
        put_u16(&mut page, offset + 8, entry.kind);
        put_u64(&mut page, offset + 16, entry.record_offset);
        put_u64(&mut page, offset + 24, entry.record_len);
        put_u64(&mut page, offset + 32, entry.logical_len);
        page[offset + 40..offset + 72].copy_from_slice(&entry.digest);
    }
    Ok(page)
}

fn encode_internal(children: &[PageRef], level: u8) -> Result<Vec<u8>, ImmutableError> {
    if children.is_empty() || children.len() > INTERNAL_FANOUT || level == 0 {
        return Err(ImmutableError::Invalid("internal input"));
    }
    if children
        .windows(2)
        .any(|pair| pair[0].maximum >= pair[1].minimum)
        || children.iter().any(|child| child.level + 1 != level)
    {
        return Err(ImmutableError::Invalid("internal input order"));
    }
    let mut page = vec![0_u8; PAGE_SIZE];
    page[..8].copy_from_slice(PAGE_MAGIC);
    page[8] = 2;
    page[9] = level;
    put_u32(&mut page, 12, u32_from_usize(children.len())?);
    put_u32(&mut page, 16, u32_from_usize(INTERNAL_ENTRY_LEN)?);
    put_u64(&mut page, 20, children[0].minimum);
    put_u64(
        &mut page,
        28,
        children
            .last()
            .ok_or(ImmutableError::Invalid("internal input"))?
            .maximum,
    );
    for (index, child) in children.iter().enumerate() {
        let offset = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
        put_u64(&mut page, offset, child.minimum);
        put_u64(&mut page, offset + 8, child.maximum);
        put_u64(&mut page, offset + 16, child.offset);
        put_u64(&mut page, offset + 24, u64_from_usize(PAGE_SIZE)?);
        page[offset + 32..offset + 64].copy_from_slice(&child.digest);
    }
    Ok(page)
}

fn append_page(
    output: &mut Vec<u8>,
    page: &[u8],
    limits: ImmutableLimits,
) -> Result<PageRef, ImmutableError> {
    if output
        .len()
        .checked_add(PAGE_SIZE)
        .ok_or(ImmutableError::Limit("output"))?
        > limits.max_output_bytes
    {
        return Err(ImmutableError::Limit("output"));
    }
    let reference = PageRef {
        minimum: u64_at(page, 20, "page")?,
        maximum: u64_at(page, 28, "page")?,
        offset: u64_from_usize(output.len())?,
        level: page[9],
        digest: digest(&[PAGE_DOMAIN, page]),
    };
    output.extend_from_slice(page);
    Ok(reference)
}

fn build_tree(
    output: &mut Vec<u8>,
    locators: &mut [Locator],
    limits: ImmutableLimits,
) -> Result<(PageRef, usize), ImmutableError> {
    if locators.is_empty() || locators.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count"));
    }
    locators.sort_by_key(|locator| locator.object_id);
    if locators
        .windows(2)
        .any(|pair| pair[0].object_id == pair[1].object_id)
    {
        return Err(ImmutableError::DuplicateObject(
            locators
                .windows(2)
                .find(|pair| pair[0].object_id == pair[1].object_id)
                .map(|pair| pair[0].object_id)
                .unwrap_or(0),
        ));
    }

    let mut pages = 0_usize;
    let leaf_sizes = canonical_group_sizes(
        locators.len(),
        LEAF_CAPACITY,
        LEAF_MIN_OCCUPANCY,
        limits,
    )?;
    let mut level = Vec::with_capacity(leaf_sizes.len());
    allocation_check::<PageRef>(leaf_sizes.len(), limits)?;
    let mut start = 0_usize;
    for size in leaf_sizes {
        if pages >= limits.max_pages {
            return Err(ImmutableError::Limit("page count"));
        }
        let end = start
            .checked_add(size)
            .ok_or(ImmutableError::Limit("object count"))?;
        level.push(append_page(
            output,
            &encode_leaf(&locators[start..end])?,
            limits,
        )?);
        pages += 1;
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
            if pages >= limits.max_pages {
                return Err(ImmutableError::Limit("page count"));
            }
            let end = start
                .checked_add(size)
                .ok_or(ImmutableError::Limit("page count"))?;
            next.push(append_page(
                output,
                &encode_internal(&level[start..end], parent_level)?,
                limits,
            )?);
            pages += 1;
            start = end;
        }
        level = next;
    }
    Ok((
        level.pop().ok_or(ImmutableError::Invalid("empty tree"))?,
        pages,
    ))
}
