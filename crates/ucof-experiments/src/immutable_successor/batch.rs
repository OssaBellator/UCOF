/// One deterministic active-state change for [`append_batch`].
///
/// `Put` inserts a new object or replaces the active object with the same identifier.
/// `Delete` removes an active object. A batch may mention each object identifier at most once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableBatchOperation {
    Put(ImmutableObjectInput),
    Delete(u64),
}

impl ImmutableBatchOperation {
    fn object_id(&self) -> u64 {
        match self {
            Self::Put(input) => input.object_id,
            Self::Delete(object_id) => *object_id,
        }
    }
}

/// Appends one complete snapshot containing a deterministic mixed batch.
///
/// Operations are canonicalized by object identifier, so caller order does not affect the
/// resulting bytes. `Put` supports both insertion and replacement, while `Delete` requires an
/// active object. Duplicate operation identifiers are rejected rather than assigned order-based
/// semantics.
///
/// This is a reusable byte-writer baseline, not the final copy-on-write planner. It rebuilds every
/// active directory page in the new commit and therefore does not yet preserve unchanged page
/// identities across a mixed batch.
pub fn append_batch(
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
) -> Result<Vec<u8>, ImmutableError> {
    if operations.is_empty() {
        return Err(ImmutableError::Invalid("batch operations"));
    }
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }

    let previous = validate_internal(data, limits)?;
    let next_sequence = previous
        .public
        .sequence
        .checked_add(1)
        .ok_or(ImmutableError::Limit("sequence"))?;
    let parent_snapshot_digest = previous.public.snapshot_digest;
    let previous_footer_offset = u64_from_usize(previous.footer_offset)?;

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

    let projected_capacity = previous
        .locators
        .len()
        .checked_add(operations.len())
        .ok_or(ImmutableError::Limit("object count"))?;
    allocation_check::<u64>(projected_capacity, limits)?;
    let mut active_ids: Vec<u64> = previous
        .locators
        .iter()
        .map(|locator| locator.object_id)
        .collect();

    for index in &order {
        match &operations[*index] {
            ImmutableBatchOperation::Put(input) => {
                if input.object_id == 0 || input.kind == 0 {
                    return Err(ImmutableError::Invalid("object input"));
                }
                if let Err(position) = active_ids.binary_search(&input.object_id) {
                    active_ids.insert(position, input.object_id);
                }
            }
            ImmutableBatchOperation::Delete(object_id) => {
                if *object_id == 0 {
                    return Err(ImmutableError::Invalid("batch object id"));
                }
                let position = active_ids
                    .binary_search(object_id)
                    .map_err(|_| ImmutableError::MissingObject(*object_id))?;
                active_ids.remove(position);
            }
        }
    }

    if active_ids.is_empty() {
        return Err(ImmutableError::Invalid("batch result"));
    }
    if active_ids.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count"));
    }
    allocation_check::<Locator>(active_ids.len(), limits)?;

    let mut output = data.to_vec();
    let mut locators = previous.locators;
    for index in order {
        match &operations[index] {
            ImmutableBatchOperation::Put(input) => {
                let locator = append_object(&mut output, input, limits)?;
                match locators.binary_search_by_key(&input.object_id, |entry| entry.object_id) {
                    Ok(position) => locators[position] = locator,
                    Err(position) => locators.insert(position, locator),
                }
            }
            ImmutableBatchOperation::Delete(object_id) => {
                let position = locators
                    .binary_search_by_key(object_id, |entry| entry.object_id)
                    .map_err(|_| ImmutableError::Invalid("batch state"))?;
                locators.remove(position);
            }
        }
    }

    let (root, pages) = build_tree(&mut output, &mut locators, limits)?;
    publish(
        &mut output,
        next_sequence,
        &root,
        parent_snapshot_digest,
        previous_footer_offset,
        pages,
        limits,
    )?;
    validate(&output, limits)?;
    Ok(output)
}
