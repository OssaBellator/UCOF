use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableSourceError {
    Format(ImmutableError),
    Io(&'static str),
    Limit(&'static str),
}

impl fmt::Display for ImmutableSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "{error}"),
            Self::Io(label) => write!(formatter, "source I/O failed: {label}"),
            Self::Limit(label) => write!(formatter, "source {label} limit exceeded"),
        }
    }
}

impl Error for ImmutableSourceError {}

impl From<ImmutableError> for ImmutableSourceError {
    fn from(error: ImmutableError) -> Self {
        Self::Format(error)
    }
}

pub trait ImmutableReadAt {
    fn len(&mut self) -> Result<u64, ImmutableSourceError>;
    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError>;
}

#[derive(Debug)]
pub struct ImmutableSliceSource<'a> {
    data: &'a [u8],
}

impl<'a> ImmutableSliceSource<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }
}

impl ImmutableReadAt for ImmutableSliceSource<'_> {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        u64::try_from(self.data.len()).map_err(|_| ImmutableSourceError::Limit("length"))
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
        let end = start
            .checked_add(buffer.len())
            .ok_or(ImmutableSourceError::Io("range"))?;
        let source = self
            .data
            .get(start..end)
            .ok_or(ImmutableSourceError::Io("range"))?;
        buffer.copy_from_slice(source);
        Ok(())
    }
}

#[derive(Debug)]
pub struct ImmutableSeekSource<R> {
    inner: R,
}

impl<R> ImmutableSeekSource<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read + Seek> ImmutableReadAt for ImmutableSeekSource<R> {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        let current = self
            .inner
            .stream_position()
            .map_err(|_| ImmutableSourceError::Io("position"))?;
        let length = self
            .inner
            .seek(SeekFrom::End(0))
            .map_err(|_| ImmutableSourceError::Io("length"))?;
        self.inner
            .seek(SeekFrom::Start(current))
            .map_err(|_| ImmutableSourceError::Io("restore position"))?;
        Ok(length)
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        self.inner
            .seek(SeekFrom::Start(offset))
            .map_err(|_| ImmutableSourceError::Io("seek"))?;
        self.inner
            .read_exact(buffer)
            .map_err(|_| ImmutableSourceError::Io("read"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableSourceLimits {
    pub format: ImmutableLimits,
    pub max_total_bytes_read: u64,
    pub max_read_operations: u64,
    pub max_read_request_bytes: usize,
    pub hash_block_bytes: usize,
}

impl Default for ImmutableSourceLimits {
    fn default() -> Self {
        Self {
            format: ImmutableLimits::default(),
            max_total_bytes_read: 1024 * 1024 * 1024,
            max_read_operations: 1_000_000,
            max_read_request_bytes: 64 * 1024,
            hash_block_bytes: 64 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImmutableSourceStats {
    pub read_operations: u64,
    pub bytes_read: u64,
    pub bytes_hashed: u64,
    pub largest_allocation: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableLookupResult {
    Found {
        object_id: u64,
        kind: u16,
        logical_len: u64,
        record_offset: u64,
        object_digest: [u8; 32],
    },
    Absent { object_id: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceLookupReport {
    pub sequence: u64,
    pub snapshot_digest: [u8; 32],
    pub commit_digest: [u8; 32],
    pub result: ImmutableLookupResult,
    pub stats: ImmutableSourceStats,
}

struct SourceReader<'a, S> {
    source: &'a mut S,
    length: usize,
    limits: ImmutableSourceLimits,
    stats: ImmutableSourceStats,
}

impl<'a, S: ImmutableReadAt> SourceReader<'a, S> {
    fn new(
        source: &'a mut S,
        limits: ImmutableSourceLimits,
    ) -> Result<Self, ImmutableSourceError> {
        if limits.max_read_request_bytes == 0 || limits.hash_block_bytes == 0 {
            return Err(ImmutableSourceError::Limit("configuration"));
        }
        let length_u64 = source.len()?;
        let length = usize::try_from(length_u64)
            .map_err(|_| ImmutableSourceError::Limit("length"))?;
        if length > limits.format.max_file_bytes {
            return Err(ImmutableSourceError::Format(ImmutableError::Limit(
                "file size",
            )));
        }
        Ok(Self {
            source,
            length,
            limits,
            stats: ImmutableSourceStats::default(),
        })
    }

    fn read_into(
        &mut self,
        offset: usize,
        buffer: &mut [u8],
        label: &'static str,
    ) -> Result<(), ImmutableSourceError> {
        let end = offset
            .checked_add(buffer.len())
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(label)))?;
        if end > self.length {
            return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                label,
            )));
        }
        let mut completed = 0_usize;
        while completed < buffer.len() {
            let take = (buffer.len() - completed).min(self.limits.max_read_request_bytes);
            if self.stats.read_operations >= self.limits.max_read_operations {
                return Err(ImmutableSourceError::Limit("read operations"));
            }
            let take_u64 = u64::try_from(take)
                .map_err(|_| ImmutableSourceError::Limit("read bytes"))?;
            let next_total = self
                .stats
                .bytes_read
                .checked_add(take_u64)
                .ok_or(ImmutableSourceError::Limit("read bytes"))?;
            if next_total > self.limits.max_total_bytes_read {
                return Err(ImmutableSourceError::Limit("read bytes"));
            }
            let source_offset = offset
                .checked_add(completed)
                .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(label)))?;
            self.source.read_exact_at(
                u64::try_from(source_offset)
                    .map_err(|_| ImmutableSourceError::Limit("offset"))?,
                &mut buffer[completed..completed + take],
            )?;
            self.stats.read_operations += 1;
            self.stats.bytes_read = next_total;
            completed += take;
        }
        Ok(())
    }

    fn read_vec(
        &mut self,
        offset: usize,
        length: usize,
        label: &'static str,
    ) -> Result<Vec<u8>, ImmutableSourceError> {
        if length > self.limits.format.max_allocation_bytes {
            return Err(ImmutableSourceError::Format(ImmutableError::Limit(
                "allocation",
            )));
        }
        self.stats.largest_allocation = self.stats.largest_allocation.max(length);
        let mut output = vec![0_u8; length];
        self.read_into(offset, &mut output, label)?;
        Ok(output)
    }

    fn hash_range(
        &mut self,
        hasher: &mut Sha256,
        offset: usize,
        length: usize,
        label: &'static str,
    ) -> Result<(), ImmutableSourceError> {
        let block = self
            .limits
            .hash_block_bytes
            .min(self.limits.max_read_request_bytes)
            .min(self.limits.format.max_allocation_bytes);
        if block == 0 {
            return Err(ImmutableSourceError::Limit("hash block"));
        }
        self.stats.largest_allocation = self.stats.largest_allocation.max(block);
        let mut buffer = vec![0_u8; block];
        let mut completed = 0_usize;
        while completed < length {
            let take = (length - completed).min(buffer.len());
            self.read_into(
                offset
                    .checked_add(completed)
                    .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(label)))?,
                &mut buffer[..take],
                label,
            )?;
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

#[derive(Clone, Debug)]
struct LookupReference {
    offset: usize,
    level: u8,
    digest: [u8; 32],
    range: Option<(u64, u64)>,
}

struct LookupEnvelope {
    sequence: u64,
    snapshot_digest: [u8; 32],
    commit_digest: [u8; 32],
    snapshot_offset: usize,
    footer_offset: usize,
    root: LookupReference,
}

fn read_lookup_envelope<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
) -> Result<LookupEnvelope, ImmutableSourceError> {
    if reader.length < FILE_HEADER_LEN + OBJECT_HEADER_LEN + PAGE_SIZE + SNAPSHOT_LEN + FOOTER_LEN {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "file length",
        )));
    }
    let header = reader.read_vec(0, FILE_HEADER_LEN, "header")?;
    if &header[..8] != FILE_MAGIC || header[8..].iter().any(|byte| *byte != 0) {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "header",
        )));
    }

    let footer_offset = reader.length - FOOTER_LEN;
    let footer_raw = reader.read_vec(footer_offset, FOOTER_LEN, "footer")?;
    let footer = parse_footer(&footer_raw, 0)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot_len = usize_from_u64(footer.snapshot_len, "snapshot range")?;
    if snapshot_len != SNAPSHOT_LEN
        || snapshot_offset
            .checked_add(snapshot_len)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "snapshot range",
            )))?
            != footer_offset
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "snapshot range",
        )));
    }
    let snapshot = reader.read_vec(snapshot_offset, snapshot_len, "snapshot")?;
    let snapshot_digest = digest(&[SNAPSHOT_DOMAIN, &snapshot]);
    reader.stats.bytes_hashed += u64::try_from(snapshot.len())
        .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?;
    if snapshot_digest != footer.snapshot_digest
        || &snapshot[..8] != SNAPSHOT_MAGIC
        || u64_at(&snapshot, 8, "snapshot")? != footer.sequence
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "snapshot",
        )));
    }
    let parent_snapshot_digest = array::<32>(&snapshot, 64, "snapshot parent")?;
    let commit_start = if footer.previous_footer_offset == ABSENT_OFFSET {
        if footer.sequence != 0 || parent_snapshot_digest.iter().any(|byte| *byte != 0) {
            return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                "genesis linkage",
            )));
        }
        0
    } else {
        let previous_offset = usize_from_u64(footer.previous_footer_offset, "previous footer")?;
        let previous_end = previous_offset
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "previous footer",
            )))?;
        if previous_end > snapshot_offset {
            return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                "previous footer",
            )));
        }
        let previous_raw = reader.read_vec(previous_offset, FOOTER_LEN, "previous footer")?;
        let previous = parse_footer(&previous_raw, 0)?;
        if footer.sequence != previous.sequence + 1
            || previous.snapshot_digest != parent_snapshot_digest
        {
            return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                "parent linkage",
            )));
        }
        previous_end
    };

    let mut commit_hasher = Sha256::new();
    commit_hasher.update(COMMIT_DOMAIN);
    reader.hash_range(
        &mut commit_hasher,
        commit_start,
        footer_offset - commit_start,
        "commit",
    )?;
    commit_hasher.update(footer_semantics(&footer));
    let commit_digest: [u8; 32] = commit_hasher.finalize().into();
    if commit_digest != footer.commit_digest {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "commit digest",
        )));
    }

    let root_level_u64 = u64_at(&snapshot, 24, "snapshot root")?;
    let root_level = u8::try_from(root_level_u64)
        .map_err(|_| ImmutableSourceError::Format(ImmutableError::Invalid("snapshot root")))?;
    if root_level > reader.limits.format.max_depth {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit(
            "page depth",
        )));
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

enum PageLookup {
    Next(LookupReference),
    Found(Locator),
    Absent,
}

fn register_page_range(
    known_ranges: &mut Vec<(usize, usize)>,
    offset: usize,
    snapshot_offset: usize,
) -> Result<(), ImmutableSourceError> {
    let end = offset
        .checked_add(PAGE_SIZE)
        .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page range",
        )))?;
    if offset < FILE_HEADER_LEN || end > snapshot_offset {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page range",
        )));
    }
    if known_ranges
        .iter()
        .any(|(start, stop)| offset < *stop && *start < end)
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page overlap",
        )));
    }
    known_ranges.push((offset, end));
    Ok(())
}

fn read_lookup_page<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    reference: &LookupReference,
    object_id: u64,
    envelope: &LookupEnvelope,
    visited: &mut HashSet<usize>,
    known_ranges: &mut Vec<(usize, usize)>,
) -> Result<PageLookup, ImmutableSourceError> {
    if visited.len() >= reader.limits.format.max_pages {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit(
            "page count",
        )));
    }
    if !visited.insert(reference.offset) {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page cycle",
        )));
    }
    if !known_ranges
        .iter()
        .any(|range| *range == (reference.offset, reference.offset + PAGE_SIZE))
    {
        register_page_range(known_ranges, reference.offset, envelope.snapshot_offset)?;
    }
    let page = reader.read_vec(reference.offset, PAGE_SIZE, "page")?;
    let page_digest = digest(&[PAGE_DOMAIN, &page]);
    reader.stats.bytes_hashed += u64::try_from(page.len())
        .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?;
    if page_digest != reference.digest || &page[..8] != PAGE_MAGIC {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page digest",
        )));
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
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page header",
        )));
    }
    if level != reference.level
        || reference
            .range
            .is_some_and(|range| range != (minimum, maximum))
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page reference",
        )));
    }

    match kind {
        1 => {
            if level != 0 || entry_size != LEAF_ENTRY_LEN || count > LEAF_CAPACITY {
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "leaf shape",
                )));
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
                    return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                        "leaf entry",
                    )));
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
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "leaf order",
                )));
            }
            Ok(selected.map_or(PageLookup::Absent, PageLookup::Found))
        }
        2 => {
            if level == 0 || entry_size != INTERNAL_ENTRY_LEN || count > INTERNAL_FANOUT {
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "internal shape",
                )));
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
                    return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                        "child entry",
                    )));
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
                return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
                    "child order",
                )));
            }
            Ok(selected.map_or(PageLookup::Absent, PageLookup::Next))
        }
        _ => Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "page kind",
        ))),
    }
}

fn validate_lookup_object<S: ImmutableReadAt>(
    reader: &mut SourceReader<'_, S>,
    locator: &Locator,
    envelope: &LookupEnvelope,
    known_ranges: &[(usize, usize)],
) -> Result<ImmutableLookupResult, ImmutableSourceError> {
    let offset = usize_from_u64(locator.record_offset, "object range")?;
    let length = usize_from_u64(locator.record_len, "object range")?;
    let end = offset
        .checked_add(length)
        .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object range",
        )))?;
    if offset < FILE_HEADER_LEN
        || end > envelope.snapshot_offset
        || known_ranges
            .iter()
            .any(|(start, stop)| offset < *stop && *start < end)
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object structural overlap",
        )));
    }
    let header = reader.read_vec(offset, OBJECT_HEADER_LEN, "object header")?;
    if &header[..8] != OBJECT_MAGIC
        || usize::from(u16_at(&header, 8, "object header")?) != OBJECT_HEADER_LEN
        || u32_at(&header, 12, "object header")? != 0
        || header[40..].iter().any(|byte| *byte != 0)
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object header",
        )));
    }
    let kind = u16_at(&header, 10, "object header")?;
    let object_id = u64_at(&header, 16, "object header")?;
    let payload_len = usize_at(&header, 24, "object length")?;
    let logical_len = u64_at(&header, 32, "object length")?;
    if kind == 0
        || object_id == 0
        || OBJECT_HEADER_LEN
            .checked_add(payload_len)
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid(
                "object length",
            )))?
            != length
        || u64_from_usize(payload_len)? != logical_len
        || object_id != locator.object_id
        || kind != locator.kind
        || logical_len != locator.logical_len
    {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object locator",
        )));
    }

    let mut object_hasher = Sha256::new();
    object_hasher.update(OBJECT_DOMAIN);
    object_hasher.update(&header);
    reader.stats.bytes_hashed += u64::try_from(header.len())
        .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?;
    reader.hash_range(
        &mut object_hasher,
        offset + OBJECT_HEADER_LEN,
        payload_len,
        "object payload",
    )?;
    let object_digest: [u8; 32] = object_hasher.finalize().into();
    if object_digest != locator.digest {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object digest",
        )));
    }
    Ok(ImmutableLookupResult::Found {
        object_id,
        kind,
        logical_len,
        record_offset: locator.record_offset,
        object_digest,
    })
}

/// Authenticates the exact-end commit, one canonical root-to-leaf path, and the selected object.
///
/// This assurance level may return authenticated absence. It does not claim that unrelated pages
/// or historical objects were traversed or rehashed.
pub fn lookup_at<S: ImmutableReadAt>(
    source: &mut S,
    object_id: u64,
    limits: ImmutableSourceLimits,
) -> Result<ImmutableSourceLookupReport, ImmutableSourceError> {
    if object_id == 0 {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object identifier",
        )));
    }
    let mut reader = SourceReader::new(source, limits)?;
    let envelope = read_lookup_envelope(&mut reader)?;
    let mut visited = HashSet::new();
    let mut known_ranges = vec![
        (envelope.snapshot_offset, envelope.footer_offset),
        (envelope.footer_offset, reader.length),
    ];
    let mut reference = envelope.root.clone();
    let result = loop {
        match read_lookup_page(
            &mut reader,
            &reference,
            object_id,
            &envelope,
            &mut visited,
            &mut known_ranges,
        )? {
            PageLookup::Next(next) => reference = next,
            PageLookup::Found(locator) => {
                break validate_lookup_object(&mut reader, &locator, &envelope, &known_ranges)?;
            }
            PageLookup::Absent => break ImmutableLookupResult::Absent { object_id },
        }
    };
    Ok(ImmutableSourceLookupReport {
        sequence: envelope.sequence,
        snapshot_digest: envelope.snapshot_digest,
        commit_digest: envelope.commit_digest,
        result,
        stats: reader.stats,
    })
}
