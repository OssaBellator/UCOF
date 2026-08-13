use std::future::Future;
use std::pin::Pin;

/// Native asynchronous random-access contract for one strong-version source operation.
///
/// Implementations acquire exact length + one strong version once, then every accepted range must
/// be conditioned on that exact version. This trait intentionally does not hide an async runtime
/// behind the synchronous `ImmutableReadAt` interface.
pub trait AsyncStrongVersionReadAt {
    fn metadata_async<'a>(
        &'a mut self,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ConditionalObjectMetadata, ConditionalSourceError>>
                + Send
                + 'a,
        >,
    >;

    fn read_range_if_match_async<'a>(
        &'a mut self,
        expected: &'a StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ConditionalRangeResponse, ConditionalSourceError>>
                + Send
                + 'a,
        >,
    >;
}

impl AsyncStrongVersionReadAt for AsyncRetryingReqwestConditionalClient {
    fn metadata_async<'a>(
        &'a mut self,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ConditionalObjectMetadata, ConditionalSourceError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { AsyncRetryingReqwestConditionalClient::metadata(self).await })
    }

    fn read_range_if_match_async<'a>(
        &'a mut self,
        expected: &'a StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ConditionalRangeResponse, ConditionalSourceError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            AsyncRetryingReqwestConditionalClient::read_range_if_match(
                self,
                expected,
                offset,
                length,
                total_length,
            )
            .await
        })
    }
}

impl<R> AsyncStrongVersionReadAt for AsyncAuthenticatedReqwestConditionalClient<R>
where
    R: AsyncConditionalAuthenticationRefresher + Send,
{
    fn metadata_async<'a>(
        &'a mut self,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ConditionalObjectMetadata, ConditionalSourceError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { AsyncAuthenticatedReqwestConditionalClient::metadata(self).await })
    }

    fn read_range_if_match_async<'a>(
        &'a mut self,
        expected: &'a StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ConditionalRangeResponse, ConditionalSourceError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            AsyncAuthenticatedReqwestConditionalClient::read_range_if_match(
                self,
                expected,
                offset,
                length,
                total_length,
            )
            .await
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AsyncImmutableSourceError {
    Source(ImmutableSourceError),
    Conditional(ConditionalSourceError),
}

impl std::fmt::Display for AsyncImmutableSourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "{error}"),
            Self::Conditional(error) => write!(formatter, "conditional source failed: {error}"),
        }
    }
}

impl std::error::Error for AsyncImmutableSourceError {}

impl From<ImmutableError> for AsyncImmutableSourceError {
    fn from(error: ImmutableError) -> Self {
        Self::Source(ImmutableSourceError::Format(error))
    }
}

impl From<ImmutableSourceError> for AsyncImmutableSourceError {
    fn from(error: ImmutableSourceError) -> Self {
        Self::Source(error)
    }
}

impl From<ConditionalSourceError> for AsyncImmutableSourceError {
    fn from(error: ConditionalSourceError) -> Self {
        Self::Conditional(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncImmutableSourceLookupReport {
    pub source_version: StrongVersionToken,
    pub source_length: u64,
    pub lookup: ImmutableSourceLookupReport,
}

struct AsyncSourceReader<'a, S> {
    source: &'a mut S,
    version: StrongVersionToken,
    length: usize,
    length_u64: u64,
    limits: ImmutableSourceLimits,
    stats: ImmutableSourceStats,
}

impl<'a, S: AsyncStrongVersionReadAt> AsyncSourceReader<'a, S> {
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

async fn read_lookup_envelope_async<S: AsyncStrongVersionReadAt>(
    reader: &mut AsyncSourceReader<'_, S>,
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

async fn read_lookup_page_async<S: AsyncStrongVersionReadAt>(
    reader: &mut AsyncSourceReader<'_, S>,
    reference: &LookupReference,
    object_id: u64,
    envelope: &LookupEnvelope,
    visited: &mut HashSet<usize>,
    known_ranges: &mut Vec<(usize, usize)>,
) -> Result<PageLookup, AsyncImmutableSourceError> {
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
    let page_digest = digest(&[PAGE_DOMAIN, &page]);
    reader.stats.bytes_hashed = reader
        .stats
        .bytes_hashed
        .checked_add(
            u64::try_from(page.len())
                .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
        )
        .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
    if page_digest != reference.digest || &page[..8] != PAGE_MAGIC {
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
    if reserved != 0 || page[36..64].iter().any(|byte| *byte != 0) || count == 0 {
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
            let mut previous = None;
            let mut selected = None;
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                let entry_id = u64_at(&page, entry, "leaf entry")?;
                let entry_kind = u16_at(&page, entry + 8, "leaf entry")?;
                if entry_id == 0
                    || entry_kind == 0
                    || page[entry + 10..entry + 16].iter().any(|byte| *byte != 0)
                    || page[entry + 72..entry + 88].iter().any(|byte| *byte != 0)
                    || previous.is_some_and(|value| value >= entry_id)
                {
                    return Err(
                        ImmutableSourceError::Format(ImmutableError::Invalid("leaf entry")).into(),
                    );
                }
                previous = Some(entry_id);
                if entry_id == object_id {
                    selected = Some(Locator {
                        object_id: entry_id,
                        kind: entry_kind,
                        record_offset: u64_at(&page, entry + 16, "leaf entry")?,
                        record_len: u64_at(&page, entry + 24, "leaf entry")?,
                        logical_len: u64_at(&page, entry + 32, "leaf entry")?,
                        digest: array(&page, entry + 40, "leaf entry")?,
                    });
                }
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
            Ok(selected.map_or(PageLookup::Absent, PageLookup::Found))
        }
        2 => {
            if level == 0 || entry_size != INTERNAL_ENTRY_LEN || count > INTERNAL_FANOUT {
                return Err(
                    ImmutableSourceError::Format(ImmutableError::Invalid("internal shape")).into(),
                );
            }
            let mut previous_maximum = None;
            let mut selected = None;
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
                if child_minimum <= object_id && object_id <= child_maximum {
                    selected = Some(LookupReference {
                        offset: child_offset,
                        level: level - 1,
                        digest: array(&page, entry + 32, "child entry")?,
                        range: Some((child_minimum, child_maximum)),
                    });
                }
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
            Ok(selected.map_or(PageLookup::Absent, PageLookup::Next))
        }
        _ => Err(ImmutableSourceError::Format(ImmutableError::Invalid("page kind")).into()),
    }
}

async fn validate_lookup_object_async<S: AsyncStrongVersionReadAt>(
    reader: &mut AsyncSourceReader<'_, S>,
    locator: &Locator,
    envelope: &LookupEnvelope,
    known_ranges: &[(usize, usize)],
) -> Result<ImmutableLookupResult, AsyncImmutableSourceError> {
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
    Ok(ImmutableLookupResult::Found {
        object_id,
        kind,
        logical_len,
        record_offset: locator.record_offset,
        object_digest,
    })
}

/// Async counterpart of `lookup_at` using one strong source version for the complete assurance
/// operation and native conditional range I/O throughout.
pub async fn lookup_at_async<S: AsyncStrongVersionReadAt>(
    source: &mut S,
    object_id: u64,
    limits: ImmutableSourceLimits,
) -> Result<AsyncImmutableSourceLookupReport, AsyncImmutableSourceError> {
    if object_id == 0 {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("object identifier")).into(),
        );
    }
    let mut reader = AsyncSourceReader::new(source, limits).await?;
    let source_version = reader.version.clone();
    let source_length = reader.length_u64;
    let envelope = read_lookup_envelope_async(&mut reader).await?;
    let mut visited = HashSet::new();
    let mut known_ranges = vec![
        (envelope.snapshot_offset, envelope.footer_offset),
        (envelope.footer_offset, reader.length),
    ];
    let mut reference = envelope.root.clone();
    let result = loop {
        match read_lookup_page_async(
            &mut reader,
            &reference,
            object_id,
            &envelope,
            &mut visited,
            &mut known_ranges,
        )
        .await?
        {
            PageLookup::Next(next) => reference = next,
            PageLookup::Found(locator) => {
                break validate_lookup_object_async(
                    &mut reader,
                    &locator,
                    &envelope,
                    &known_ranges,
                )
                .await?;
            }
            PageLookup::Absent => break ImmutableLookupResult::Absent { object_id },
        }
    };
    Ok(AsyncImmutableSourceLookupReport {
        source_version,
        source_length,
        lookup: ImmutableSourceLookupReport {
            sequence: envelope.sequence,
            snapshot_digest: envelope.snapshot_digest,
            commit_digest: envelope.commit_digest,
            result,
            stats: reader.stats,
        },
    })
}

#[cfg(test)]
mod conditional_async_source_lookup_tests {
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
    struct ServerCounts {
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
        fail_ranges_with_412: bool,
    ) -> (
        String,
        Arc<Mutex<ServerCounts>>,
        oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let counts = Arc::new(Mutex::new(ServerCounts::default()));
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
                    observed.lock().expect("counts").ranges += 1;
                    if fail_ranges_with_412 {
                        socket
                            .write_all(
                                b"HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            )
                            .await
                            .expect("412 response");
                        continue;
                    }
                    assert_eq!(header_value(&request, "if-match"), Some("\"v1\""));
                    assert_eq!(header_value(&request, "accept-encoding"), Some("identity"));
                    let (start, end) = parse_range(&request).expect("range");
                    assert!(start <= end && end < shared.len());
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
                } else {
                    panic!("unexpected request: {first}");
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
            ConditionalRetryPolicy::new(128).expect("retry policy"),
            ConditionalBackoffPolicy::new(1, 10, 100).expect("backoff policy"),
        );
        AsyncAuthenticatedReqwestConditionalClient::new(retrying, NoAuthentication)
    }

    #[tokio::test]
    async fn real_http_targeted_lookup_matches_synchronous_found_report() {
        let bytes = build_genesis(
            &[
                ImmutableObjectInput::new(1, 7, b"alpha".to_vec()),
                ImmutableObjectInput::new(9, 8, b"target payload".to_vec()),
                ImmutableObjectInput::new(20, 9, b"omega".to_vec()),
            ],
            ImmutableLimits::default(),
        )
        .expect("genesis");
        let source_limits = limits();
        let mut sync_source = ImmutableSliceSource::new(&bytes);
        let sync = lookup_at(&mut sync_source, 9, source_limits).expect("sync lookup");

        let (url, counts, shutdown, server) = serve_object(bytes.clone(), false).await;
        let mut client = http_client(&url);
        let async_report = lookup_at_async(&mut client, 9, source_limits)
            .await
            .expect("async lookup");
        assert_eq!(async_report.lookup, sync);
        assert_eq!(async_report.source_length, bytes.len() as u64);
        assert_eq!(async_report.source_version.as_str(), "\"v1\"");
        let observed = counts.lock().expect("counts");
        assert_eq!(observed.head, 1);
        assert_eq!(observed.ranges as u64, async_report.lookup.stats.read_operations);
        drop(observed);
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn real_http_targeted_absence_matches_synchronous_report() {
        let bytes = build_genesis(
            &[
                ImmutableObjectInput::new(2, 1, b"a".to_vec()),
                ImmutableObjectInput::new(8, 1, b"b".to_vec()),
                ImmutableObjectInput::new(30, 1, b"c".to_vec()),
            ],
            ImmutableLimits::default(),
        )
        .expect("genesis");
        let source_limits = limits();
        let mut sync_source = ImmutableSliceSource::new(&bytes);
        let sync = lookup_at(&mut sync_source, 9, source_limits).expect("sync absence");
        assert_eq!(sync.result, ImmutableLookupResult::Absent { object_id: 9 });

        let (url, _, shutdown, server) = serve_object(bytes, false).await;
        let mut client = http_client(&url);
        let async_report = lookup_at_async(&mut client, 9, source_limits)
            .await
            .expect("async absence");
        assert_eq!(async_report.lookup, sync);
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn version_change_terminates_lookup_before_accepting_range_bytes() {
        let bytes = build_genesis(
            &[ImmutableObjectInput::new(1, 1, b"payload".to_vec())],
            ImmutableLimits::default(),
        )
        .expect("genesis");
        let (url, counts, shutdown, server) = serve_object(bytes, true).await;
        let mut client = http_client(&url);
        assert_eq!(
            lookup_at_async(&mut client, 1, limits()).await,
            Err(AsyncImmutableSourceError::Conditional(
                ConditionalSourceError::VersionChanged
            ))
        );
        let observed = counts.lock().expect("counts");
        assert_eq!(observed.head, 1);
        assert_eq!(observed.ranges, 1);
        drop(observed);
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn async_lookup_preserves_source_read_budget_failure() {
        let bytes = build_genesis(
            &[ImmutableObjectInput::new(1, 1, b"payload".to_vec())],
            ImmutableLimits::default(),
        )
        .expect("genesis");
        let constrained = ImmutableSourceLimits {
            max_read_operations: 1,
            max_read_request_bytes: 64,
            hash_block_bytes: 64,
            ..ImmutableSourceLimits::default()
        };
        let (url, counts, shutdown, server) = serve_object(bytes, false).await;
        let mut client = http_client(&url);
        assert_eq!(
            lookup_at_async(&mut client, 1, constrained).await,
            Err(AsyncImmutableSourceError::Source(
                ImmutableSourceError::Limit("read operations")
            ))
        );
        let observed = counts.lock().expect("counts");
        assert_eq!(observed.head, 1);
        assert_eq!(observed.ranges, 1);
        drop(observed);
        let _ = shutdown.send(());
        server.await.expect("server");
    }
}
