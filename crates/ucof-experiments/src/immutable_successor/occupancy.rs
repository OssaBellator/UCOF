/// Minimum entries in a non-root leaf page under the canonical half-full policy.
pub const LEAF_MIN_OCCUPANCY: usize = LEAF_CAPACITY.div_ceil(2);
/// Minimum children in a non-root internal page under the canonical half-full policy.
pub const INTERNAL_MIN_OCCUPANCY: usize = INTERNAL_FANOUT.div_ceil(2);

fn canonical_group_sizes(
    total: usize,
    capacity: usize,
    minimum: usize,
    limits: ImmutableLimits,
) -> Result<Vec<usize>, ImmutableError> {
    if total == 0 || capacity == 0 || minimum == 0 || minimum > capacity {
        return Err(ImmutableError::Invalid("canonical occupancy"));
    }
    let groups = total
        .checked_add(capacity - 1)
        .ok_or(ImmutableError::Limit("page count"))?
        / capacity;
    allocation_check::<usize>(groups, limits)?;
    if groups == 1 {
        return Ok(vec![total]);
    }

    let full_groups = total / capacity;
    let remainder = total % capacity;
    let mut sizes = Vec::with_capacity(groups);
    if remainder == 0 {
        sizes.resize(full_groups, capacity);
    } else if remainder >= minimum {
        sizes.resize(full_groups, capacity);
        sizes.push(remainder);
    } else {
        let prefix_groups = full_groups
            .checked_sub(1)
            .ok_or(ImmutableError::Invalid("canonical occupancy"))?;
        sizes.resize(prefix_groups, capacity);
        let transfer = minimum - remainder;
        let penultimate = capacity
            .checked_sub(transfer)
            .ok_or(ImmutableError::Invalid("canonical occupancy"))?;
        sizes.push(penultimate);
        sizes.push(minimum);
    }

    if sizes.len() != groups
        || sizes.iter().any(|size| *size < minimum || *size > capacity)
        || sizes.iter().try_fold(0_usize, |sum, size| sum.checked_add(*size)) != Some(total)
    {
        return Err(ImmutableError::Invalid("canonical occupancy"));
    }
    Ok(sizes)
}

fn validate_canonical_internal(
    data: &[u8],
    limits: ImmutableLimits,
) -> Result<InternalReport, ImmutableError> {
    let report = validate_internal(data, limits)?;
    let footer = parse_footer(data, report.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut stack = vec![root.clone()];
    let mut seen = HashSet::new();

    while let Some(reference) = stack.pop() {
        if seen.len() >= limits.max_pages {
            return Err(ImmutableError::Limit("page count"));
        }
        let offset = usize_from_u64(reference.offset, "page offset")?;
        if !seen.insert(offset) {
            return Err(ImmutableError::Invalid("page cycle"));
        }
        let page = checked_range(data, offset, PAGE_SIZE, "page")?;
        let kind = page[8];
        let count = usize::try_from(u32_at(page, 12, "page count")?)
            .map_err(|_| ImmutableError::Invalid("page count"))?;
        let is_root = reference.offset == root.offset;

        match kind {
            1 => {
                if (!is_root && count < LEAF_MIN_OCCUPANCY) || count > LEAF_CAPACITY {
                    return Err(ImmutableError::Invalid("leaf occupancy"));
                }
            }
            2 => {
                let below_minimum = if is_root {
                    count < 2
                } else {
                    count < INTERNAL_MIN_OCCUPANCY
                };
                if below_minimum || count > INTERNAL_FANOUT {
                    return Err(ImmutableError::Invalid("internal occupancy"));
                }
                let required = stack
                    .len()
                    .checked_add(count)
                    .ok_or(ImmutableError::Limit("page count"))?;
                allocation_check::<PageRef>(required, limits)?;
                for index in (0..count).rev() {
                    let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                    stack.push(PageRef {
                        minimum: u64_at(page, entry, "child entry")?,
                        maximum: u64_at(page, entry + 8, "child entry")?,
                        offset: u64_at(page, entry + 16, "child entry")?,
                        level: reference
                            .level
                            .checked_sub(1)
                            .ok_or(ImmutableError::Invalid("child level"))?,
                        digest: array(page, entry + 32, "child entry")?,
                    });
                }
            }
            _ => return Err(ImmutableError::Invalid("page kind")),
        }
    }
    Ok(report)
}

/// Strictly validates the file and additionally enforces canonical half-full non-root occupancy.
pub fn validate_canonical_occupancy(
    data: &[u8],
    limits: ImmutableLimits,
) -> Result<ImmutableReport, ImmutableError> {
    Ok(validate_canonical_internal(data, limits)?.public)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_two_groups_are_redistributed_only_when_required() {
        let limits = ImmutableLimits::default();
        assert_eq!(
            canonical_group_sizes(LEAF_CAPACITY, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY, limits)
                .expect("single root leaf"),
            vec![LEAF_CAPACITY]
        );
        assert_eq!(
            canonical_group_sizes(
                LEAF_CAPACITY + 1,
                LEAF_CAPACITY,
                LEAF_MIN_OCCUPANCY,
                limits,
            )
            .expect("minimum split"),
            vec![LEAF_CAPACITY + 1 - LEAF_MIN_OCCUPANCY, LEAF_MIN_OCCUPANCY]
        );
        assert_eq!(
            canonical_group_sizes(400, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY, limits)
                .expect("400-object partition"),
            vec![LEAF_CAPACITY, 122, LEAF_MIN_OCCUPANCY]
        );
        assert_eq!(
            canonical_group_sizes(
                2 * LEAF_CAPACITY,
                LEAF_CAPACITY,
                LEAF_MIN_OCCUPANCY,
                limits,
            )
            .expect("two full leaves"),
            vec![LEAF_CAPACITY, LEAF_CAPACITY]
        );
    }
}
