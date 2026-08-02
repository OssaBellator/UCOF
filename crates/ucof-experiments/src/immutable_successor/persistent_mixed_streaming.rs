#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersistentMixedStreamingOptions {
    pub max_write_request_bytes: usize,
}

impl Default for PersistentMixedStreamingOptions {
    fn default() -> Self {
        Self {
            max_write_request_bytes: 64 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentMixedStreamingError {
    Format(ImmutableError),
    Io(std::io::ErrorKind),
}

impl std::fmt::Display for PersistentMixedStreamingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "persistent mixed streaming failed: {error}"),
            Self::Io(kind) => write!(formatter, "persistent mixed sink failed: {kind:?}"),
        }
    }
}

impl std::error::Error for PersistentMixedStreamingError {}

impl From<ImmutableError> for PersistentMixedStreamingError {
    fn from(error: ImmutableError) -> Self {
        Self::Format(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentMixedStreamingReport {
    pub report: ImmutableReport,
    pub mode: PersistentBatchMode,
    pub pages_written: usize,
    pub pages_reused: usize,
    pub base_bytes_written: u64,
    pub tail_bytes_written: u64,
    pub largest_write_request: usize,
    pub tail_allocation_bytes: usize,
}

#[derive(Clone, Debug)]
struct PersistentMixedTail {
    bytes: Vec<u8>,
    report: ImmutableReport,
    pages_written: usize,
    pages_reused: usize,
}

#[derive(Clone, Debug)]
struct PersistentTailPublication {
    base_len: usize,
    sequence: u64,
    root: PageRef,
    parent_snapshot_digest: [u8; 32],
    previous_footer_offset: u64,
    page_count: usize,
    object_count: usize,
}

fn persistent_tail_total_len(
    base_len: usize,
    tail_len: usize,
    limits: ImmutableLimits,
) -> Result<usize, ImmutableError> {
    let total = base_len
        .checked_add(tail_len)
        .ok_or(ImmutableError::Limit("output"))?;
    if total > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
    Ok(total)
}

fn append_persistent_tail_object(
    tail: &mut Vec<u8>,
    base_len: usize,
    input: &ImmutableObjectInput,
    limits: ImmutableLimits,
) -> Result<Locator, ImmutableError> {
    let record = encode_object(input)?;
    persistent_tail_total_len(
        base_len,
        tail.len()
            .checked_add(record.len())
            .ok_or(ImmutableError::Limit("output"))?,
        limits,
    )?;
    let absolute_offset = base_len
        .checked_add(tail.len())
        .ok_or(ImmutableError::Limit("output"))?;
    tail.extend_from_slice(&record);
    Ok(Locator {
        object_id: input.object_id,
        kind: input.kind,
        record_offset: u64_from_usize(absolute_offset)?,
        record_len: u64_from_usize(record.len())?,
        logical_len: u64_from_usize(input.payload.len())?,
        digest: digest(&[OBJECT_DOMAIN, &record]),
    })
}

fn apply_persistent_tail_operations(
    tail: &mut Vec<u8>,
    base_len: usize,
    operations: &[ImmutableBatchOperation],
    order: &[usize],
    previous: &InternalReport,
    limits: ImmutableLimits,
) -> Result<Vec<Locator>, ImmutableError> {
    let mut locators = previous.locators.clone();
    for index in order {
        match &operations[*index] {
            ImmutableBatchOperation::Put(input) => {
                let replacement =
                    append_persistent_tail_object(tail, base_len, input, limits)?;
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
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableError::Invalid("persistent mixed locator order"));
    }
    Ok(locators)
}

fn append_persistent_tail_page(
    tail: &mut Vec<u8>,
    base_len: usize,
    page: &[u8],
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<PageRef, ImmutableError> {
    if *pages_written >= limits.max_pages || page.len() != PAGE_SIZE {
        return Err(ImmutableError::Limit("page count"));
    }
    persistent_tail_total_len(
        base_len,
        tail.len()
            .checked_add(PAGE_SIZE)
            .ok_or(ImmutableError::Limit("output"))?,
        limits,
    )?;
    let absolute_offset = base_len
        .checked_add(tail.len())
        .ok_or(ImmutableError::Limit("output"))?;
    let reference = PageRef {
        minimum: u64_at(page, 20, "page")?,
        maximum: u64_at(page, 28, "page")?,
        offset: u64_from_usize(absolute_offset)?,
        level: page[9],
        digest: digest(&[PAGE_DOMAIN, page]),
    };
    tail.extend_from_slice(page);
    *pages_written += 1;
    Ok(reference)
}

fn materialize_persistent_tail_tree(
    tail: &mut Vec<u8>,
    base_len: usize,
    locators: &[Locator],
    originals: &[Vec<OriginalMixedPage>],
    limits: ImmutableLimits,
) -> Result<(PageRef, usize, usize), ImmutableError> {
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
            level.push(append_persistent_tail_page(
                tail,
                base_len,
                &encode_leaf(entries)?,
                limits,
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
                next.push(append_persistent_tail_page(
                    tail,
                    base_len,
                    &encode_internal(children, parent_level)?,
                    limits,
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

fn publish_persistent_tail(
    tail: &mut Vec<u8>,
    publication: PersistentTailPublication,
    limits: ImmutableLimits,
) -> Result<ImmutableReport, ImmutableError> {
    let commit_start = usize_from_u64(publication.previous_footer_offset, "previous footer")?
        .checked_add(FOOTER_LEN)
        .ok_or(ImmutableError::Invalid("previous footer"))?;
    if commit_start != publication.base_len {
        return Err(ImmutableError::Invalid("persistent mixed exact end"));
    }
    let publication_len = SNAPSHOT_LEN
        .checked_add(FOOTER_LEN)
        .ok_or(ImmutableError::Limit("output"))?;
    persistent_tail_total_len(
        publication.base_len,
        tail.len()
            .checked_add(publication_len)
            .ok_or(ImmutableError::Limit("output"))?,
        limits,
    )?;

    let snapshot_offset = publication
        .base_len
        .checked_add(tail.len())
        .ok_or(ImmutableError::Limit("output"))?;
    let mut snapshot = vec![0_u8; SNAPSHOT_LEN];
    snapshot[..8].copy_from_slice(SNAPSHOT_MAGIC);
    put_u64(&mut snapshot, 8, publication.sequence);
    put_u64(&mut snapshot, 16, publication.root.offset);
    put_u64(&mut snapshot, 24, u64::from(publication.root.level));
    snapshot[32..64].copy_from_slice(&publication.root.digest);
    snapshot[64..].copy_from_slice(&publication.parent_snapshot_digest);
    let snapshot_digest = digest(&[SNAPSHOT_DOMAIN, &snapshot]);
    tail.extend_from_slice(&snapshot);

    let footer = Footer {
        sequence: publication.sequence,
        snapshot_offset: u64_from_usize(snapshot_offset)?,
        snapshot_len: u64_from_usize(SNAPSHOT_LEN)?,
        previous_footer_offset: publication.previous_footer_offset,
        page_count_current: u64_from_usize(publication.page_count)?,
        snapshot_digest,
        commit_digest: [0_u8; 32],
    };
    let semantics = footer_semantics(&footer);
    let commit_digest = digest(&[COMMIT_DOMAIN, tail.as_slice(), &semantics]);
    let mut raw = vec![0_u8; FOOTER_LEN];
    raw[..8].copy_from_slice(FOOTER_MAGIC);
    put_u64(&mut raw, 8, publication.sequence);
    put_u64(&mut raw, 16, u64_from_usize(snapshot_offset)?);
    put_u64(&mut raw, 24, u64_from_usize(SNAPSHOT_LEN)?);
    put_u64(&mut raw, 32, publication.previous_footer_offset);
    put_u64(&mut raw, 40, u64_from_usize(publication.page_count)?);
    raw[48..80].copy_from_slice(&snapshot_digest);
    raw[80..112].copy_from_slice(&commit_digest);
    tail.extend_from_slice(&raw);

    Ok(ImmutableReport {
        sequence: publication.sequence,
        object_count: publication.object_count,
        page_count: publication.page_count,
        root_level: publication.root.level,
        snapshot_digest,
        commit_digest,
    })
}

fn build_persistent_mixed_tail(
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
) -> Result<PersistentMixedTail, ImmutableError> {
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
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

    let mut tail = Vec::new();
    let locators = apply_persistent_tail_operations(
        &mut tail,
        data.len(),
        operations,
        &order,
        &previous,
        limits,
    )?;
    let (next_root, pages_written, pages_reused) = materialize_persistent_tail_tree(
        &mut tail,
        data.len(),
        &locators,
        &originals,
        limits,
    )?;
    let active_page_count = pages_written
        .checked_add(pages_reused)
        .ok_or(ImmutableError::Limit("page count"))?;
    let publication = PersistentTailPublication {
        base_len: data.len(),
        sequence: previous
            .public
            .sequence
            .checked_add(1)
            .ok_or(ImmutableError::Limit("sequence"))?,
        root: next_root,
        parent_snapshot_digest: previous.public.snapshot_digest,
        previous_footer_offset: u64_from_usize(previous.footer_offset)?,
        page_count: pages_written,
        object_count: locators.len(),
    };
    let mut report = publish_persistent_tail(&mut tail, publication, limits)?;
    report.page_count = active_page_count;
    Ok(PersistentMixedTail {
        bytes: tail,
        report,
        pages_written,
        pages_reused,
    })
}

fn write_persistent_mixed_chunked<W: std::io::Write>(
    writer: &mut W,
    bytes: &[u8],
    max_request: usize,
    largest_request: &mut usize,
) -> Result<(), PersistentMixedStreamingError> {
    for chunk in bytes.chunks(max_request) {
        *largest_request = (*largest_request).max(chunk.len());
        writer
            .write_all(chunk)
            .map_err(|error| PersistentMixedStreamingError::Io(error.kind()))?;
    }
    Ok(())
}

/// Appends one canonical persistent mixed commit through a bounded sequential sink without owning a
/// second complete successor-file buffer.
///
/// Strict canonical validation, operation canonicalization, page-reuse decisions, complete tail
/// construction, output-size checks, and commit hashing all finish before the first sink write. The
/// verified base slice is then copied directly to the sink in bounded requests, followed by the
/// newly allocated append tail. The tail contains only inserted/replacement records, changed pages,
/// snapshot, and footer. Exact current page references remain reusable. Sink failure after output
/// begins is terminal and returns no report; atomic visibility requires separate private staging.
pub fn append_persistent_mixed_batch_to<W: std::io::Write>(
    writer: &mut W,
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
    options: PersistentMixedStreamingOptions,
) -> Result<PersistentMixedStreamingReport, PersistentMixedStreamingError> {
    if options.max_write_request_bytes == 0 {
        return Err(ImmutableError::Invalid("write request").into());
    }
    let tail = build_persistent_mixed_tail(data, operations, limits)?;
    let mut largest_write_request = 0_usize;
    write_persistent_mixed_chunked(
        writer,
        data,
        options.max_write_request_bytes,
        &mut largest_write_request,
    )?;
    write_persistent_mixed_chunked(
        writer,
        &tail.bytes,
        options.max_write_request_bytes,
        &mut largest_write_request,
    )?;
    Ok(PersistentMixedStreamingReport {
        report: tail.report,
        mode: PersistentBatchMode::CopyOnWriteCanonicalMixed,
        pages_written: tail.pages_written,
        pages_reused: tail.pages_reused,
        base_bytes_written: u64_from_usize(data.len())?,
        tail_bytes_written: u64_from_usize(tail.bytes.len())?,
        largest_write_request,
        tail_allocation_bytes: tail.bytes.len(),
    })
}

#[cfg(test)]
mod persistent_mixed_streaming_tests {
    use super::*;

    fn object(object_id: u64, seed: u8, length: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(object_id, 1 + u16::from(seed % 31), vec![seed; length])
    }

    fn even_objects(count: usize) -> Vec<ImmutableObjectInput> {
        (0..count)
            .map(|index| {
                object(
                    u64::try_from((index + 1) * 2).expect("small object id"),
                    u8::try_from(index % 251).expect("seed"),
                    17 + index % 29,
                )
            })
            .collect()
    }

    #[test]
    fn streaming_mixed_output_matches_owned_writer_byte_for_byte() {
        let limits = ImmutableLimits {
            max_file_bytes: 64 * 1024 * 1024,
            max_output_bytes: 64 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = build_genesis(&even_objects(400), limits).expect("base");
        let operations = vec![
            ImmutableBatchOperation::Delete(20),
            ImmutableBatchOperation::Put(object(200, 91, 73)),
            ImmutableBatchOperation::Put(object(741, 17, 41)),
        ];
        let expected =
            append_persistent_mixed_batch(&base, &operations, limits).expect("owned mixed");
        let mut actual = Vec::new();
        let report = append_persistent_mixed_batch_to(
            &mut actual,
            &base,
            &operations,
            limits,
            PersistentMixedStreamingOptions {
                max_write_request_bytes: 37,
            },
        )
        .expect("streamed mixed");
        assert_eq!(actual, expected.bytes);
        assert_eq!(report.report, expected.report);
        assert_eq!(report.pages_written, expected.pages_written);
        assert_eq!(report.pages_reused, expected.pages_reused);
        assert_eq!(report.mode, expected.mode);
        assert_eq!(report.base_bytes_written, base.len() as u64);
        assert_eq!(
            report.tail_bytes_written,
            u64::try_from(actual.len() - base.len()).expect("tail")
        );
        assert_eq!(report.tail_allocation_bytes, actual.len() - base.len());
        assert!(report.tail_allocation_bytes < actual.len());
        assert!(report.largest_write_request <= 37);
        validate_canonical_occupancy(&actual, limits).expect("streamed validation");
    }

    #[test]
    fn reused_pages_do_not_inflate_footer_current_page_count() {
        let limits = ImmutableLimits {
            max_file_bytes: 8 * 1024 * 1024,
            max_objects: 2 * LEAF_CAPACITY + 16,
            max_pages: 128,
            max_depth: 4,
            max_allocation_bytes: 8 * 1024 * 1024,
            max_output_bytes: 8 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let objects: Vec<_> = (1..=253_u64)
            .map(|index| {
                let seed = if index % 3 == 0 {
                    202
                } else {
                    u8::try_from(index).expect("bounded fuzz regression index")
                };
                object(index * 2, seed, 1 + usize::from(seed % 64))
            })
            .collect();
        let base = build_genesis(&objects, limits).expect("fuzz regression base");
        let operations = vec![
            ImmutableBatchOperation::Delete(406),
            ImmutableBatchOperation::Put(object(408, 17, 18)),
            ImmutableBatchOperation::Put(object(507, 29, 30)),
        ];
        let expected =
            append_persistent_mixed_batch(&base, &operations, limits).expect("owned mixed");
        assert!(expected.pages_reused > 0);

        let mut actual = Vec::new();
        let report = append_persistent_mixed_batch_to(
            &mut actual,
            &base,
            &operations,
            limits,
            PersistentMixedStreamingOptions {
                max_write_request_bytes: 54,
            },
        )
        .expect("streamed mixed");

        assert_eq!(actual, expected.bytes);
        assert_eq!(report.report, expected.report);
        assert_eq!(report.pages_written, expected.pages_written);
        assert_eq!(report.pages_reused, expected.pages_reused);
        assert_eq!(
            report.report.page_count,
            report
                .pages_written
                .checked_add(report.pages_reused)
                .expect("active page count")
        );
        let footer = parse_footer(&actual, actual.len() - FOOTER_LEN).expect("footer");
        assert_eq!(
            footer.page_count_current,
            u64::try_from(report.pages_written).expect("current pages")
        );
    }

    #[test]
    fn invalid_request_fails_before_output() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(8), limits).expect("base");
        let mut sink = Vec::new();
        assert!(append_persistent_mixed_batch_to(
            &mut sink,
            &base,
            &[ImmutableBatchOperation::Delete(999)],
            limits,
            PersistentMixedStreamingOptions::default(),
        )
        .is_err());
        assert!(sink.is_empty());
    }

    #[derive(Debug)]
    struct FailingWriter {
        remaining: usize,
        bytes: Vec<u8>,
    }

    impl std::io::Write for FailingWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("injected sink failure"));
            }
            let take = buffer.len().min(self.remaining);
            self.bytes.extend_from_slice(&buffer[..take]);
            self.remaining -= take;
            Ok(take)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn sink_failure_is_terminal_and_returns_no_report() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(16), limits).expect("base");
        let operations = vec![
            ImmutableBatchOperation::Delete(4),
            ImmutableBatchOperation::Put(object(3, 77, 29)),
        ];
        let mut sink = FailingWriter {
            remaining: base.len() / 2,
            bytes: Vec::new(),
        };
        assert_eq!(
            append_persistent_mixed_batch_to(
                &mut sink,
                &base,
                &operations,
                limits,
                PersistentMixedStreamingOptions {
                    max_write_request_bytes: 31,
                },
            ),
            Err(PersistentMixedStreamingError::Io(
                std::io::ErrorKind::Other
            ))
        );
        assert!(!sink.bytes.is_empty());
        assert!(sink.bytes.len() < base.len());
    }
}
