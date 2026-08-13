use std::future::Future;
use std::pin::Pin;

/// Strict-validation report for one native async strong-version source operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncImmutableSourceStrictReport {
    pub source_version: StrongVersionToken,
    pub source_length: u64,
    pub strict: ImmutableSourceStrictReport,
}

/// Full-validation async source reader.
///
/// This intentionally mirrors the bounded `SourceReader` accounting used by the synchronous
/// validator. It acquires metadata exactly once and rechecks every accepted range against the same
/// strong source version and exact object length before parser bytes are accepted.
struct AsyncFullSourceReader<'a, S> {
    source: &'a mut S,
    version: StrongVersionToken,
    length: usize,
    length_u64: u64,
    limits: ImmutableSourceLimits,
    stats: ImmutableSourceStats,
}

impl<'a, S: AsyncStrongVersionReadAt> AsyncFullSourceReader<'a, S> {
    async fn new(
        source: &'a mut S,
        limits: ImmutableSourceLimits,
    ) -> Result<Self, AsyncImmutableSourceError> {
        if limits.max_read_request_bytes == 0 || limits.hash_block_bytes == 0 {
            return Err(ImmutableSourceError::Limit("configuration").into());
        }
        let metadata = source.metadata_async().await?;
        let version = StrongVersionToken::parse(metadata.version)?;
        let length = usize::try_from(metadata.length)
            .map_err(|_| ImmutableSourceError::Limit("length"))?;
        if length > limits.format.max_file_bytes {
            return Err(ImmutableSourceError::Format(ImmutableError::Limit("file size")).into());
        }
        Ok(Self {
            source,
            version,
            length,
            length_u64: metadata.length,
            limits,
            stats: ImmutableSourceStats::default(),
        })
    }

    async fn read_into(
        &mut self,
        offset: usize,
        buffer: &mut [u8],
        label: &'static str,
    ) -> Result<(), AsyncImmutableSourceError> {
        let end = offset
            .checked_add(buffer.len())
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(label)))?;
        if end > self.length {
            return Err(ImmutableSourceError::Format(ImmutableError::Invalid(label)).into());
        }
        let mut completed = 0_usize;
        while completed < buffer.len() {
            let take = (buffer.len() - completed).min(self.limits.max_read_request_bytes);
            if self.stats.read_operations >= self.limits.max_read_operations {
                return Err(ImmutableSourceError::Limit("read operations").into());
            }
            let take_u64 =
                u64::try_from(take).map_err(|_| ImmutableSourceError::Limit("read bytes"))?;
            let next_total = self
                .stats
                .bytes_read
                .checked_add(take_u64)
                .ok_or(ImmutableSourceError::Limit("read bytes"))?;
            if next_total > self.limits.max_total_bytes_read {
                return Err(ImmutableSourceError::Limit("read bytes").into());
            }
            let source_offset = offset
                .checked_add(completed)
                .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(label)))?;
            let source_offset_u64 =
                u64::try_from(source_offset).map_err(|_| ImmutableSourceError::Limit("offset"))?;
            let response = self
                .source
                .read_range_if_match_async(
                    &self.version,
                    source_offset_u64,
                    take,
                    self.length_u64,
                )
                .await?;
            let response_version = StrongVersionToken::parse(response.version)?;
            if response_version != self.version
                || response.offset != source_offset_u64
                || response.total_length != self.length_u64
                || response.body.len() != take
            {
                return Err(ConditionalSourceError::Protocol("async source range").into());
            }
            buffer[completed..completed + take].copy_from_slice(&response.body);
            self.stats.read_operations += 1;
            self.stats.bytes_read = next_total;
            completed += take;
        }
        Ok(())
    }

    async fn read_vec(
        &mut self,
        offset: usize,
        length: usize,
        label: &'static str,
    ) -> Result<Vec<u8>, AsyncImmutableSourceError> {
        if length > self.limits.format.max_allocation_bytes {
            return Err(
                ImmutableSourceError::Format(ImmutableError::Limit("allocation")).into(),
            );
        }
        self.stats.largest_allocation = self.stats.largest_allocation.max(length);
        let mut output = vec![0_u8; length];
        self.read_into(offset, &mut output, label).await?;
        Ok(output)
    }

    async fn hash_range(
        &mut self,
        hasher: &mut Sha256,
        offset: usize,
        length: usize,
        label: &'static str,
    ) -> Result<(), AsyncImmutableSourceError> {
        let block = self
            .limits
            .hash_block_bytes
            .min(self.limits.max_read_request_bytes)
            .min(self.limits.format.max_allocation_bytes);
        if block == 0 {
            return Err(ImmutableSourceError::Limit("hash block").into());
        }
        self.stats.largest_allocation = self.stats.largest_allocation.max(block);
        let mut buffer = vec![0_u8; block];
        let mut completed = 0_usize;
        while completed < length {
            let take = (length - completed).min(buffer.len());
            let block_offset = offset
                .checked_add(completed)
                .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(label)))?;
            self.read_into(block_offset, &mut buffer[..take], label)
                .await?;
            hasher.update(&buffer[..take]);
            self.stats.bytes_hashed = self
                .stats
                .bytes_hashed
                .checked_add(
                    u64::try_from(take)
                        .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
                )
                .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
            completed += take;
        }
        Ok(())
    }
}

async fn read_full_envelope_async<S: AsyncStrongVersionReadAt>(
    reader: &mut AsyncFullSourceReader<'_, S>,
) -> Result<LookupEnvelope, AsyncImmutableSourceError> {
    if reader.length < FILE_HEADER_LEN + OBJECT_HEADER_LEN + PAGE_SIZE + SNAPSHOT_LEN + FOOTER_LEN {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid("file length")).into());
    }
    let header = reader.read_vec(0, FILE_HEADER_LEN, "header").await?;
    if &header[..8] != FILE_MAGIC || header[8..].iter().any(|byte| *byte != 0) {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid("header")).into());
    }

    let footer_offset = reader.length - FOOTER_LEN;
    let footer_raw = reader.read_vec(footer_offset, FOOTER_LEN, "footer").await?;
    let footer = parse_footer(&footer_raw, 0)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot_len = usize_from_u64(footer.snapshot_len, "snapshot range")?;
    if snapshot_len != SNAPSHOT_LEN
        || snapshot_offset
            .checked_add(snapshot_len)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid("snapshot range")))?
            != footer_offset
    {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("snapshot range")).into(),
        );
    }
    let snapshot = reader
        .read_vec(snapshot_offset, snapshot_len, "snapshot")
        .await?;
    let snapshot_digest = digest(&[SNAPSHOT_DOMAIN, &snapshot]);
    reader.stats.bytes_hashed = reader
        .stats
        .bytes_hashed
        .checked_add(
            u64::try_from(snapshot.len())
                .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
        )
        .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
    if snapshot_digest != footer.snapshot_digest
        || &snapshot[..8] != SNAPSHOT_MAGIC
        || u64_at(&snapshot, 8, "snapshot")? != footer.sequence
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid("snapshot")).into());
    }
    let parent_snapshot_digest = array::<32>(&snapshot, 64, "snapshot parent")?;
    let commit_start = if footer.previous_footer_offset == ABSENT_OFFSET {
        if footer.sequence != 0 || parent_snapshot_digest.iter().any(|byte| *byte != 0) {
            return Err(
                ImmutableSourceError::Format(ImmutableError::Invalid("genesis linkage")).into(),
            );
        }
        0
    } else {
        let previous_offset = usize_from_u64(footer.previous_footer_offset, "previous footer")?;
        let previous_end = previous_offset
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid("previous footer")))?;
        if previous_end > snapshot_offset {
            return Err(
                ImmutableSourceError::Format(ImmutableError::Invalid("previous footer")).into(),
            );
        }
        let previous_raw = reader
            .read_vec(previous_offset, FOOTER_LEN, "previous footer")
            .await?;
        let previous = parse_footer(&previous_raw, 0)?;
        if footer.sequence != previous.sequence + 1
            || previous.snapshot_digest != parent_snapshot_digest
        {
            return Err(
                ImmutableSourceError::Format(ImmutableError::Invalid("parent linkage")).into(),
            );
        }
        previous_end
    };

    let mut commit_hasher = Sha256::new();
    commit_hasher.update(COMMIT_DOMAIN);
    reader
        .hash_range(
            &mut commit_hasher,
            commit_start,
            footer_offset - commit_start,
            "commit",
        )
        .await?;
    commit_hasher.update(footer_semantics(&footer));
    let commit_digest: [u8; 32] = commit_hasher.finalize().into();
    if commit_digest != footer.commit_digest {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("commit digest")).into(),
        );
    }

    let root_level_u64 = u64_at(&snapshot, 24, "snapshot root")?;
    let root_level = u8::try_from(root_level_u64).map_err(|_| {
        ImmutableSourceError::Format(ImmutableError::Invalid("snapshot root"))
    })?;
    if root_level > reader.limits.format.max_depth {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Limit("page depth")).into(),
        );
    }
    Ok(LookupEnvelope {
        sequence: footer.sequence,
        snapshot_digest: footer.snapshot_digest,
        commit_digest: footer.commit_digest,
        snapshot_offset,
        footer_offset,
        root: LookupReference {
            offset: usize_at(&snapshot, 16, "snapshot root")?,
            level: root_level,
            digest: array(&snapshot, 32, "snapshot root")?,
            range: None,
        },
    })
}

async fn read_full_page_async<S: AsyncStrongVersionReadAt>(
    reader: &mut AsyncFullSourceReader<'_, S>,
    reference: &LookupReference,
    envelope: &LookupEnvelope,
    visited: &mut HashSet<usize>,
    stack: &mut Vec<LookupReference>,
    locators: &mut Vec<Locator>,
    known_ranges: &mut Vec<(usize, usize)>,
) -> Result<(), AsyncImmutableSourceError> {
    if visited.len() >= reader.limits.format.max_pages {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Limit("page count")).into(),
        );
    }
    if !visited.insert(reference.offset) {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("page cycle")).into(),
        );
    }
    if !known_ranges
        .iter()
        .any(|range| *range == (reference.offset, reference.offset + PAGE_SIZE))
    {
        register_page_range(known_ranges, reference.offset, envelope.snapshot_offset)?;
    }
    let page = reader.read_vec(reference.offset, PAGE_SIZE, "page").await?;
    reader.stats.bytes_hashed = reader
        .stats
        .bytes_hashed
        .checked_add(
            u64::try_from(page.len())
                .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
        )
        .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
    if digest(&[PAGE_DOMAIN, &page]) != reference.digest || &page[..8] != PAGE_MAGIC {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("page digest")).into(),
        );
    }

    let kind = page[8];
    let level = page[9];
    let reserved = u16_at(&page, 10, "page header")?;
    let count = usize::try_from(u32_at(&page, 12, "page header")?)
        .map_err(|_| ImmutableSourceError::Format(ImmutableError::Invalid("page count")))?;
    let entry_size = usize::try_from(u32_at(&page, 16, "page header")?)
        .map_err(|_| ImmutableSourceError::Format(ImmutableError::Invalid("page entry size")))?;
    let minimum = u64_at(&page, 20, "page header")?;
    let maximum = u64_at(&page, 28, "page header")?;
    if reserved != 0 || page[36..PAGE_HEADER_LEN].iter().any(|byte| *byte != 0) || count == 0 {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("page header")).into(),
        );
    }
    if level != reference.level
        || reference
            .range
            .is_some_and(|range| range != (minimum, maximum))
    {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("page reference")).into(),
        );
    }

    match kind {
        1 => {
            if level != 0 || entry_size != LEAF_ENTRY_LEN || count > LEAF_CAPACITY {
                return Err(
                    ImmutableSourceError::Format(ImmutableError::Invalid("leaf shape")).into(),
                );
            }
            if locators
                .len()
                .checked_add(count)
                .is_none_or(|value| value > reader.limits.format.max_objects)
            {
                return Err(
                    ImmutableSourceError::Format(ImmutableError::Limit("object count")).into(),
                );
            }
            allocation_check::<Locator>(locators.len() + count, reader.limits.format)?;
            let mut previous = None;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                let object_id = u64_at(&page, entry, "leaf entry")?;
                let object_kind = u16_at(&page, entry + 8, "leaf entry")?;
                if object_id == 0
                    || object_kind == 0
                    || page[entry + 10..entry + 16].iter().any(|byte| *byte != 0)
                    || page[entry + 72..entry + 88].iter().any(|byte| *byte != 0)
                    || previous.is_some_and(|value| value >= object_id)
                {
                    return Err(
                        ImmutableSourceError::Format(ImmutableError::Invalid("leaf entry")).into(),
                    );
                }
                previous = Some(object_id);
                locators.push(Locator {
                    object_id,
                    kind: object_kind,
                    record_offset: u64_at(&page, entry + 16, "leaf entry")?,
                    record_len: u64_at(&page, entry + 24, "leaf entry")?,
                    logical_len: u64_at(&page, entry + 32, "leaf entry")?,
                    digest: array(&page, entry + 40, "leaf entry")?,
                });
            }
            if u64_at(&page, PAGE_HEADER_LEN, "leaf order")? != minimum
                || previous != Some(maximum)
                || page[PAGE_HEADER_LEN + count * LEAF_ENTRY_LEN..]
                    .iter()
                    .any(|byte| *byte != 0)
            {
                return Err(
                    ImmutableSourceError::Format(ImmutableError::Invalid("leaf order")).into(),
                );
            }
        }
        2 => {
            if level == 0 || entry_size != INTERNAL_ENTRY_LEN || count > INTERNAL_FANOUT {
                return Err(
                    ImmutableSourceError::Format(ImmutableError::Invalid("internal shape")).into(),
                );
            }
            if stack
                .len()
                .checked_add(count)
                .is_none_or(|value| value > reader.limits.format.max_pages)
            {
                return Err(
                    ImmutableSourceError::Format(ImmutableError::Limit("page count")).into(),
                );
            }
            allocation_check::<LookupReference>(stack.len() + count, reader.limits.format)?;
            let mut previous_maximum = None;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                let child_minimum = u64_at(&page, entry, "child entry")?;
                let child_maximum = u64_at(&page, entry + 8, "child entry")?;
                let child_offset = usize_at(&page, entry + 16, "child entry")?;
                let child_len = usize_at(&page, entry + 24, "child entry")?;
                if child_minimum > child_maximum
                    || child_len != PAGE_SIZE
                    || previous_maximum.is_some_and(|value| value >= child_minimum)
                {
                    return Err(
                        ImmutableSourceError::Format(ImmutableError::Invalid("child entry")).into(),
                    );
                }
                previous_maximum = Some(child_maximum);
                register_page_range(known_ranges, child_offset, envelope.snapshot_offset)?;
                stack.push(LookupReference {
                    offset: child_offset,
                    level: level - 1,
                    digest: array(&page, entry + 32, "child entry")?,
                    range: Some((child_minimum, child_maximum)),
                });
            }
            if u64_at(&page, PAGE_HEADER_LEN, "child order")? != minimum
                || previous_maximum != Some(maximum)
                || page[PAGE_HEADER_LEN + count * INTERNAL_ENTRY_LEN..]
                    .iter()
                    .any(|byte| *byte != 0)
            {
                return Err(
                    ImmutableSourceError::Format(ImmutableError::Invalid("child order")).into(),
                );
            }
        }
        _ => {
            return Err(
                ImmutableSourceError::Format(ImmutableError::Invalid("page kind")).into(),
            );
        }
    }
    Ok(())
}

async fn validate_full_object_async<S: AsyncStrongVersionReadAt>(
    reader: &mut AsyncFullSourceReader<'_, S>,
    locator: &Locator,
    envelope: &LookupEnvelope,
    known_ranges: &[(usize, usize)],
) -> Result<(), AsyncImmutableSourceError> {
    let offset = usize_from_u64(locator.record_offset, "object range")?;
    let length = usize_from_u64(locator.record_len, "object range")?;
    let end = offset
        .checked_add(length)
        .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid("object range")))?;
    if offset < FILE_HEADER_LEN
        || end > envelope.snapshot_offset
        || known_ranges
            .iter()
            .any(|(start, stop)| offset < *stop && *start < end)
    {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("object structural overlap"))
                .into(),
        );
    }
    let header = reader
        .read_vec(offset, OBJECT_HEADER_LEN, "object header")
        .await?;
    if &header[..8] != OBJECT_MAGIC
        || usize::from(u16_at(&header, 8, "object header")?) != OBJECT_HEADER_LEN
        || u32_at(&header, 12, "object header")? != 0
        || header[40..].iter().any(|byte| *byte != 0)
    {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("object header")).into(),
        );
    }
    let kind = u16_at(&header, 10, "object header")?;
    let object_id = u64_at(&header, 16, "object header")?;
    let payload_len = usize_at(&header, 24, "object length")?;
    let logical_len = u64_at(&header, 32, "object length")?;
    if kind == 0
        || object_id == 0
        || OBJECT_HEADER_LEN
            .checked_add(payload_len)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid("object length")))?
            != length
        || u64_from_usize(payload_len)? != logical_len
        || object_id != locator.object_id
        || kind != locator.kind
        || logical_len != locator.logical_len
    {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("object locator")).into(),
        );
    }
    let mut object_hasher = Sha256::new();
    object_hasher.update(OBJECT_DOMAIN);
    object_hasher.update(&header);
    reader.stats.bytes_hashed = reader
        .stats
        .bytes_hashed
        .checked_add(
            u64::try_from(header.len())
                .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
        )
        .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
    reader
        .hash_range(
            &mut object_hasher,
            offset + OBJECT_HEADER_LEN,
            payload_len,
            "object payload",
        )
        .await?;
    let object_digest: [u8; 32] = object_hasher.finalize().into();
    if object_digest != locator.digest {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("object digest")).into(),
        );
    }
    Ok(())
}

/// Strictly validates the exact-end active snapshot over one native async strong-version source.
pub async fn validate_source_at_async<S: AsyncStrongVersionReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<AsyncImmutableSourceStrictReport, AsyncImmutableSourceError> {
    let mut reader = AsyncFullSourceReader::new(source, limits).await?;
    let source_version = reader.version.clone();
    let source_length = reader.length_u64;
    let envelope = read_full_envelope_async(&mut reader).await?;
    let footer_raw = reader
        .read_vec(envelope.footer_offset, FOOTER_LEN, "footer")
        .await?;
    let footer = parse_footer(&footer_raw, 0)?;
    let commit_start = if footer.previous_footer_offset == ABSENT_OFFSET {
        0
    } else {
        usize_from_u64(footer.previous_footer_offset, "previous footer")?
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "previous footer",
            )))?
    };

    let mut visited = HashSet::new();
    let mut stack = vec![envelope.root.clone()];
    let mut locators = Vec::new();
    let mut known_ranges = vec![
        (envelope.snapshot_offset, envelope.footer_offset),
        (envelope.footer_offset, reader.length),
    ];
    while let Some(reference) = stack.pop() {
        read_full_page_async(
            &mut reader,
            &reference,
            &envelope,
            &mut visited,
            &mut stack,
            &mut locators,
            &mut known_ranges,
        )
        .await?;
    }

    let current_pages = visited
        .iter()
        .filter(|offset| **offset >= commit_start)
        .count();
    if footer.page_count_current != u64_from_usize(current_pages)? {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("page count")).into(),
        );
    }
    locators.sort_by_key(|locator| locator.object_id);
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("object order")).into(),
        );
    }

    allocation_check::<(usize, usize)>(locators.len(), reader.limits.format)?;
    let mut object_ranges = Vec::with_capacity(locators.len());
    for locator in &locators {
        let offset = usize_from_u64(locator.record_offset, "object range")?;
        let length = usize_from_u64(locator.record_len, "object range")?;
        let end = offset
            .checked_add(length)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "object range",
            )))?;
        object_ranges.push((offset, end));
    }
    object_ranges.sort_unstable();
    if object_ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("object overlap")).into(),
        );
    }
    for locator in &locators {
        validate_full_object_async(&mut reader, locator, &envelope, &known_ranges).await?;
    }

    Ok(AsyncImmutableSourceStrictReport {
        source_version,
        source_length,
        strict: ImmutableSourceStrictReport {
            report: ImmutableReport {
                sequence: envelope.sequence,
                object_count: locators.len(),
                page_count: visited.len(),
                root_level: envelope.root.level,
                snapshot_digest: envelope.snapshot_digest,
                commit_digest: envelope.commit_digest,
            },
            stats: reader.stats,
        },
    })
}

#[cfg(test)]
mod conditional_async_source_full_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    struct NoAuthentication;

    impl AsyncConditionalAuthenticationRefresher for NoAuthentication {
        fn current_authorization(
            &self,
        ) -> Result<Option<ReqwestAuthorizationHeader>, ConditionalSourceError> {
            Ok(None)
        }

        fn refresh_authentication<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), ConditionalSourceError>> + Send + 'a>> {
            Box::pin(async {
                Err(ConditionalSourceError::Client(
                    "unexpected authentication refresh",
                ))
            })
        }
    }

    #[derive(Default)]
    struct Counts {
        head: usize,
        ranges: usize,
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 2048];
        loop {
            let read = socket.read(&mut chunk).await.expect("read request");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(bytes).expect("request text")
    }

    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then_some(value.trim())
        })
    }

    fn parse_range(request: &str) -> Option<(usize, usize)> {
        let value = header_value(request, "range")?;
        let value = value.strip_prefix("bytes=")?;
        let (start, end) = value.split_once('-')?;
        Some((start.parse().ok()?, end.parse().ok()?))
    }

    async fn serve_object(
        bytes: Vec<u8>,
        mutate_after_ranges: Option<usize>,
    ) -> (
        String,
        Arc<Mutex<Counts>>,
        oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let counts = Arc::new(Mutex::new(Counts::default()));
        let observed = Arc::clone(&counts);
        let shared = Arc::new(bytes);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    _ = &mut shutdown_rx => break,
                    result = listener.accept() => result,
                };
                let (mut socket, _) = accepted.expect("accept");
                let request = read_request(&mut socket).await;
                let first = request.lines().next().unwrap_or_default();
                if first.starts_with("HEAD ") {
                    observed.lock().expect("counts").head += 1;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        shared.len()
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("head response");
                } else if first.starts_with("GET ") {
                    let mut counts = observed.lock().expect("counts");
                    counts.ranges += 1;
                    let range_number = counts.ranges;
                    drop(counts);
                    if mutate_after_ranges.is_some_and(|limit| range_number > limit) {
                        socket
                            .write_all(
                                b"HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            )
                            .await
                            .expect("412 response");
                        continue;
                    }
                    assert_eq!(header_value(&request, "if-match"), Some("\"v1\""));
                    let (start, end) = parse_range(&request).expect("range");
                    let body = &shared[start..=end];
                    let response = format!(
                        "HTTP/1.1 206 Partial Content\r\nETag: \"v1\"\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                        body.len(), start, end, shared.len()
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("range headers");
                    socket.write_all(body).await.expect("range body");
                }
                socket.shutdown().await.expect("shutdown");
            }
        });
        (
            format!("http://{address}/object"),
            counts,
            shutdown_tx,
            server,
        )
    }

    fn limits() -> ImmutableSourceLimits {
        ImmutableSourceLimits {
            max_read_request_bytes: 4 * 1024,
            hash_block_bytes: 4 * 1024,
            ..ImmutableSourceLimits::default()
        }
    }

    fn http_client(
        url: &str,
    ) -> AsyncAuthenticatedReqwestConditionalClient<NoAuthentication> {
        let transport = ReqwestConditionalRangeClient::new(
            url,
            ImmutableOperationControl::unlimited(),
        )
        .expect("transport");
        let retrying = AsyncRetryingReqwestConditionalClient::new(
            transport,
            ConditionalRetryPolicy::new(512).expect("retry policy"),
            ConditionalBackoffPolicy::new(1, 10, 100).expect("backoff policy"),
        );
        AsyncAuthenticatedReqwestConditionalClient::new(retrying, NoAuthentication)
    }

    fn multi_page_genesis() -> Vec<u8> {
        let objects: Vec<_> = (1..=LEAF_CAPACITY + 7)
            .map(|index| {
                ImmutableObjectInput::new(
                    u64::try_from(index).expect("id"),
                    1,
                    vec![u8::try_from(index % 251).expect("payload byte"); 3],
                )
            })
            .collect();
        build_genesis(&objects, ImmutableLimits::default()).expect("multi-page genesis")
    }

    #[tokio::test]
    async fn real_http_full_validation_matches_synchronous_report_and_stats() {
        let bytes = multi_page_genesis();
        let source_limits = limits();
        let mut sync_source = ImmutableSliceSource::new(&bytes);
        let sync = validate_source_at(&mut sync_source, source_limits).expect("sync full validation");
        assert!(sync.report.page_count > 1);

        let (url, counts, shutdown, server) = serve_object(bytes.clone(), None).await;
        let mut client = http_client(&url);
        let async_report = validate_source_at_async(&mut client, source_limits)
            .await
            .expect("async full validation");
        assert_eq!(async_report.strict, sync);
        assert_eq!(async_report.source_length, bytes.len() as u64);
        assert_eq!(async_report.source_version.as_str(), "\"v1\"");
        let observed = counts.lock().expect("counts");
        assert_eq!(observed.head, 1);
        assert_eq!(observed.ranges as u64, async_report.strict.stats.read_operations);
        drop(observed);
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn version_change_during_full_tree_walk_terminates_validation() {
        let bytes = multi_page_genesis();
        let (url, counts, shutdown, server) = serve_object(bytes, Some(8)).await;
        let mut client = http_client(&url);
        assert_eq!(
            validate_source_at_async(&mut client, limits()).await,
            Err(AsyncImmutableSourceError::Conditional(
                ConditionalSourceError::VersionChanged
            ))
        );
        let observed = counts.lock().expect("counts");
        assert_eq!(observed.head, 1);
        assert_eq!(observed.ranges, 9);
        drop(observed);
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn full_validation_preserves_page_limit_failure_class() {
        let bytes = multi_page_genesis();
        let constrained = ImmutableSourceLimits {
            format: ImmutableLimits {
                max_pages: 1,
                ..ImmutableLimits::default()
            },
            ..limits()
        };
        let (url, _, shutdown, server) = serve_object(bytes, None).await;
        let mut client = http_client(&url);
        assert_eq!(
            validate_source_at_async(&mut client, constrained).await,
            Err(AsyncImmutableSourceError::Source(
                ImmutableSourceError::Format(ImmutableError::Limit("page count"))
            ))
        );
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn full_validation_detects_payload_corruption_after_rehashed_outer_layers_are_not_changed() {
        let mut bytes = build_genesis(
            &[ImmutableObjectInput::new(1, 1, b"payload".to_vec())],
            ImmutableLimits::default(),
        )
        .expect("genesis");
        let payload_offset = FILE_HEADER_LEN + OBJECT_HEADER_LEN;
        bytes[payload_offset] ^= 0x01;
        let (url, _, shutdown, server) = serve_object(bytes, None).await;
        let mut client = http_client(&url);
        assert_eq!(
            validate_source_at_async(&mut client, limits()).await,
            Err(AsyncImmutableSourceError::Source(
                ImmutableSourceError::Format(ImmutableError::Invalid("commit digest"))
            ))
        );
        let _ = shutdown.send(());
        server.await.expect("server");
    }
}
