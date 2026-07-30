fn validate_internal(
    data: &[u8],
    limits: ImmutableLimits,
) -> Result<InternalReport, ImmutableError> {
    if data.len() > limits.max_file_bytes {
        return Err(ImmutableError::Limit("file size"));
    }
    if data.len() < FILE_HEADER_LEN + OBJECT_HEADER_LEN + PAGE_SIZE + SNAPSHOT_LEN + FOOTER_LEN {
        return Err(ImmutableError::Invalid("file length"));
    }
    if &data[..8] != FILE_MAGIC || data[8..FILE_HEADER_LEN].iter().any(|byte| *byte != 0) {
        return Err(ImmutableError::Invalid("header"));
    }

    let footer_offset = data.len() - FOOTER_LEN;
    let footer = parse_footer(data, footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot_len = usize_from_u64(footer.snapshot_len, "snapshot range")?;
    if snapshot_len != SNAPSHOT_LEN
        || snapshot_offset
            .checked_add(snapshot_len)
            .ok_or(ImmutableError::Invalid("snapshot range"))?
            != footer_offset
    {
        return Err(ImmutableError::Invalid("snapshot range"));
    }
    let snapshot = checked_range(data, snapshot_offset, snapshot_len, "snapshot")?;
    if digest(&[SNAPSHOT_DOMAIN, snapshot]) != footer.snapshot_digest {
        return Err(ImmutableError::Invalid("snapshot digest"));
    }
    if &snapshot[..8] != SNAPSHOT_MAGIC || u64_at(snapshot, 8, "snapshot")? != footer.sequence {
        return Err(ImmutableError::Invalid("snapshot"));
    }
    let parent_snapshot_digest = array::<32>(snapshot, 64, "snapshot parent")?;
    let commit_start = if footer.previous_footer_offset == ABSENT_OFFSET {
        if footer.sequence != 0 || parent_snapshot_digest.iter().any(|byte| *byte != 0) {
            return Err(ImmutableError::Invalid("genesis linkage"));
        }
        0
    } else {
        let previous_offset = usize_from_u64(footer.previous_footer_offset, "previous footer")?;
        let previous_end = previous_offset
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableError::Invalid("previous footer"))?;
        if previous_end > snapshot_offset {
            return Err(ImmutableError::Invalid("previous footer"));
        }
        let previous = parse_footer(data, previous_offset)?;
        if footer.sequence != previous.sequence + 1
            || previous.snapshot_digest != parent_snapshot_digest
        {
            return Err(ImmutableError::Invalid("parent linkage"));
        }
        previous_end
    };
    let semantics = footer_semantics(&footer);
    if digest(&[
        COMMIT_DOMAIN,
        &data[commit_start..footer_offset],
        &semantics,
    ]) != footer.commit_digest
    {
        return Err(ImmutableError::Invalid("commit digest"));
    }

    let root = root_reference(data, snapshot, limits)?;
    let mut seen = HashSet::new();
    let mut stack = vec![root.clone()];
    let mut locators = Vec::new();
    let mut structural_ranges = vec![
        (snapshot_offset, footer_offset),
        (footer_offset, data.len()),
    ];
    while let Some(reference) = stack.pop() {
        parse_page(
            data,
            &reference,
            snapshot_offset,
            limits,
            &mut seen,
            &mut stack,
            &mut locators,
            &mut structural_ranges,
        )?;
    }
    let current_pages = seen
        .iter()
        .filter(|offset| **offset >= commit_start)
        .count();
    if footer.page_count_current != u64_from_usize(current_pages)? {
        return Err(ImmutableError::Invalid("page count"));
    }
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
        || locators.first().map(|entry| entry.object_id) != Some(root.minimum)
        || locators.last().map(|entry| entry.object_id) != Some(root.maximum)
    {
        return Err(ImmutableError::Invalid("object order"));
    }

    let mut object_ranges = Vec::with_capacity(locators.len());
    allocation_check::<(usize, usize)>(locators.len(), limits)?;
    for locator in &locators {
        let offset = usize_from_u64(locator.record_offset, "object range")?;
        let length = usize_from_u64(locator.record_len, "object range")?;
        let end = offset
            .checked_add(length)
            .ok_or(ImmutableError::Invalid("object range"))?;
        if offset < FILE_HEADER_LEN || end > snapshot_offset {
            return Err(ImmutableError::Invalid("object range"));
        }
        if structural_ranges
            .iter()
            .any(|(start, stop)| offset < *stop && *start < end)
        {
            return Err(ImmutableError::Invalid("object structural overlap"));
        }
        let record = checked_range(data, offset, length, "object")?;
        if length < OBJECT_HEADER_LEN
            || &record[..8] != OBJECT_MAGIC
            || usize::from(u16_at(record, 8, "object header")?) != OBJECT_HEADER_LEN
            || u32_at(record, 12, "object header")? != 0
            || record[40..OBJECT_HEADER_LEN].iter().any(|byte| *byte != 0)
        {
            return Err(ImmutableError::Invalid("object header"));
        }
        let kind = u16_at(record, 10, "object header")?;
        let object_id = u64_at(record, 16, "object header")?;
        let payload_len = usize_at(record, 24, "object length")?;
        let logical_len = u64_at(record, 32, "object length")?;
        if kind == 0
            || object_id == 0
            || OBJECT_HEADER_LEN
                .checked_add(payload_len)
                .ok_or(ImmutableError::Invalid("object length"))?
                != length
            || u64_from_usize(payload_len)? != logical_len
        {
            return Err(ImmutableError::Invalid("object length"));
        }
        if object_id != locator.object_id
            || kind != locator.kind
            || logical_len != locator.logical_len
        {
            return Err(ImmutableError::Invalid("object locator"));
        }
        if digest(&[OBJECT_DOMAIN, record]) != locator.digest {
            return Err(ImmutableError::Invalid("object digest"));
        }
        object_ranges.push((offset, end));
    }
    object_ranges.sort_unstable();
    if object_ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(ImmutableError::Invalid("object overlap"));
    }

    Ok(InternalReport {
        public: ImmutableReport {
            sequence: footer.sequence,
            object_count: locators.len(),
            page_count: seen.len(),
            root_level: root.level,
            snapshot_digest: footer.snapshot_digest,
            commit_digest: footer.commit_digest,
        },
        locators,
        footer_offset,
    })
}

pub fn validate(data: &[u8], limits: ImmutableLimits) -> Result<ImmutableReport, ImmutableError> {
    Ok(validate_internal(data, limits)?.public)
}
