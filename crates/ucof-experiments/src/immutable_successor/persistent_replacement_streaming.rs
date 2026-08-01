fn rewrite_replacement_tail_path(
    data: &[u8],
    tail: &mut Vec<u8>,
    base_len: u64,
    reference: &PageRef,
    replacements: &BTreeMap<u64, Locator>,
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<(PageRef, bool), ImmutableError> {
    if !page_has_replacement(replacements, reference.minimum, reference.maximum) {
        return Ok((reference.clone(), false));
    }

    let offset = usize_from_u64(reference.offset, "persistent page")?;
    let page = checked_range(data, offset, PAGE_SIZE, "persistent page")?;
    if digest(&[PAGE_DOMAIN, page]) != reference.digest
        || &page[..8] != PAGE_MAGIC
        || page[9] != reference.level
        || u64_at(page, 20, "persistent page")? != reference.minimum
        || u64_at(page, 28, "persistent page")? != reference.maximum
    {
        return Err(ImmutableError::Invalid("persistent page reference"));
    }

    let count = usize::try_from(u32_at(page, 12, "persistent page")?)
        .map_err(|_| ImmutableError::Invalid("persistent page count"))?;
    match page[8] {
        1 => {
            if reference.level != 0
                || count == 0
                || count > LEAF_CAPACITY
                || usize::try_from(u32_at(page, 16, "persistent leaf")?)
                    .map_err(|_| ImmutableError::Invalid("persistent leaf"))?
                    != LEAF_ENTRY_LEN
            {
                return Err(ImmutableError::Invalid("persistent leaf"));
            }
            allocation_check::<Locator>(count, limits)?;
            let mut entries = Vec::with_capacity(count);
            let mut changed = false;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                let locator = Locator {
                    object_id: u64_at(page, entry, "persistent leaf entry")?,
                    kind: u16_at(page, entry + 8, "persistent leaf entry")?,
                    record_offset: u64_at(page, entry + 16, "persistent leaf entry")?,
                    record_len: u64_at(page, entry + 24, "persistent leaf entry")?,
                    logical_len: u64_at(page, entry + 32, "persistent leaf entry")?,
                    digest: array(page, entry + 40, "persistent leaf entry")?,
                };
                if let Some(replacement) = replacements.get(&locator.object_id) {
                    entries.push(replacement.clone());
                    changed = true;
                } else {
                    entries.push(locator);
                }
            }
            if !changed {
                return Err(ImmutableError::Invalid("persistent replacement routing"));
            }
            let rewritten = encode_leaf(&entries)?;
            Ok((
                append_persistent_tail_page(tail, base_len, &rewritten, limits, pages_written)?,
                true,
            ))
        }
        2 => {
            if reference.level == 0
                || count == 0
                || count > INTERNAL_FANOUT
                || usize::try_from(u32_at(page, 16, "persistent internal")?)
                    .map_err(|_| ImmutableError::Invalid("persistent internal"))?
                    != INTERNAL_ENTRY_LEN
            {
                return Err(ImmutableError::Invalid("persistent internal"));
            }
            allocation_check::<PageRef>(count, limits)?;
            let mut children = Vec::with_capacity(count);
            let mut changed = false;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                let child = PageRef {
                    minimum: u64_at(page, entry, "persistent child")?,
                    maximum: u64_at(page, entry + 8, "persistent child")?,
                    offset: u64_at(page, entry + 16, "persistent child")?,
                    level: reference.level - 1,
                    digest: array(page, entry + 32, "persistent child")?,
                };
                let (next, child_changed) = rewrite_replacement_tail_path(
                    data,
                    tail,
                    base_len,
                    &child,
                    replacements,
                    limits,
                    pages_written,
                )?;
                children.push(next);
                changed |= child_changed;
            }
            if !changed {
                return Err(ImmutableError::Invalid("persistent replacement routing"));
            }
            let rewritten = encode_internal(&children, reference.level)?;
            Ok((
                append_persistent_tail_page(tail, base_len, &rewritten, limits, pages_written)?,
                true,
            ))
        }
        _ => Err(ImmutableError::Invalid("persistent page kind")),
    }
}

/// Streams a replacement-only persistent batch as the verified base followed by one append tail.
///
/// All source validation, operation validation, object/page construction, commit hashing, and limit
/// checks complete before the first sink write. The function owns only the append tail rather than a
/// second complete successor file. Sink failure after output begins is terminal and returns no success
/// report.
pub fn append_persistent_replacement_batch_to<W: Write>(
    writer: &mut W,
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
    options: PersistentMixedStreamingOptions,
) -> Result<PersistentMixedStreamingReport, PersistentMixedStreamingError> {
    validate_persistent_mixed_streaming_options(data, limits, options)?;
    if operations.is_empty() {
        return Err(ImmutableError::Invalid("batch operations").into());
    }

    let previous = validate_canonical_internal(data, limits)?;
    allocation_check::<usize>(operations.len(), limits)?;
    let mut order: Vec<usize> = (0..operations.len()).collect();
    order.sort_unstable_by_key(|index| operations[*index].object_id());
    if let Some(pair) = order.windows(2).find(|pair| {
        operations[pair[0]].object_id() == operations[pair[1]].object_id()
    }) {
        return Err(ImmutableError::DuplicateObject(operations[pair[0]].object_id()).into());
    }

    let base_len = u64_from_usize(data.len())?;
    let mut tail = Vec::new();
    let mut replacements = BTreeMap::new();
    for index in order {
        let ImmutableBatchOperation::Put(input) = &operations[index] else {
            return Err(ImmutableError::Invalid("persistent replacement operations").into());
        };
        if input.object_id == 0 || input.kind == 0 {
            return Err(ImmutableError::Invalid("object identity").into());
        }
        if previous
            .locators
            .binary_search_by_key(&input.object_id, |locator| locator.object_id)
            .is_err()
        {
            return Err(ImmutableError::MissingObject(input.object_id).into());
        }
        let locator = append_persistent_tail_object(&mut tail, base_len, input, limits)?;
        replacements.insert(input.object_id, locator);
    }

    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut pages_written = 0_usize;
    let (next_root, changed) = rewrite_replacement_tail_path(
        data,
        &mut tail,
        base_len,
        &root,
        &replacements,
        limits,
        &mut pages_written,
    )?;
    if !changed || pages_written == 0 {
        return Err(ImmutableError::Invalid("persistent replacement state").into());
    }
    let pages_reused = previous
        .public
        .page_count
        .checked_sub(pages_written)
        .ok_or(ImmutableError::Invalid("persistent page accounting"))?;
    let sequence = previous
        .public
        .sequence
        .checked_add(1)
        .ok_or(ImmutableError::Limit("sequence"))?;
    let report = publish_persistent_tail(
        &mut tail,
        base_len,
        sequence,
        &next_root,
        previous.public.snapshot_digest,
        u64_from_usize(previous.footer_offset)?,
        previous.public.page_count,
        previous.public.object_count,
        limits,
    )?;
    let tail_bytes = tail.len();
    let output_bytes = data
        .len()
        .checked_add(tail_bytes)
        .ok_or(ImmutableError::Limit("output"))?;
    if output_bytes > limits.max_output_bytes || output_bytes > limits.max_file_bytes {
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
        mode: PersistentBatchMode::CopyOnWriteReplacements,
        pages_written,
        pages_reused,
        base_bytes: data.len(),
        tail_bytes,
        bytes_written: output_bytes,
        largest_write_request,
        tail_allocation_bytes: tail.capacity(),
    })
}

#[cfg(test)]
mod persistent_replacement_streaming_tests {
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

    #[test]
    fn streamed_replacements_match_owned_writer_at_multiple_leaves() {
        let limits = ImmutableLimits {
            max_file_bytes: 32 * 1024 * 1024,
            max_output_bytes: 32 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = build_genesis(&even_objects(400), limits).expect("base");
        let operations = vec![
            ImmutableBatchOperation::Put(object(2, 201, 17)),
            ImmutableBatchOperation::Put(object(400, 202, 19)),
            ImmutableBatchOperation::Put(object(800, 203, 23)),
        ];
        let owned = append_persistent_batch(&base, &operations, limits).expect("owned writer");
        let mut streamed = Vec::new();
        let report = append_persistent_replacement_batch_to(
            &mut streamed,
            &base,
            &operations,
            limits,
            PersistentMixedStreamingOptions {
                max_write_request_bytes: 31,
            },
        )
        .expect("streamed writer");

        assert_eq!(streamed, owned.bytes);
        assert_eq!(report.report, owned.report);
        assert_eq!(report.mode, PersistentBatchMode::CopyOnWriteReplacements);
        assert_eq!(report.pages_written, owned.pages_written);
        assert_eq!(report.pages_reused, owned.pages_reused);
        assert_eq!(report.base_bytes, base.len());
        assert_eq!(report.tail_bytes, streamed.len() - base.len());
        assert_eq!(report.bytes_written, streamed.len());
        assert!(report.largest_write_request <= 31);
        assert!(report.tail_allocation_bytes < streamed.len());
    }

    #[test]
    fn caller_order_does_not_change_streamed_replacement_bytes() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(220), limits).expect("base");
        let forward = vec![
            ImmutableBatchOperation::Put(object(2, 211, 11)),
            ImmutableBatchOperation::Put(object(440, 212, 13)),
        ];
        let mut reverse = forward.clone();
        reverse.reverse();
        let mut first = Vec::new();
        let mut second = Vec::new();
        let first_report = append_persistent_replacement_batch_to(
            &mut first,
            &base,
            &forward,
            limits,
            PersistentMixedStreamingOptions::default(),
        )
        .expect("forward");
        let second_report = append_persistent_replacement_batch_to(
            &mut second,
            &base,
            &reverse,
            limits,
            PersistentMixedStreamingOptions::default(),
        )
        .expect("reverse");
        assert_eq!(first, second);
        assert_eq!(first_report, second_report);
    }

    #[test]
    fn unsupported_or_missing_operations_fail_before_output() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(8), limits).expect("base");
        for operations in [
            vec![ImmutableBatchOperation::Delete(2)],
            vec![ImmutableBatchOperation::Put(object(3, 9, 4))],
        ] {
            let mut sink = Vec::new();
            assert!(append_persistent_replacement_batch_to(
                &mut sink,
                &base,
                &operations,
                limits,
                PersistentMixedStreamingOptions::default(),
            )
            .is_err());
            assert!(sink.is_empty());
        }
    }
}
