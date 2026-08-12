struct ImmutableSourceInventory {
    report: ImmutableReport,
    locators: Vec<Locator>,
    stats: ImmutableSourceStats,
}

fn validated_source_inventory<S: ImmutableReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceInventory, ImmutableSourceError> {
    let strict = validate_source_at(source, limits)?;
    let remaining = remaining_source_limits(limits, strict.stats)?;
    let mut reader = SourceReader::new(source, remaining)?;
    let envelope = read_lookup_envelope(&mut reader)?;
    if envelope.sequence != strict.report.sequence
        || envelope.snapshot_digest != strict.report.snapshot_digest
        || envelope.commit_digest != strict.report.commit_digest
        || envelope.root.level != strict.report.root_level
    {
        return Err(ImmutableSourceError::Io("source changed"));
    }

    let mut visited = HashSet::new();
    let mut stack = vec![envelope.root.clone()];
    let mut locators = Vec::new();
    let mut known_ranges = vec![
        (envelope.snapshot_offset, envelope.footer_offset),
        (envelope.footer_offset, reader.length),
    ];
    while let Some(reference) = stack.pop() {
        read_full_page(
            &mut reader,
            &reference,
            &envelope,
            &mut visited,
            &mut stack,
            &mut locators,
            &mut known_ranges,
        )?;
    }
    locators.sort_by_key(|locator| locator.object_id);
    if locators.len() != strict.report.object_count
        || visited.len() != strict.report.page_count
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableSourceError::Io("source changed"));
    }

    let mut stats = strict.stats;
    add_source_stats(&mut stats, reader.stats)?;
    Ok(ImmutableSourceInventory {
        report: strict.report,
        locators,
        stats,
    })
}

fn source_input_from_locator<S: ImmutableReadAt>(
    source: &mut S,
    locator: &Locator,
    limits: ImmutableSourceLimits,
    stats: &mut ImmutableSourceStats,
) -> Result<ImmutableObjectInput, ImmutableSourceError> {
    let length = usize_from_u64(locator.record_len, "rewrite object")
        .map_err(ImmutableSourceError::Format)?;
    if length < OBJECT_HEADER_LEN || length > limits.format.max_allocation_bytes {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit(
            "allocation",
        )));
    }
    stats.largest_allocation = stats.largest_allocation.max(length);
    let mut record = vec![0_u8; length];
    read_direct(
        source,
        limits,
        stats,
        locator.record_offset,
        &mut record,
    )?;
    if &record[..8] != OBJECT_MAGIC
        || usize::from(
            u16_at(&record, 8, "rewrite object").map_err(ImmutableSourceError::Format)?,
        ) != OBJECT_HEADER_LEN
        || u32_at(&record, 12, "rewrite object").map_err(ImmutableSourceError::Format)? != 0
        || record[40..OBJECT_HEADER_LEN]
            .iter()
            .any(|byte| *byte != 0)
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "rewrite object",
        )));
    }
    let object_id =
        u64_at(&record, 16, "rewrite object").map_err(ImmutableSourceError::Format)?;
    let kind = u16_at(&record, 10, "rewrite object").map_err(ImmutableSourceError::Format)?;
    let payload_len =
        usize_at(&record, 24, "rewrite object").map_err(ImmutableSourceError::Format)?;
    let logical_len =
        u64_at(&record, 32, "rewrite object").map_err(ImmutableSourceError::Format)?;
    if object_id != locator.object_id
        || kind != locator.kind
        || kind == 0
        || OBJECT_HEADER_LEN
            .checked_add(payload_len)
            .is_none_or(|value| value != length)
        || u64_from_usize(payload_len).map_err(ImmutableSourceError::Format)? != logical_len
        || logical_len != locator.logical_len
        || digest(&[OBJECT_DOMAIN, &record]) != locator.digest
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "rewrite object",
        )));
    }
    stats.bytes_hashed = stats
        .bytes_hashed
        .checked_add(
            u64::try_from(record.len())
                .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
        )
        .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
    Ok(ImmutableObjectInput::new(
        object_id,
        kind,
        record[OBJECT_HEADER_LEN..].to_vec(),
    ))
}
