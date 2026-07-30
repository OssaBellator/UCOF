#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableRewriteResult {
    pub bytes: Vec<u8>,
    pub source: ImmutableReport,
    pub output: ImmutableReport,
    pub retained_object_ids: Vec<u64>,
    /// Rewriting changes byte-scoped commit identity and therefore does not preserve signatures.
    pub byte_scoped_signatures_preserved: bool,
}

fn input_from_locator(
    data: &[u8],
    locator: &Locator,
) -> Result<ImmutableObjectInput, ImmutableError> {
    let offset = usize_from_u64(locator.record_offset, "rewrite object")?;
    let length = usize_from_u64(locator.record_len, "rewrite object")?;
    let record = checked_range(data, offset, length, "rewrite object")?;
    let payload = checked_range(
        record,
        OBJECT_HEADER_LEN,
        length
            .checked_sub(OBJECT_HEADER_LEN)
            .ok_or(ImmutableError::Invalid("rewrite object"))?,
        "rewrite object",
    )?;
    Ok(ImmutableObjectInput::new(
        locator.object_id,
        locator.kind,
        payload.to_vec(),
    ))
}

fn check_rewrite_allocation(
    locators: &[&Locator],
    limits: ImmutableLimits,
) -> Result<(), ImmutableError> {
    let payload_bytes = locators.iter().try_fold(0_usize, |total, locator| {
        let record_len = usize_from_u64(locator.record_len, "rewrite object")?;
        let payload_len = record_len
            .checked_sub(OBJECT_HEADER_LEN)
            .ok_or(ImmutableError::Invalid("rewrite object"))?;
        total
            .checked_add(payload_len)
            .ok_or(ImmutableError::Limit("allocation"))
    })?;

    // The retained identifier vector and input vector are owned here. build_genesis
    // canonicalizes through one cloned input vector, so account for both sets of
    // input structs and payload buffers before cloning any payload bytes.
    let identifier_bytes = locators
        .len()
        .checked_mul(std::mem::size_of::<u64>())
        .ok_or(ImmutableError::Limit("allocation"))?;
    let input_bytes = locators
        .len()
        .checked_mul(std::mem::size_of::<ImmutableObjectInput>())
        .and_then(|bytes| bytes.checked_mul(2))
        .ok_or(ImmutableError::Limit("allocation"))?;
    let cloned_payload_bytes = payload_bytes
        .checked_mul(2)
        .ok_or(ImmutableError::Limit("allocation"))?;
    let required = identifier_bytes
        .checked_add(input_bytes)
        .and_then(|bytes| bytes.checked_add(cloned_payload_bytes))
        .ok_or(ImmutableError::Limit("allocation"))?;
    if required > limits.max_allocation_bytes {
        return Err(ImmutableError::Limit("allocation"));
    }
    Ok(())
}

fn rewrite_from_internal(
    data: &[u8],
    requested_ids: &[u64],
    source_internal: InternalReport,
    limits: ImmutableLimits,
) -> Result<ImmutableRewriteResult, ImmutableError> {
    if requested_ids.is_empty() {
        return Err(ImmutableError::Invalid("rewrite selection"));
    }

    allocation_check::<u64>(requested_ids.len(), limits)?;
    let mut retained_object_ids = requested_ids.to_vec();
    retained_object_ids.sort_unstable();
    if retained_object_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ImmutableError::Invalid("rewrite selection"));
    }
    if retained_object_ids.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count"));
    }

    let mut selected_locators = Vec::with_capacity(retained_object_ids.len());
    allocation_check::<&Locator>(retained_object_ids.len(), limits)?;
    for object_id in &retained_object_ids {
        let index = source_internal
            .locators
            .binary_search_by_key(object_id, |locator| locator.object_id)
            .map_err(|_| ImmutableError::MissingObject(*object_id))?;
        selected_locators.push(&source_internal.locators[index]);
    }
    check_rewrite_allocation(&selected_locators, limits)?;

    let mut inputs = Vec::with_capacity(retained_object_ids.len());
    for locator in selected_locators {
        inputs.push(input_from_locator(data, locator)?);
    }

    let source = source_internal.public;
    let bytes = build_genesis(&inputs, limits)?;
    let output = validate(&bytes, limits)?;
    Ok(ImmutableRewriteResult {
        bytes,
        source,
        output,
        retained_object_ids,
        byte_scoped_signatures_preserved: false,
    })
}

/// Strictly validates the active state and rewrites every active object into a new genesis file.
pub fn rewrite_all(
    data: &[u8],
    limits: ImmutableLimits,
) -> Result<ImmutableRewriteResult, ImmutableError> {
    let source_internal = validate_internal(data, limits)?;
    allocation_check::<u64>(source_internal.locators.len(), limits)?;
    let ids: Vec<u64> = source_internal
        .locators
        .iter()
        .map(|locator| locator.object_id)
        .collect();
    rewrite_from_internal(data, &ids, source_internal, limits)
}

/// Strictly validates the active state and rewrites only caller-selected object identifiers.
///
/// This function performs no semantic dependency discovery. Profiles and applications are
/// responsible for supplying a complete retained set.
pub fn rewrite_selected(
    data: &[u8],
    object_ids: &[u64],
    limits: ImmutableLimits,
) -> Result<ImmutableRewriteResult, ImmutableError> {
    let source_internal = validate_internal(data, limits)?;
    rewrite_from_internal(data, object_ids, source_internal, limits)
}
