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

fn rewrite_from_ids(
    data: &[u8],
    requested_ids: &[u64],
    limits: ImmutableLimits,
) -> Result<ImmutableRewriteResult, ImmutableError> {
    if requested_ids.is_empty() {
        return Err(ImmutableError::Invalid("rewrite selection"));
    }
    let source_internal = validate_internal(data, limits)?;
    let source = source_internal.public.clone();

    allocation_check::<u64>(requested_ids.len(), limits)?;
    allocation_check::<ImmutableObjectInput>(requested_ids.len(), limits)?;
    let mut retained_object_ids = requested_ids.to_vec();
    retained_object_ids.sort_unstable();
    if retained_object_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ImmutableError::Invalid("rewrite selection"));
    }
    if retained_object_ids.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count"));
    }

    let mut inputs = Vec::with_capacity(retained_object_ids.len());
    for object_id in &retained_object_ids {
        let index = source_internal
            .locators
            .binary_search_by_key(object_id, |locator| locator.object_id)
            .map_err(|_| ImmutableError::MissingObject(*object_id))?;
        inputs.push(input_from_locator(data, &source_internal.locators[index])?);
    }

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
    let source = validate_internal(data, limits)?;
    let ids: Vec<u64> = source
        .locators
        .iter()
        .map(|locator| locator.object_id)
        .collect();
    rewrite_from_ids(data, &ids, limits)
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
    rewrite_from_ids(data, object_ids, limits)
}
