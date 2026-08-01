#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersistentMixedWriteOptions {
    pub max_write_request_bytes: usize,
}

impl Default for PersistentMixedWriteOptions {
    fn default() -> Self {
        Self {
            max_write_request_bytes: 64 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentMixedSuffixResult {
    pub prefix_len: usize,
    pub suffix: Vec<u8>,
    pub report: ImmutableReport,
    pub pages_written: usize,
    pub pages_reused: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentMixedWriteReport {
    pub report: ImmutableReport,
    pub prefix_bytes_written: usize,
    pub suffix_bytes_written: usize,
    pub largest_write_request: usize,
    pub pages_written: usize,
    pub pages_reused: usize,
}

#[derive(Debug)]
pub enum PersistentMixedWriteError {
    Format(ImmutableError),
    Output(std::io::Error),
}

impl std::fmt::Display for PersistentMixedWriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "persistent mixed suffix failed: {error}"),
            Self::Output(error) => write!(formatter, "persistent mixed output failed: {error}"),
        }
    }
}

impl std::error::Error for PersistentMixedWriteError {}

impl From<ImmutableError> for PersistentMixedWriteError {
    fn from(error: ImmutableError) -> Self {
        Self::Format(error)
    }
}

struct MixedSuffixBuffer {
    base_offset: usize,
    bytes: Vec<u8>,
    limits: ImmutableLimits,
}

impl MixedSuffixBuffer {
    fn new(base_offset: usize, limits: ImmutableLimits) -> Result<Self, ImmutableError> {
        if base_offset > limits.max_output_bytes || base_offset > limits.max_file_bytes {
            return Err(ImmutableError::Limit("output"));
        }
        Ok(Self {
            base_offset,
            bytes: Vec::new(),
            limits,
        })
    }

    fn absolute_offset(&self) -> Result<usize, ImmutableError> {
        self.base_offset
            .checked_add(self.bytes.len())
            .ok_or(ImmutableError::Limit("output"))
    }

    fn append(&mut self, bytes: &[u8]) -> Result<usize, ImmutableError> {
        let offset = self.absolute_offset()?;
        let end = offset
            .checked_add(bytes.len())
            .ok_or(ImmutableError::Limit("output"))?;
        if end > self.limits.max_output_bytes || end > self.limits.max_file_bytes {
            return Err(ImmutableError::Limit("output"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(offset)
    }

    fn append_object(&mut self, input: &ImmutableObjectInput) -> Result<Locator, ImmutableError> {
        let record = encode_object(input)?;
        let offset = self.append(&record)?;
        Ok(Locator {
            object_id: input.object_id,
            kind: input.kind,
            record_offset: u64_from_usize(offset)?,
            record_len: u64_from_usize(record.len())?,
            logical_len: u64_from_usize(input.payload.len())?,
            digest: digest(&[OBJECT_DOMAIN, &record]),
        })
    }

    fn append_page(&mut self, page: &[u8]) -> Result<PageRef, ImmutableError> {
        if page.len() != PAGE_SIZE {
            return Err(ImmutableError::Invalid("page size"));
        }
        let offset = self.append(page)?;
        Ok(PageRef {
            minimum: u64_at(page, 20, "page")?,
            maximum: u64_at(page, 28, "page")?,
            offset: u64_from_usize(offset)?,
            level: page[9],
            digest: digest(&[PAGE_DOMAIN, page]),
        })
    }
}

fn apply_mixed_suffix_operations(
    suffix: &mut MixedSuffixBuffer,
    operations: &[ImmutableBatchOperation],
    order: &[usize],
    previous: &InternalReport,
) -> Result<Vec<Locator>, ImmutableError> {
    let mut locators = previous.locators.clone();
    for index in order {
        match &operations[*index] {
            ImmutableBatchOperation::Put(input) => {
                let replacement = suffix.append_object(input)?;
                match locators
                    .binary_search_by_key(&input.object_id, |locator| locator.object_id)
                {
                    Ok(position) => locators[position] = replacement,
                    Err(position) => locators.insert(position, replacement),
                }
            }
            ImmutableBatchOperation::Delete(object_id) => {
                let position = locators
                    .binary_search_by_key(object_id, |locator| locator.object_id)
                    .map_err(|_| ImmutableError::MissingObject(*object_id))?;
                locators.remove(position);
            }
        }
    }
    Ok(locators)
}

fn append_mixed_suffix_page(
    suffix: &mut MixedSuffixBuffer,
    page: &[u8],
    pages_written: &mut usize,
) -> Result<PageRef, ImmutableError> {
    if *pages_written >= suffix.limits.max_pages {
        return Err(ImmutableError::Limit("page count"));
    }
    let reference = suffix.append_page(page)?;
    *pages_written += 1;
    Ok(reference)
}

fn materialize_mixed_suffix_tree(
    suffix: &mut MixedSuffixBuffer,
    locators: &[Locator],
    originals: &[Vec<OriginalMixedPage>],
) -> Result<(PageRef, usize, usize), ImmutableError> {
    let limits = suffix.limits;
    let leaf_sizes = canonical_group_sizes(
        locators.len(),
        LEAF_CAPACITY,
        LEAF_MIN_OCCUPANCY,
        limits,
    )?;
    allocation_check::<PageRef>(leaf_sizes.len(), limits)?;
    let mut pages_written = 0_usize;
    let mut pages_reused = 0_usize;
    let mut level = Vec::with_capacity(leaf_sizes.len());
    let mut start = 0_usize;
    for size in leaf_sizes {
        let end = start
            .checked_add(size)
            .ok_or(ImmutableError::Limit("object count"))?;
        let entries = &locators[start..end];
        if let Some(reference) = reusable_mixed_leaf(originals, entries) {
            pages_reused += 1;
            level.push(reference);
        } else {
            level.push(append_mixed_suffix_page(
                suffix,
                &encode_leaf(entries)?,
                &mut pages_written,
            )?);
        }
        start = end;
    }

    while level.len() > 1 {
        let parent_level = level[0]
            .level
            .checked_add(1)
            .ok_or(ImmutableError::Limit("page depth"))?;
        if parent_level > limits.max_depth {
            return Err(ImmutableError::Limit("page depth"));
        }
        let group_sizes = canonical_group_sizes(
            level.len(),
            INTERNAL_FANOUT,
            INTERNAL_MIN_OCCUPANCY,
            limits,
        )?;
        allocation_check::<PageRef>(group_sizes.len(), limits)?;
        let mut next = Vec::with_capacity(group_sizes.len());
        let mut start = 0_usize;
        for size in group_sizes {
            let end = start
                .checked_add(size)
                .ok_or(ImmutableError::Limit("page count"))?;
            let children = &level[start..end];
            if let Some(reference) = reusable_mixed_internal(originals, parent_level, children) {
                pages_reused += 1;
                next.push(reference);
            } else {
                next.push(append_mixed_suffix_page(
                    suffix,
                    &encode_internal(children, parent_level)?,
                    &mut pages_written,
                )?);
            }
            start = end;
        }
        level = next;
    }

    Ok((
        level
            .pop()
            .ok_or(ImmutableError::Invalid("persistent mixed root"))?,
        pages_written,
        pages_reused,
    ))
}

fn publish_mixed_suffix(
    suffix: &mut MixedSuffixBuffer,
    previous: &InternalReport,
    root: &PageRef,
    page_count: usize,
) -> Result<ImmutableReport, ImmutableError> {
    let previous_footer_offset = u64_from_usize(previous.footer_offset)?;
    let commit_start = previous
        .footer_offset
        .checked_add(FOOTER_LEN)
        .ok_or(ImmutableError::Limit("output"))?;
    if commit_start != suffix.base_offset {
        return Err(ImmutableError::Invalid("persistent append boundary"));
    }
    let sequence = previous
        .public
        .sequence
        .checked_add(1)
        .ok_or(ImmutableError::Limit("sequence"))?;
    let snapshot_offset = u64_from_usize(suffix.absolute_offset()?)?;
    let mut snapshot = vec![0_u8; SNAPSHOT_LEN];
    snapshot[..8].copy_from_slice(SNAPSHOT_MAGIC);
    put_u64(&mut snapshot, 8, sequence);
    put_u64(&mut snapshot, 16, root.offset);
    put_u64(&mut snapshot, 24, u64::from(root.level));
    snapshot[32..64].copy_from_slice(&root.digest);
    snapshot[64..].copy_from_slice(&previous.public.snapshot_digest);
    let snapshot_digest = digest(&[SNAPSHOT_DOMAIN, &snapshot]);
    suffix.append(&snapshot)?;

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
    let commit_digest = digest(&[COMMIT_DOMAIN, &suffix.bytes, &semantics]);
    let mut raw = vec![0_u8; FOOTER_LEN];
    raw[..8].copy_from_slice(FOOTER_MAGIC);
    put_u64(&mut raw, 8, sequence);
    put_u64(&mut raw, 16, snapshot_offset);
    put_u64(&mut raw, 24, u64_from_usize(SNAPSHOT_LEN)?);
    put_u64(&mut raw, 32, previous_footer_offset);
    put_u64(&mut raw, 40, u64_from_usize(page_count)?);
    raw[48..80].copy_from_slice(&snapshot_digest);
    raw[80..112].copy_from_slice(&commit_digest);
    suffix.append(&raw)?;

    Ok(ImmutableReport {
        sequence,
        object_count: 0,
        page_count,
        root_level: root.level,
        snapshot_digest,
        commit_digest,
    })
}

/// Builds only the deterministic append region for a persistent mixed batch.
///
/// The existing file remains borrowed. New locators and pages use absolute offsets based on the
/// borrowed prefix length. Exact current page bodies are reused by reference. Concatenating `data`
/// and the returned suffix yields the same bytes as `append_persistent_mixed_batch` without cloning
/// the complete predecessor or successor file.
pub fn append_persistent_mixed_suffix(
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
) -> Result<PersistentMixedSuffixResult, ImmutableError> {
    let previous = validate_canonical_internal(data, limits)?;
    let order = canonical_mixed_operation_order(operations, &previous, limits)?;
    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut originals = vec![Vec::new(); usize::from(root.level) + 1];
    let mut visited = 0_usize;
    collect_original_mixed_pages(
        data,
        &root,
        &mut originals,
        limits,
        &mut visited,
    )?;
    if visited != previous.public.page_count {
        return Err(ImmutableError::Invalid("persistent mixed page inventory"));
    }

    let mut suffix = MixedSuffixBuffer::new(data.len(), limits)?;
    let locators = apply_mixed_suffix_operations(
        &mut suffix,
        operations,
        &order,
        &previous,
    )?;
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableError::Invalid("persistent mixed locator order"));
    }
    let (next_root, pages_written, pages_reused) =
        materialize_mixed_suffix_tree(&mut suffix, &locators, &originals)?;
    let page_count = pages_written
        .checked_add(pages_reused)
        .ok_or(ImmutableError::Limit("page count"))?;
    let mut report = publish_mixed_suffix(&mut suffix, &previous, &next_root, page_count)?;
    report.object_count = locators.len();

    Ok(PersistentMixedSuffixResult {
        prefix_len: data.len(),
        suffix: suffix.bytes,
        report,
        pages_written,
        pages_reused,
    })
}

fn write_mixed_chunks<W: std::io::Write>(
    writer: &mut W,
    bytes: &[u8],
    chunk: usize,
    largest: &mut usize,
) -> Result<(), std::io::Error> {
    for part in bytes.chunks(chunk) {
        writer.write_all(part)?;
        *largest = (*largest).max(part.len());
    }
    Ok(())
}

/// Writes the borrowed predecessor followed by its deterministic mixed append suffix in bounded
/// requests. Sink failure is terminal and returns no publication report; private staging is required
/// before this output can become atomically visible.
pub fn write_persistent_mixed_batch_to<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
    options: PersistentMixedWriteOptions,
) -> Result<PersistentMixedWriteReport, PersistentMixedWriteError> {
    if options.max_write_request_bytes == 0 {
        return Err(ImmutableError::Limit("write request").into());
    }
    let result = append_persistent_mixed_suffix(data, operations, limits)?;
    let mut largest_write_request = 0_usize;
    write_mixed_chunks(
        writer,
        data,
        options.max_write_request_bytes,
        &mut largest_write_request,
    )
    .map_err(PersistentMixedWriteError::Output)?;
    write_mixed_chunks(
        writer,
        &result.suffix,
        options.max_write_request_bytes,
        &mut largest_write_request,
    )
    .map_err(PersistentMixedWriteError::Output)?;
    Ok(PersistentMixedWriteReport {
        report: result.report,
        prefix_bytes_written: data.len(),
        suffix_bytes_written: result.suffix.len(),
        largest_write_request,
        pages_written: result.pages_written,
        pages_reused: result.pages_reused,
    })
}
