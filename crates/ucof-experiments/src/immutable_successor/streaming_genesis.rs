use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableStreamingWriteOptions {
    pub max_write_request_bytes: usize,
}

impl Default for ImmutableStreamingWriteOptions {
    fn default() -> Self {
        Self {
            max_write_request_bytes: 64 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableStreamingWriteReport {
    pub report: ImmutableReport,
    pub bytes_written: usize,
    pub largest_write_request: usize,
    pub locator_entries: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableStreamingWriteError {
    Format(ImmutableError),
    Io(&'static str),
}

impl fmt::Display for ImmutableStreamingWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "{error}"),
            Self::Io(label) => write!(formatter, "streaming output failed: {label}"),
        }
    }
}

impl Error for ImmutableStreamingWriteError {}

impl From<ImmutableError> for ImmutableStreamingWriteError {
    fn from(error: ImmutableError) -> Self {
        Self::Format(error)
    }
}

struct StreamingSink<'a, W> {
    writer: &'a mut W,
    offset: usize,
    max_write_request_bytes: usize,
    largest_write_request: usize,
    commit_hasher: Sha256,
}

impl<'a, W: Write> StreamingSink<'a, W> {
    fn new(
        writer: &'a mut W,
        max_write_request_bytes: usize,
    ) -> Result<Self, ImmutableStreamingWriteError> {
        if max_write_request_bytes == 0 {
            return Err(ImmutableError::Limit("write request").into());
        }
        let mut commit_hasher = Sha256::new();
        commit_hasher.update(COMMIT_DOMAIN);
        Ok(Self {
            writer,
            offset: 0,
            max_write_request_bytes,
            largest_write_request: 0,
            commit_hasher,
        })
    }

    fn write_bytes(
        &mut self,
        bytes: &[u8],
        include_in_commit: bool,
    ) -> Result<(), ImmutableStreamingWriteError> {
        for chunk in bytes.chunks(self.max_write_request_bytes) {
            self.writer
                .write_all(chunk)
                .map_err(|_| ImmutableStreamingWriteError::Io("write"))?;
            if include_in_commit {
                self.commit_hasher.update(chunk);
            }
            self.offset = self
                .offset
                .checked_add(chunk.len())
                .ok_or(ImmutableError::Limit("output"))?;
            self.largest_write_request = self.largest_write_request.max(chunk.len());
        }
        Ok(())
    }

    fn write_commit_bytes(&mut self, bytes: &[u8]) -> Result<(), ImmutableStreamingWriteError> {
        self.write_bytes(bytes, true)
    }

    fn write_footer(&mut self, bytes: &[u8]) -> Result<(), ImmutableStreamingWriteError> {
        self.write_bytes(bytes, false)
    }

    fn write_page(
        &mut self,
        page: &[u8],
    ) -> Result<PageRef, ImmutableStreamingWriteError> {
        if page.len() != PAGE_SIZE {
            return Err(ImmutableError::Invalid("streaming page").into());
        }
        let reference = PageRef {
            minimum: u64_at(page, 20, "page")?,
            maximum: u64_at(page, 28, "page")?,
            offset: u64_from_usize(self.offset)?,
            level: page[9],
            digest: digest(&[PAGE_DOMAIN, page]),
        };
        self.write_commit_bytes(page)?;
        Ok(reference)
    }
}

fn streaming_tree_shape(
    object_count: usize,
    limits: ImmutableLimits,
) -> Result<(usize, u8), ImmutableError> {
    let leaf_sizes = canonical_group_sizes(
        object_count,
        LEAF_CAPACITY,
        LEAF_MIN_OCCUPANCY,
        limits,
    )?;
    let mut page_count = leaf_sizes.len();
    let mut current = leaf_sizes.len();
    let mut root_level = 0_u8;
    while current > 1 {
        root_level = root_level
            .checked_add(1)
            .ok_or(ImmutableError::Limit("page depth"))?;
        if root_level > limits.max_depth {
            return Err(ImmutableError::Limit("page depth"));
        }
        let groups = canonical_group_sizes(
            current,
            INTERNAL_FANOUT,
            INTERNAL_MIN_OCCUPANCY,
            limits,
        )?;
        page_count = page_count
            .checked_add(groups.len())
            .ok_or(ImmutableError::Limit("page count"))?;
        if page_count > limits.max_pages {
            return Err(ImmutableError::Limit("page count"));
        }
        current = groups.len();
    }
    Ok((page_count, root_level))
}

fn preflight_streaming_genesis(
    inputs: &[ImmutableObjectInput],
    options: ImmutableStreamingWriteOptions,
    limits: ImmutableLimits,
) -> Result<(Vec<usize>, usize, usize, u8), ImmutableStreamingWriteError> {
    if inputs.is_empty() || inputs.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count").into());
    }
    if options.max_write_request_bytes == 0 {
        return Err(ImmutableError::Limit("write request").into());
    }
    allocation_check::<usize>(inputs.len(), limits)?;
    allocation_check::<Locator>(inputs.len(), limits)?;

    let mut order: Vec<usize> = (0..inputs.len()).collect();
    order.sort_unstable_by_key(|index| inputs[*index].object_id);
    if let Some(pair) = order
        .windows(2)
        .find(|pair| inputs[pair[0]].object_id == inputs[pair[1]].object_id)
    {
        return Err(ImmutableError::DuplicateObject(inputs[pair[0]].object_id).into());
    }

    let mut object_bytes = 0_usize;
    for index in &order {
        let input = &inputs[*index];
        if input.object_id == 0 || input.kind == 0 {
            return Err(ImmutableError::Invalid("object input").into());
        }
        let record_len = OBJECT_HEADER_LEN
            .checked_add(input.payload.len())
            .ok_or(ImmutableError::Limit("object size"))?;
        object_bytes = object_bytes
            .checked_add(record_len)
            .ok_or(ImmutableError::Limit("output"))?;
    }

    let (page_count, root_level) = streaming_tree_shape(inputs.len(), limits)?;
    let page_bytes = page_count
        .checked_mul(PAGE_SIZE)
        .ok_or(ImmutableError::Limit("output"))?;
    let expected_bytes = FILE_HEADER_LEN
        .checked_add(object_bytes)
        .and_then(|value| value.checked_add(page_bytes))
        .and_then(|value| value.checked_add(SNAPSHOT_LEN))
        .and_then(|value| value.checked_add(FOOTER_LEN))
        .ok_or(ImmutableError::Limit("output"))?;
    if expected_bytes > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output").into());
    }
    if expected_bytes > limits.max_file_bytes {
        return Err(ImmutableError::Limit("file size").into());
    }
    Ok((order, expected_bytes, page_count, root_level))
}

fn write_streaming_object<W: Write>(
    sink: &mut StreamingSink<'_, W>,
    input: &ImmutableObjectInput,
) -> Result<Locator, ImmutableStreamingWriteError> {
    let mut header = [0_u8; OBJECT_HEADER_LEN];
    header[..8].copy_from_slice(OBJECT_MAGIC);
    put_u16(
        &mut header,
        8,
        u16::try_from(OBJECT_HEADER_LEN).map_err(|_| ImmutableError::Limit("object header"))?,
    );
    put_u16(&mut header, 10, input.kind);
    put_u64(&mut header, 16, input.object_id);
    put_u64(&mut header, 24, u64_from_usize(input.payload.len())?);
    put_u64(&mut header, 32, u64_from_usize(input.payload.len())?);

    let offset = u64_from_usize(sink.offset)?;
    let record_len = OBJECT_HEADER_LEN
        .checked_add(input.payload.len())
        .ok_or(ImmutableError::Limit("object size"))?;
    let mut object_hasher = Sha256::new();
    object_hasher.update(OBJECT_DOMAIN);
    object_hasher.update(header);
    object_hasher.update(&input.payload);
    sink.write_commit_bytes(&header)?;
    sink.write_commit_bytes(&input.payload)?;
    Ok(Locator {
        object_id: input.object_id,
        kind: input.kind,
        record_offset: offset,
        record_len: u64_from_usize(record_len)?,
        logical_len: u64_from_usize(input.payload.len())?,
        digest: object_hasher.finalize().into(),
    })
}

fn write_streaming_tree<W: Write>(
    sink: &mut StreamingSink<'_, W>,
    locators: &[Locator],
    limits: ImmutableLimits,
) -> Result<(PageRef, usize), ImmutableStreamingWriteError> {
    let leaf_sizes = canonical_group_sizes(
        locators.len(),
        LEAF_CAPACITY,
        LEAF_MIN_OCCUPANCY,
        limits,
    )?;
    allocation_check::<PageRef>(leaf_sizes.len(), limits)?;
    let mut pages = 0_usize;
    let mut level = Vec::with_capacity(leaf_sizes.len());
    let mut start = 0_usize;
    for size in leaf_sizes {
        let end = start
            .checked_add(size)
            .ok_or(ImmutableError::Limit("object count"))?;
        level.push(sink.write_page(&encode_leaf(&locators[start..end])?)?);
        pages += 1;
        start = end;
    }

    while level.len() > 1 {
        let parent_level = level[0]
            .level
            .checked_add(1)
            .ok_or(ImmutableError::Limit("page depth"))?;
        if parent_level > limits.max_depth {
            return Err(ImmutableError::Limit("page depth").into());
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
            next.push(sink.write_page(&encode_internal(
                &level[start..end],
                parent_level,
            )?)?);
            pages += 1;
            start = end;
        }
        level = next;
    }
    Ok((
        level
            .pop()
            .ok_or(ImmutableError::Invalid("streaming root"))?,
        pages,
    ))
}

fn write_streaming_publication<W: Write>(
    sink: &mut StreamingSink<'_, W>,
    root: &PageRef,
    page_count: usize,
) -> Result<ImmutableReport, ImmutableStreamingWriteError> {
    let snapshot_offset = u64_from_usize(sink.offset)?;
    let mut snapshot = [0_u8; SNAPSHOT_LEN];
    snapshot[..8].copy_from_slice(SNAPSHOT_MAGIC);
    put_u64(&mut snapshot, 8, 0);
    put_u64(&mut snapshot, 16, root.offset);
    put_u64(&mut snapshot, 24, u64::from(root.level));
    snapshot[32..64].copy_from_slice(&root.digest);
    let snapshot_digest = digest(&[SNAPSHOT_DOMAIN, &snapshot]);
    sink.write_commit_bytes(&snapshot)?;

    let footer = Footer {
        sequence: 0,
        snapshot_offset,
        snapshot_len: u64_from_usize(SNAPSHOT_LEN)?,
        previous_footer_offset: ABSENT_OFFSET,
        page_count_current: u64_from_usize(page_count)?,
        snapshot_digest,
        commit_digest: [0_u8; 32],
    };
    let semantics = footer_semantics(&footer);
    let mut commit_hasher = sink.commit_hasher.clone();
    commit_hasher.update(semantics);
    let commit_digest: [u8; 32] = commit_hasher.finalize().into();

    let mut raw = [0_u8; FOOTER_LEN];
    raw[..8].copy_from_slice(FOOTER_MAGIC);
    put_u64(&mut raw, 8, 0);
    put_u64(&mut raw, 16, snapshot_offset);
    put_u64(&mut raw, 24, u64_from_usize(SNAPSHOT_LEN)?);
    put_u64(&mut raw, 32, ABSENT_OFFSET);
    put_u64(&mut raw, 40, u64_from_usize(page_count)?);
    raw[48..80].copy_from_slice(&snapshot_digest);
    raw[80..112].copy_from_slice(&commit_digest);
    sink.write_footer(&raw)?;

    Ok(ImmutableReport {
        sequence: 0,
        object_count: 0,
        page_count,
        root_level: root.level,
        snapshot_digest,
        commit_digest,
    })
}

/// Writes a canonical genesis file to a bounded sequential sink without materializing the complete
/// output in one `Vec<u8>`.
///
/// Input payloads remain caller-owned and locator metadata remains `O(object_count)`. The function
/// preflights deterministic output size, object validity, duplicate identifiers, depth, and page
/// limits before the first write. I/O failure is terminal and never returns a success report; callers
/// requiring atomic visibility must still stage and publish the sink through an appropriate
/// filesystem or object-store protocol.
pub fn write_genesis_to<W: Write>(
    writer: &mut W,
    inputs: &[ImmutableObjectInput],
    options: ImmutableStreamingWriteOptions,
    limits: ImmutableLimits,
) -> Result<ImmutableStreamingWriteReport, ImmutableStreamingWriteError> {
    let (order, expected_bytes, expected_pages, expected_root_level) =
        preflight_streaming_genesis(inputs, options, limits)?;
    let mut sink = StreamingSink::new(writer, options.max_write_request_bytes)?;

    let mut header = [0_u8; FILE_HEADER_LEN];
    header[..8].copy_from_slice(FILE_MAGIC);
    sink.write_commit_bytes(&header)?;

    let mut locators = Vec::with_capacity(order.len());
    for index in order {
        locators.push(write_streaming_object(&mut sink, &inputs[index])?);
    }
    let (root, page_count) = write_streaming_tree(&mut sink, &locators, limits)?;
    if page_count != expected_pages || root.level != expected_root_level {
        return Err(ImmutableError::Invalid("streaming tree shape").into());
    }
    let mut report = write_streaming_publication(&mut sink, &root, page_count)?;
    report.object_count = locators.len();
    if sink.offset != expected_bytes {
        return Err(ImmutableError::Invalid("streaming output length").into());
    }
    Ok(ImmutableStreamingWriteReport {
        report,
        bytes_written: sink.offset,
        largest_write_request: sink.largest_write_request,
        locator_entries: locators.len(),
    })
}

#[cfg(test)]
mod streaming_genesis_tests {
    use super::*;

    fn object(object_id: u64) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 17).expect("kind"),
            vec![u8::try_from(object_id % 251).expect("payload seed"); 257],
        )
    }

    #[test]
    fn streaming_genesis_matches_canonical_writer_byte_for_byte() {
        let limits = ImmutableLimits::default();
        for count in [1_usize, LEAF_CAPACITY, 400] {
            let inputs: Vec<_> = (1..=u64::try_from(count).expect("count"))
                .rev()
                .map(object)
                .collect();
            let expected = build_genesis(&inputs, limits).expect("canonical genesis");
            let mut actual = Vec::new();
            let report = write_genesis_to(
                &mut actual,
                &inputs,
                ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 113,
                },
                limits,
            )
            .expect("streaming genesis");
            assert_eq!(actual, expected);
            assert_eq!(
                report.report,
                validate_canonical_occupancy(&actual, limits).expect("canonical output")
            );
            assert_eq!(report.bytes_written, actual.len());
            assert!(report.largest_write_request <= 113);
            assert_eq!(report.locator_entries, count);
        }
    }

    #[test]
    fn preflight_failure_leaves_the_sink_untouched() {
        let limits = ImmutableLimits {
            max_output_bytes: 100,
            ..ImmutableLimits::default()
        };
        let mut sink = Vec::new();
        assert_eq!(
            write_genesis_to(
                &mut sink,
                &[object(1)],
                ImmutableStreamingWriteOptions::default(),
                limits,
            ),
            Err(ImmutableStreamingWriteError::Format(ImmutableError::Limit(
                "output"
            )))
        );
        assert!(sink.is_empty());
    }

    struct FailAfter {
        bytes: Vec<u8>,
        remaining: usize,
    }

    impl Write for FailAfter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("injected failure"));
            }
            let count = buffer.len().min(self.remaining);
            self.bytes.extend_from_slice(&buffer[..count]);
            self.remaining -= count;
            Ok(count)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn io_failure_never_returns_a_published_report() {
        let mut sink = FailAfter {
            bytes: Vec::new(),
            remaining: FILE_HEADER_LEN + 10,
        };
        assert_eq!(
            write_genesis_to(
                &mut sink,
                &[object(1)],
                ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 32,
                },
                ImmutableLimits::default(),
            ),
            Err(ImmutableStreamingWriteError::Io("write"))
        );
        assert!(sink.bytes.len() < FOOTER_LEN);
        assert_ne!(sink.bytes.get(..8), Some(FOOTER_MAGIC.as_slice()));
    }

    #[test]
    fn duplicate_and_invalid_inputs_fail_before_output() {
        let mut sink = Vec::new();
        assert_eq!(
            write_genesis_to(
                &mut sink,
                &[object(2), object(2)],
                ImmutableStreamingWriteOptions::default(),
                ImmutableLimits::default(),
            ),
            Err(ImmutableStreamingWriteError::Format(
                ImmutableError::DuplicateObject(2)
            ))
        );
        assert!(sink.is_empty());
        assert_eq!(
            write_genesis_to(
                &mut sink,
                &[ImmutableObjectInput::new(0, 1, Vec::new())],
                ImmutableStreamingWriteOptions::default(),
                ImmutableLimits::default(),
            ),
            Err(ImmutableStreamingWriteError::Format(ImmutableError::Invalid(
                "object input"
            )))
        );
        assert!(sink.is_empty());
    }
}
