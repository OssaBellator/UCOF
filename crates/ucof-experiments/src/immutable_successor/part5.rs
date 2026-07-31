fn publish(
    output: &mut Vec<u8>,
    sequence: u64,
    root: &PageRef,
    parent_snapshot_digest: [u8; 32],
    previous_footer_offset: u64,
    page_count: usize,
    limits: ImmutableLimits,
) -> Result<(), ImmutableError> {
    let required = SNAPSHOT_LEN
        .checked_add(FOOTER_LEN)
        .and_then(|value| output.len().checked_add(value))
        .ok_or(ImmutableError::Limit("output"))?;
    if required > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
    let snapshot_offset = u64_from_usize(output.len())?;
    let mut snapshot = vec![0_u8; SNAPSHOT_LEN];
    snapshot[..8].copy_from_slice(SNAPSHOT_MAGIC);
    put_u64(&mut snapshot, 8, sequence);
    put_u64(&mut snapshot, 16, root.offset);
    put_u64(&mut snapshot, 24, u64::from(root.level));
    snapshot[32..64].copy_from_slice(&root.digest);
    snapshot[64..].copy_from_slice(&parent_snapshot_digest);
    let snapshot_digest = digest(&[SNAPSHOT_DOMAIN, &snapshot]);
    output.extend_from_slice(&snapshot);

    let footer = Footer {
        sequence,
        snapshot_offset,
        snapshot_len: u64_from_usize(SNAPSHOT_LEN)?,
        previous_footer_offset,
        page_count_current: u64_from_usize(page_count)?,
        snapshot_digest,
        commit_digest: [0_u8; 32],
    };
    let semantics = footer_semantics(&footer);
    let commit_start = if previous_footer_offset == ABSENT_OFFSET {
        0
    } else {
        usize_from_u64(previous_footer_offset, "previous footer")?
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableError::Invalid("previous footer"))?
    };
    let commit_digest = digest(&[COMMIT_DOMAIN, &output[commit_start..], &semantics]);

    let mut raw = vec![0_u8; FOOTER_LEN];
    raw[..8].copy_from_slice(FOOTER_MAGIC);
    put_u64(&mut raw, 8, sequence);
    put_u64(&mut raw, 16, snapshot_offset);
    put_u64(&mut raw, 24, u64_from_usize(SNAPSHOT_LEN)?);
    put_u64(&mut raw, 32, previous_footer_offset);
    put_u64(&mut raw, 40, u64_from_usize(page_count)?);
    raw[48..80].copy_from_slice(&snapshot_digest);
    raw[80..112].copy_from_slice(&commit_digest);
    output.extend_from_slice(&raw);
    Ok(())
}

pub fn build_genesis(
    inputs: &[ImmutableObjectInput],
    limits: ImmutableLimits,
) -> Result<Vec<u8>, ImmutableError> {
    if inputs.is_empty() || inputs.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count"));
    }
    allocation_check::<Locator>(inputs.len(), limits)?;
    let mut ordered = inputs.to_vec();
    ordered.sort_by_key(|input| input.object_id);
    if let Some(pair) = ordered
        .windows(2)
        .find(|pair| pair[0].object_id == pair[1].object_id)
    {
        return Err(ImmutableError::DuplicateObject(pair[0].object_id));
    }

    let mut output = vec![0_u8; FILE_HEADER_LEN];
    output[..8].copy_from_slice(FILE_MAGIC);
    let mut locators = Vec::with_capacity(ordered.len());
    for input in &ordered {
        locators.push(append_object(&mut output, input, limits)?);
    }
    let (root, pages) = build_tree(&mut output, &mut locators, limits)?;
    publish(
        &mut output,
        0,
        &root,
        [0_u8; 32],
        ABSENT_OFFSET,
        pages,
        limits,
    )?;
    validate_canonical_occupancy(&output, limits)?;
    Ok(output)
}

pub fn append_replacement(
    data: &[u8],
    replacement: &ImmutableObjectInput,
    limits: ImmutableLimits,
) -> Result<Vec<u8>, ImmutableError> {
    let previous = validate_canonical_internal(data, limits)?;
    let index = previous
        .locators
        .iter()
        .position(|locator| locator.object_id == replacement.object_id)
        .ok_or(ImmutableError::MissingObject(replacement.object_id))?;
    let mut output = data.to_vec();
    let locator = append_object(&mut output, replacement, limits)?;
    let mut locators = previous.locators;
    locators[index] = locator;
    let (root, pages) = build_tree(&mut output, &mut locators, limits)?;
    publish(
        &mut output,
        previous.public.sequence + 1,
        &root,
        previous.public.snapshot_digest,
        u64_from_usize(previous.footer_offset)?,
        pages,
        limits,
    )?;
    validate_canonical_occupancy(&output, limits)?;
    Ok(output)
}
