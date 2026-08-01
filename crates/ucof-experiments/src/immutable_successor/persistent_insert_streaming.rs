fn insert_persistent_tail_path(
    data: &[u8],
    tail: &mut Vec<u8>,
    base_len: usize,
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
            return Ok(vec![append_persistent_tail_page(
                tail,
                base_len,
                &encode_leaf(&entries)?,
                limits,
                pages_written,
            )?]);
        }
        let split = entries.len().div_ceil(2);
        let left = append_persistent_tail_page(
            tail,
            base_len,
            &encode_leaf(&entries[..split])?,
            limits,
            pages_written,
        )?;
        let right = append_persistent_tail_page(
            tail,
            base_len,
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
    let replacements = insert_persistent_tail_path(
        data,
        tail,
        base_len,
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
        return Ok(vec![append_persistent_tail_page(
            tail,
            base_len,
            &encode_internal(&updated, reference.level)?,
            limits,
            pages_written,
        )?]);
    }
    let split = updated.len().div_ceil(2);
    let left = append_persistent_tail_page(
        tail,
        base_len,
        &encode_internal(&updated[..split], reference.level)?,
        limits,
        pages_written,
    )?;
    let right = append_persistent_tail_page(
        tail,
        base_len,
        &encode_internal(&updated[split..], reference.level)?,
        limits,
        pages_written,
    )?;
    Ok(vec![left, right])
}

/// Streams one persistent insertion as the verified base followed by one absolute-offset append tail.
///
/// Exact-end canonical validation, duplicate checks, object/page construction, split propagation,
/// root growth, commit hashing, and output limits all complete before the first sink write. Sink
/// failure after output begins is terminal and returns no success report.
pub fn append_persistent_insert_to<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
    input: &ImmutableObjectInput,
    limits: ImmutableLimits,
    options: PersistentMixedStreamingOptions,
) -> Result<PersistentMixedStreamingReport, PersistentMixedStreamingError> {
    if options.max_write_request_bytes == 0 {
        return Err(ImmutableError::Invalid("write request").into());
    }
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output").into());
    }
    if input.object_id == 0 || input.kind == 0 {
        return Err(ImmutableError::Invalid("object input").into());
    }

    let previous = validate_canonical_internal(data, limits)?;
    if previous.locators.len() >= limits.max_objects {
        return Err(ImmutableError::Limit("object count").into());
    }
    if previous
        .locators
        .binary_search_by_key(&input.object_id, |locator| locator.object_id)
        .is_ok()
    {
        return Err(ImmutableError::DuplicateObject(input.object_id).into());
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

    let base_len = data.len();
    let mut tail = Vec::new();
    let inserted = append_persistent_tail_object(&mut tail, base_len, input, limits)?;
    let mut pages_written = 0_usize;
    let mut roots = insert_persistent_tail_path(
        data,
        &mut tail,
        base_len,
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
                return Err(ImmutableError::Limit("page depth").into());
            }
            append_persistent_tail_page(
                &mut tail,
                base_len,
                &encode_internal(&roots, next_level)?,
                limits,
                &mut pages_written,
            )?
        }
        _ => return Err(ImmutableError::Invalid("persistent insertion root").into()),
    };

    let object_count = previous
        .public
        .object_count
        .checked_add(1)
        .ok_or(ImmutableError::Limit("object count"))?;
    let reachable_page_count = pages_reused
        .checked_add(pages_written)
        .ok_or(ImmutableError::Limit("page count"))?;
    let publication = PersistentTailPublication {
        base_len,
        sequence: previous
            .public
            .sequence
            .checked_add(1)
            .ok_or(ImmutableError::Limit("sequence"))?,
        root: next_root,
        parent_snapshot_digest: previous.public.snapshot_digest,
        previous_footer_offset: u64_from_usize(previous.footer_offset)?,
        page_count: pages_written,
        object_count,
    };
    let mut report = publish_persistent_tail(&mut tail, publication, limits)?;
    report.page_count = reachable_page_count;
    let output_bytes = persistent_tail_total_len(base_len, tail.len(), limits)?;
    if output_bytes > limits.max_file_bytes {
        return Err(ImmutableError::Limit("output").into());
    }

    let mut largest_write_request = 0_usize;
    write_persistent_mixed_chunked(
        writer,
        data,
        options.max_write_request_bytes,
        &mut largest_write_request,
    )?;
    write_persistent_mixed_chunked(
        writer,
        &tail,
        options.max_write_request_bytes,
        &mut largest_write_request,
    )?;

    Ok(PersistentMixedStreamingReport {
        report,
        mode: PersistentBatchMode::CopyOnWriteInsertion,
        pages_written,
        pages_reused,
        base_bytes_written: u64_from_usize(base_len)?,
        tail_bytes_written: u64_from_usize(tail.len())?,
        largest_write_request,
        tail_allocation_bytes: tail.capacity(),
    })
}

#[cfg(test)]
mod persistent_insert_streaming_tests {
    use super::*;

    fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::from(1 + seed % 31),
            vec![seed; payload_len],
        )
    }

    fn even_objects(count: usize) -> Vec<ImmutableObjectInput> {
        (1..=count)
            .map(|index| {
                let object_id = u64::try_from(index * 2).expect("object id");
                object(
                    object_id,
                    u8::try_from(index % 251).expect("seed"),
                    1 + index % 23,
                )
            })
            .collect()
    }

    fn assert_streamed_matches_owned(count: usize, inserted_id: u64, chunk: usize) {
        let limits = ImmutableLimits {
            max_file_bytes: 32 * 1024 * 1024,
            max_output_bytes: 32 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = build_genesis(&even_objects(count), limits).expect("base");
        let input = object(inserted_id, 211, 29);
        let owned = append_persistent_insert(&base, &input, limits).expect("owned insertion");
        let mut streamed = Vec::new();
        let report = append_persistent_insert_to(
            &mut streamed,
            &base,
            &input,
            limits,
            PersistentMixedStreamingOptions {
                max_write_request_bytes: chunk,
            },
        )
        .expect("streamed insertion");

        assert_eq!(streamed, owned.bytes);
        assert_eq!(report.report, owned.report);
        assert_eq!(report.mode, PersistentBatchMode::CopyOnWriteInsertion);
        assert_eq!(report.pages_written, owned.pages_written);
        assert_eq!(report.pages_reused, owned.pages_reused);
        assert_eq!(report.base_bytes_written, u64_from_usize(base.len()).expect("base"));
        assert_eq!(
            report.tail_bytes_written,
            u64_from_usize(streamed.len() - base.len()).expect("tail")
        );
        assert!(report.largest_write_request <= chunk);
        assert!(report.tail_allocation_bytes < streamed.len());
    }

    #[test]
    fn streamed_insertion_matches_owned_without_split() {
        assert_streamed_matches_owned(8, 9, 17);
    }

    #[test]
    fn streamed_insertion_matches_owned_leaf_split_and_root_growth() {
        let inserted_id = u64::try_from(LEAF_CAPACITY)
            .expect("capacity")
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .expect("identifier");
        assert_streamed_matches_owned(LEAF_CAPACITY, inserted_id, 31);
    }

    #[test]
    fn streamed_insertion_matches_owned_inside_internal_tree() {
        assert_streamed_matches_owned(LEAF_CAPACITY + 37, 3, 43);
    }

    #[test]
    fn duplicate_or_invalid_insertion_fails_before_output() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(8), limits).expect("base");
        for input in [object(2, 9, 4), object(0, 9, 4)] {
            let mut sink = Vec::new();
            assert!(append_persistent_insert_to(
                &mut sink,
                &base,
                &input,
                limits,
                PersistentMixedStreamingOptions::default(),
            )
            .is_err());
            assert!(sink.is_empty());
        }
    }
}
