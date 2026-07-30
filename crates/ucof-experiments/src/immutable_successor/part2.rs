fn parse_footer(data: &[u8], offset: usize) -> Result<Footer, ImmutableError> {
    let raw = checked_range(data, offset, FOOTER_LEN, "footer")?;
    if &raw[..8] != FOOTER_MAGIC || raw[112..].iter().any(|byte| *byte != 0) {
        return Err(ImmutableError::Invalid("footer"));
    }
    Ok(Footer {
        sequence: u64_at(raw, 8, "footer")?,
        snapshot_offset: u64_at(raw, 16, "footer")?,
        snapshot_len: u64_at(raw, 24, "footer")?,
        previous_footer_offset: u64_at(raw, 32, "footer")?,
        page_count_current: u64_at(raw, 40, "footer")?,
        snapshot_digest: array(raw, 48, "footer")?,
        commit_digest: array(raw, 80, "footer")?,
    })
}

fn footer_semantics(footer: &Footer) -> Vec<u8> {
    let mut result = vec![0_u8; 72];
    put_u64(&mut result, 0, footer.sequence);
    put_u64(&mut result, 8, footer.snapshot_offset);
    put_u64(&mut result, 16, footer.snapshot_len);
    put_u64(&mut result, 24, footer.previous_footer_offset);
    put_u64(&mut result, 32, footer.page_count_current);
    result[40..].copy_from_slice(&footer.snapshot_digest);
    result
}

fn root_reference(
    data: &[u8],
    snapshot: &[u8],
    limits: ImmutableLimits,
) -> Result<PageRef, ImmutableError> {
    let root_offset = usize_at(snapshot, 16, "snapshot root")?;
    let root_level_u64 = u64_at(snapshot, 24, "snapshot root")?;
    let root_level =
        u8::try_from(root_level_u64).map_err(|_| ImmutableError::Invalid("snapshot root level"))?;
    if root_level > limits.max_depth {
        return Err(ImmutableError::Limit("page depth"));
    }
    let page = checked_range(data, root_offset, PAGE_SIZE, "root page")?;
    Ok(PageRef {
        minimum: u64_at(page, 20, "root page")?,
        maximum: u64_at(page, 28, "root page")?,
        offset: u64_from_usize(root_offset)?,
        level: root_level,
        digest: array(snapshot, 32, "snapshot root")?,
    })
}

// The traversal state remains explicit so every bounded collection is visible.
#[allow(clippy::too_many_arguments)]
fn parse_page(
    data: &[u8],
    reference: &PageRef,
    snapshot_offset: usize,
    limits: ImmutableLimits,
    seen: &mut HashSet<usize>,
    stack: &mut Vec<PageRef>,
    locators: &mut Vec<Locator>,
    structural_ranges: &mut Vec<(usize, usize)>,
) -> Result<(), ImmutableError> {
    let offset = usize_from_u64(reference.offset, "page offset")?;
    if offset < FILE_HEADER_LEN {
        return Err(ImmutableError::Invalid("page range"));
    }
    let end = offset
        .checked_add(PAGE_SIZE)
        .ok_or(ImmutableError::Invalid("page range"))?;
    if end > snapshot_offset {
        return Err(ImmutableError::Invalid("page range"));
    }
    if structural_ranges
        .iter()
        .any(|(start, stop)| offset < *stop && *start < end)
    {
        return Err(ImmutableError::Invalid("page overlap"));
    }
    if seen.len() >= limits.max_pages {
        return Err(ImmutableError::Limit("page count"));
    }
    if !seen.insert(offset) {
        return Err(ImmutableError::Invalid("page cycle"));
    }

    let page = checked_range(data, offset, PAGE_SIZE, "page")?;
    if digest(&[PAGE_DOMAIN, page]) != reference.digest {
        return Err(ImmutableError::Invalid("page digest"));
    }
    if &page[..8] != PAGE_MAGIC {
        return Err(ImmutableError::Invalid("page header"));
    }
    let kind = page[8];
    let level = page[9];
    let reserved = u16_at(page, 10, "page header")?;
    let count = usize::try_from(u32_at(page, 12, "page header")?)
        .map_err(|_| ImmutableError::Invalid("page count"))?;
    let entry_size = usize::try_from(u32_at(page, 16, "page header")?)
        .map_err(|_| ImmutableError::Invalid("page entry size"))?;
    let minimum = u64_at(page, 20, "page header")?;
    let maximum = u64_at(page, 28, "page header")?;
    if reserved != 0 || page[36..64].iter().any(|byte| *byte != 0) || count == 0 {
        return Err(ImmutableError::Invalid("page header"));
    }
    if level != reference.level || minimum != reference.minimum || maximum != reference.maximum {
        return Err(ImmutableError::Invalid("page reference"));
    }
    structural_ranges.push((offset, end));

    match kind {
        1 => {
            if level != 0 || entry_size != LEAF_ENTRY_LEN || count > LEAF_CAPACITY {
                return Err(ImmutableError::Invalid("leaf shape"));
            }
            if locators
                .len()
                .checked_add(count)
                .ok_or(ImmutableError::Limit("object count"))?
                > limits.max_objects
            {
                return Err(ImmutableError::Limit("object count"));
            }
            allocation_check::<Locator>(locators.len() + count, limits)?;
            let before = locators.len();
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                let object_id = u64_at(page, entry, "leaf entry")?;
                let kind = u16_at(page, entry + 8, "leaf entry")?;
                if object_id == 0
                    || kind == 0
                    || page[entry + 10..entry + 16].iter().any(|byte| *byte != 0)
                    || page[entry + 72..entry + 88].iter().any(|byte| *byte != 0)
                {
                    return Err(ImmutableError::Invalid("leaf entry"));
                }
                locators.push(Locator {
                    object_id,
                    kind,
                    record_offset: u64_at(page, entry + 16, "leaf entry")?,
                    record_len: u64_at(page, entry + 24, "leaf entry")?,
                    logical_len: u64_at(page, entry + 32, "leaf entry")?,
                    digest: array(page, entry + 40, "leaf entry")?,
                });
            }
            let added = &locators[before..];
            if added
                .windows(2)
                .any(|pair| pair[0].object_id >= pair[1].object_id)
                || added.first().map(|entry| entry.object_id) != Some(minimum)
                || added.last().map(|entry| entry.object_id) != Some(maximum)
            {
                return Err(ImmutableError::Invalid("leaf order"));
            }
            let used = PAGE_HEADER_LEN + count * LEAF_ENTRY_LEN;
            if page[used..].iter().any(|byte| *byte != 0) {
                return Err(ImmutableError::Invalid("leaf padding"));
            }
        }
        2 => {
            if level == 0 || entry_size != INTERNAL_ENTRY_LEN || count > INTERNAL_FANOUT {
                return Err(ImmutableError::Invalid("internal shape"));
            }
            if level > limits.max_depth {
                return Err(ImmutableError::Limit("page depth"));
            }
            allocation_check::<PageRef>(stack.len() + count, limits)?;
            let mut children = Vec::with_capacity(count);
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                let child_minimum = u64_at(page, entry, "child entry")?;
                let child_maximum = u64_at(page, entry + 8, "child entry")?;
                let child_len = usize_at(page, entry + 24, "child entry")?;
                if child_minimum > child_maximum || child_len != PAGE_SIZE {
                    return Err(ImmutableError::Invalid("child entry"));
                }
                children.push(PageRef {
                    minimum: child_minimum,
                    maximum: child_maximum,
                    offset: u64_at(page, entry + 16, "child entry")?,
                    level: level - 1,
                    digest: array(page, entry + 32, "child entry")?,
                });
            }
            if children
                .windows(2)
                .any(|pair| pair[0].maximum >= pair[1].minimum)
                || children.first().map(|entry| entry.minimum) != Some(minimum)
                || children.last().map(|entry| entry.maximum) != Some(maximum)
            {
                return Err(ImmutableError::Invalid("child order"));
            }
            let used = PAGE_HEADER_LEN + count * INTERNAL_ENTRY_LEN;
            if page[used..].iter().any(|byte| *byte != 0) {
                return Err(ImmutableError::Invalid("internal padding"));
            }
            stack.extend(children.into_iter().rev());
        }
        _ => return Err(ImmutableError::Invalid("page kind")),
    }
    Ok(())
}
