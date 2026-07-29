//! Bounded random-access lookup for the provisional EXP-0002 candidate.
//!
//! The implementation streams the active commit digest, reads one authenticated
//! directory path, and streams the selected object digest. It does not
//! materialize the complete file or unrelated historical payloads.

use crate::exp0002::{
    Exp0002Error, FileHeader, Snapshot, ValidationLimits, ABSENT_OFFSET, FOOTER_LEN,
    INTERNAL_ENTRY_LEN, LEAF_ENTRY_LEN, OBJECT_HEADER_LEN, PAGE_HEADER_LEN, PAGE_SIZE,
};
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;

const FOOTER_MAGIC: &[u8; 8] = b"UCOF2END";
const PAGE_MAGIC: &[u8; 4] = b"PG02";
const OBJECT_MAGIC: &[u8; 4] = b"OBJ2";
const COMMIT_DOMAIN: &[u8] = b"UCOF-EXP-0002-COMMIT\0";
const SNAPSHOT_DOMAIN: &[u8] = b"UCOF-EXP-0002-SNAPSHOT\0";
const PAGE_DOMAIN: &[u8] = b"UCOF-EXP-0002-PAGE\0";
const OBJECT_DOMAIN: &[u8] = b"UCOF-EXP-0002-OBJECT\0";

pub trait Exp0002ReadAt {
    fn len(&mut self) -> io::Result<u64>;
    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()>;
}

#[derive(Debug, Clone, Copy)]
pub struct Exp0002SliceSource<'a> {
    bytes: &'a [u8],
}

impl<'a> Exp0002SliceSource<'a> {
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl Exp0002ReadAt for Exp0002SliceSource<'_> {
    fn len(&mut self) -> io::Result<u64> {
        u64::try_from(self.bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "source length exceeds u64"))
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        let start = usize::try_from(offset)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "offset exceeds usize"))?;
        let end = start
            .checked_add(buffer.len())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "source range overflow"))?;
        let source = self.bytes.get(start..end).ok_or_else(|| {
            io::Error::new(io::ErrorKind::UnexpectedEof, "source range is truncated")
        })?;
        buffer.copy_from_slice(source);
        Ok(())
    }
}

#[derive(Debug)]
pub struct Exp0002SeekSource<R> {
    inner: R,
}

impl<R> Exp0002SeekSource<R> {
    #[must_use]
    pub const fn new(inner: R) -> Self {
        Self { inner }
    }

    #[must_use]
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read + Seek> Exp0002ReadAt for Exp0002SeekSource<R> {
    fn len(&mut self) -> io::Result<u64> {
        self.inner.seek(SeekFrom::End(0))
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        self.inner.seek(SeekFrom::Start(offset))?;
        self.inner.read_exact(buffer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exp0002SourceError {
    Format(Exp0002Error),
    Io(&'static str),
}

impl fmt::Display for Exp0002SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => error.fmt(formatter),
            Self::Io(context) => write!(formatter, "source I/O failure: {context}"),
        }
    }
}

impl std::error::Error for Exp0002SourceError {}

impl From<Exp0002Error> for Exp0002SourceError {
    fn from(value: Exp0002Error) -> Self {
        Self::Format(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002SourceLimits {
    pub validation: ValidationLimits,
    pub max_source_bytes_read: u64,
    pub max_read_operations: u64,
    pub max_read_request_bytes: usize,
    pub hash_block_bytes: usize,
    pub max_page_reads: usize,
}

impl Default for Exp0002SourceLimits {
    fn default() -> Self {
        Self {
            validation: ValidationLimits::default(),
            max_source_bytes_read: 32 * 1024 * 1024 * 1024,
            max_read_operations: 1_000_000,
            max_read_request_bytes: 64 * 1024,
            hash_block_bytes: 64 * 1024,
            max_page_reads: 32,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Exp0002SourceStats {
    pub read_operations: u64,
    pub bytes_read: u64,
    pub largest_request: u64,
    pub bytes_hashed: u64,
    pub pages_read: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002SourceLookup {
    pub object_id: u64,
    pub kind: u16,
    pub record_offset: u64,
    pub record_len: u64,
    pub payload_offset: u64,
    pub payload_len: u64,
    pub logical_len: u64,
    pub sequence: u64,
    pub stats: Exp0002SourceStats,
}

#[derive(Clone)]
struct SourceFooter {
    commit_start: u64,
    commit_len: u64,
    snapshot_offset: u64,
    snapshot_len: u64,
    sequence: u64,
    previous_footer_offset: u64,
    snapshot_digest: [u8; 32],
    commit_digest: [u8; 32],
    semantics: [u8; 104],
}

#[derive(Clone)]
struct ExpectedPage {
    offset: u64,
    digest: [u8; 32],
    level: u16,
    minimum: Option<u64>,
    maximum: Option<u64>,
}

#[derive(Clone)]
struct SelectedObject {
    object_id: u64,
    kind: u16,
    record_offset: u64,
    record_len: u64,
    logical_len: u64,
    digest: [u8; 32],
}

#[derive(Clone)]
struct PageHeader {
    kind: u8,
    level: u8,
    count: usize,
    entry_size: usize,
    minimum: u64,
    maximum: u64,
}

pub fn lookup_authenticated_at<S: Exp0002ReadAt>(
    source: &mut S,
    object_id: u64,
    limits: &Exp0002SourceLimits,
) -> Result<Option<Exp0002SourceLookup>, Exp0002SourceError> {
    if object_id == 0 {
        return Err(Exp0002Error::InvalidObjectId.into());
    }
    validate_limit_configuration(limits)?;
    let file_len = source
        .len()
        .map_err(|_| Exp0002SourceError::Io("source length"))?;
    if file_len > limits.validation.max_file_bytes {
        return Err(Exp0002Error::ResourceLimit("file bytes").into());
    }
    let minimum = u64::try_from(64 + FOOTER_LEN).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
    if file_len < minimum {
        return Err(Exp0002Error::Truncated.into());
    }
    let mut reader = BudgetedSource::new(source, file_len, limits);
    let header = reader.read_array::<64>(0, "file header")?;
    FileHeader::parse(&header)?;

    let footer_offset = file_len
        .checked_sub(FOOTER_LEN as u64)
        .ok_or(Exp0002Error::Truncated)?;
    let footer_bytes = reader.read_array::<FOOTER_LEN>(footer_offset, "footer")?;
    let footer = parse_footer(&footer_bytes)?;
    if footer
        .commit_start
        .checked_add(footer.commit_len)
        .ok_or(Exp0002Error::ArithmeticOverflow)?
        != footer_offset
    {
        return Err(Exp0002Error::InvalidCommitRange.into());
    }
    if footer.commit_len > limits.validation.max_commit_bytes {
        return Err(Exp0002Error::ResourceLimit("commit bytes").into());
    }
    let mut commit_hasher = Sha256::new();
    commit_hasher.update(COMMIT_DOMAIN);
    reader.hash_range(
        &mut commit_hasher,
        footer.commit_start,
        footer.commit_len,
        "commit",
    )?;
    commit_hasher.update(footer.semantics);
    let actual_commit: [u8; 32] = commit_hasher.finalize().into();
    if actual_commit != footer.commit_digest {
        return Err(Exp0002Error::DigestMismatch("commit").into());
    }

    if footer.snapshot_len > limits.validation.max_snapshot_bytes {
        return Err(Exp0002Error::ResourceLimit("snapshot bytes").into());
    }
    let snapshot_end = footer
        .snapshot_offset
        .checked_add(footer.snapshot_len)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    if footer.snapshot_offset < footer.commit_start || snapshot_end > footer_offset {
        return Err(Exp0002Error::InvalidCommitRange.into());
    }
    let snapshot_bytes =
        reader.read_vec(footer.snapshot_offset, footer.snapshot_len, "snapshot")?;
    reader.add_hashed(footer.snapshot_len)?;
    if digest(SNAPSHOT_DOMAIN, &snapshot_bytes) != footer.snapshot_digest {
        return Err(Exp0002Error::DigestMismatch("snapshot").into());
    }
    let snapshot = Snapshot::parse(&snapshot_bytes, &limits.validation)?;
    if snapshot.sequence != footer.sequence
        || snapshot.previous_footer_offset != footer.previous_footer_offset
    {
        return Err(Exp0002Error::InvalidSnapshotSequence.into());
    }
    validate_parent_link(&mut reader, footer_offset, &footer, &snapshot)?;

    let snapshot_range = to_range(footer.snapshot_offset, footer.snapshot_len, file_len)?;
    let footer_range = to_range(footer_offset, FOOTER_LEN as u64, file_len)?;
    let mut page_ranges = Vec::new();
    let mut expected = ExpectedPage {
        offset: snapshot.directory_root_offset,
        digest: snapshot.directory_root_digest,
        level: snapshot.directory_root_level,
        minimum: None,
        maximum: None,
    };

    loop {
        reader.stats.pages_read = reader
            .stats
            .pages_read
            .checked_add(1)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if reader.stats.pages_read > limits.max_page_reads
            || reader.stats.pages_read > limits.validation.max_page_depth
            || reader.stats.pages_read > limits.validation.max_pages
        {
            return Err(Exp0002Error::ResourceLimit("lookup pages").into());
        }
        let page = reader.read_array::<PAGE_SIZE>(expected.offset, "directory page")?;
        reader.add_hashed(PAGE_SIZE as u64)?;
        if digest(PAGE_DOMAIN, &page) != expected.digest {
            return Err(Exp0002Error::DigestMismatch("page").into());
        }
        let page_range = to_range(expected.offset, PAGE_SIZE as u64, file_len)?;
        page_ranges.push(page_range);
        let header = parse_page_header(&page, snapshot.sequence)?;
        if u16::from(header.level) != expected.level
            || expected
                .minimum
                .is_some_and(|value| value != header.minimum)
            || expected
                .maximum
                .is_some_and(|value| value != header.maximum)
        {
            return Err(Exp0002Error::InvalidPageReference.into());
        }
        if object_id < header.minimum || object_id > header.maximum {
            return Ok(None);
        }
        match header.kind {
            1 => {
                let Some(selected) = select_leaf(&page, &header, object_id)? else {
                    return Ok(None);
                };
                let record_range = to_range(selected.record_offset, selected.record_len, file_len)?;
                if page_ranges
                    .iter()
                    .any(|page_range| ranges_overlap(&record_range, page_range))
                    || ranges_overlap(&record_range, &snapshot_range)
                    || ranges_overlap(&record_range, &footer_range)
                {
                    return Err(Exp0002Error::PhysicalOverlap.into());
                }
                validate_selected_object(&mut reader, &selected, limits)?;
                let payload_len = selected
                    .record_len
                    .checked_sub(OBJECT_HEADER_LEN as u64)
                    .ok_or(Exp0002Error::InvalidLength("object record"))?;
                return Ok(Some(Exp0002SourceLookup {
                    object_id: selected.object_id,
                    kind: selected.kind,
                    record_offset: selected.record_offset,
                    record_len: selected.record_len,
                    payload_offset: selected
                        .record_offset
                        .checked_add(OBJECT_HEADER_LEN as u64)
                        .ok_or(Exp0002Error::ArithmeticOverflow)?,
                    payload_len,
                    logical_len: selected.logical_len,
                    sequence: footer.sequence,
                    stats: reader.stats,
                }));
            }
            2 => {
                let Some(child) = select_internal(&page, &header, object_id)? else {
                    return Ok(None);
                };
                expected = child;
            }
            _ => return Err(Exp0002Error::InvalidPageKind.into()),
        }
    }
}

fn validate_limit_configuration(limits: &Exp0002SourceLimits) -> Result<(), Exp0002SourceError> {
    if limits.max_read_request_bytes < PAGE_SIZE
        || limits.hash_block_bytes == 0
        || limits.hash_block_bytes > limits.max_read_request_bytes
    {
        return Err(Exp0002Error::ResourceLimit("source request configuration").into());
    }
    Ok(())
}

struct BudgetedSource<'a, S> {
    source: &'a mut S,
    file_len: u64,
    limits: &'a Exp0002SourceLimits,
    stats: Exp0002SourceStats,
}

impl<'a, S: Exp0002ReadAt> BudgetedSource<'a, S> {
    fn new(source: &'a mut S, file_len: u64, limits: &'a Exp0002SourceLimits) -> Self {
        Self {
            source,
            file_len,
            limits,
            stats: Exp0002SourceStats::default(),
        }
    }

    fn read_array<const N: usize>(
        &mut self,
        offset: u64,
        context: &'static str,
    ) -> Result<[u8; N], Exp0002SourceError> {
        let mut bytes = [0_u8; N];
        self.read_exact(offset, &mut bytes, context)?;
        Ok(bytes)
    }

    fn read_vec(
        &mut self,
        offset: u64,
        length: u64,
        context: &'static str,
    ) -> Result<Vec<u8>, Exp0002SourceError> {
        let length = usize::try_from(length).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
        let mut bytes = vec![0_u8; length];
        let mut cursor = 0_usize;
        while cursor < length {
            let take = (length - cursor).min(self.limits.max_read_request_bytes);
            let read_offset = offset
                .checked_add(u64::try_from(cursor).map_err(|_| Exp0002Error::ArithmeticOverflow)?)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            self.read_exact(read_offset, &mut bytes[cursor..cursor + take], context)?;
            cursor += take;
        }
        Ok(bytes)
    }

    fn read_exact(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
        context: &'static str,
    ) -> Result<(), Exp0002SourceError> {
        if buffer.len() > self.limits.max_read_request_bytes {
            return Err(Exp0002Error::ResourceLimit("source read request").into());
        }
        let length = u64::try_from(buffer.len()).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
        let end = offset
            .checked_add(length)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if end > self.file_len {
            return Err(Exp0002Error::Truncated.into());
        }
        self.stats.read_operations = self
            .stats
            .read_operations
            .checked_add(1)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if self.stats.read_operations > self.limits.max_read_operations {
            return Err(Exp0002Error::ResourceLimit("source read operations").into());
        }
        self.stats.bytes_read = self
            .stats
            .bytes_read
            .checked_add(length)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if self.stats.bytes_read > self.limits.max_source_bytes_read {
            return Err(Exp0002Error::ResourceLimit("source bytes read").into());
        }
        self.stats.largest_request = self.stats.largest_request.max(length);
        self.source
            .read_exact_at(offset, buffer)
            .map_err(|_| Exp0002SourceError::Io(context))
    }

    fn hash_range(
        &mut self,
        hasher: &mut Sha256,
        offset: u64,
        length: u64,
        context: &'static str,
    ) -> Result<(), Exp0002SourceError> {
        let mut remaining = length;
        let mut cursor = offset;
        let mut block = vec![0_u8; self.limits.hash_block_bytes];
        while remaining > 0 {
            let take = usize::try_from(remaining.min(block.len() as u64))
                .map_err(|_| Exp0002Error::ArithmeticOverflow)?;
            self.read_exact(cursor, &mut block[..take], context)?;
            hasher.update(&block[..take]);
            cursor = cursor
                .checked_add(take as u64)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            remaining -= take as u64;
        }
        self.add_hashed(length)
    }

    fn add_hashed(&mut self, length: u64) -> Result<(), Exp0002SourceError> {
        self.stats.bytes_hashed = self
            .stats
            .bytes_hashed
            .checked_add(length)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if self.stats.bytes_hashed > self.limits.validation.max_hashed_bytes {
            return Err(Exp0002Error::ResourceLimit("hashed bytes").into());
        }
        Ok(())
    }
}

fn parse_footer(bytes: &[u8; FOOTER_LEN]) -> Result<SourceFooter, Exp0002Error> {
    if &bytes[0..8] != FOOTER_MAGIC {
        return Err(Exp0002Error::InvalidMagic("footer"));
    }
    if usize::from(read_u16(bytes, 8)?) != FOOTER_LEN || read_u16(bytes, 10)? != 2 {
        return Err(Exp0002Error::InvalidVersion);
    }
    if read_u32(bytes, 12)? != 0 || read_u16(bytes, 72)? != 1 {
        return Err(Exp0002Error::InvalidFlags("footer"));
    }
    require_zero(&bytes[74..80], "footer")?;
    require_zero(&bytes[144..160], "footer")?;
    Ok(SourceFooter {
        commit_start: read_u64(bytes, 16)?,
        commit_len: read_u64(bytes, 24)?,
        snapshot_offset: read_u64(bytes, 32)?,
        snapshot_len: read_u64(bytes, 40)?,
        sequence: read_u64(bytes, 48)?,
        previous_footer_offset: read_u64(bytes, 56)?,
        snapshot_digest: read_array(bytes, 80)?,
        commit_digest: read_array(bytes, 112)?,
        semantics: read_array(bytes, 8)?,
    })
}

fn validate_parent_link<S: Exp0002ReadAt>(
    reader: &mut BudgetedSource<'_, S>,
    footer_offset: u64,
    footer: &SourceFooter,
    snapshot: &Snapshot,
) -> Result<(), Exp0002SourceError> {
    if footer.previous_footer_offset == ABSENT_OFFSET {
        if footer.sequence != 0
            || footer.commit_start != 0
            || snapshot.parent_snapshot_digest != [0_u8; 32]
        {
            return Err(Exp0002Error::InvalidParent.into());
        }
        return Ok(());
    }
    if footer.previous_footer_offset >= footer_offset
        || footer.commit_start
            != footer
                .previous_footer_offset
                .checked_add(FOOTER_LEN as u64)
                .ok_or(Exp0002Error::ArithmeticOverflow)?
    {
        return Err(Exp0002Error::InvalidPreviousFooter.into());
    }
    let previous_bytes =
        reader.read_array::<FOOTER_LEN>(footer.previous_footer_offset, "previous footer")?;
    let previous = parse_footer(&previous_bytes)?;
    if previous.snapshot_digest != snapshot.parent_snapshot_digest
        || previous
            .sequence
            .checked_add(1)
            .ok_or(Exp0002Error::ArithmeticOverflow)?
            != footer.sequence
    {
        return Err(Exp0002Error::InvalidParent.into());
    }
    Ok(())
}

fn parse_page_header(page: &[u8; PAGE_SIZE], sequence: u64) -> Result<PageHeader, Exp0002Error> {
    if &page[0..4] != PAGE_MAGIC {
        return Err(Exp0002Error::InvalidMagic("page"));
    }
    let kind = page[4];
    let level = page[5];
    if usize::from(read_u16(page, 6)?) != PAGE_HEADER_LEN {
        return Err(Exp0002Error::InvalidLength("page header"));
    }
    let count = usize::from(read_u16(page, 8)?);
    if count == 0 {
        return Err(Exp0002Error::InvalidEntryCount);
    }
    let entry_size = usize::from(read_u16(page, 10)?);
    if read_u32(page, 12)? != 0 || read_u64(page, 32)? != sequence {
        return Err(Exp0002Error::InvalidFlags("page"));
    }
    let minimum = read_u64(page, 16)?;
    let maximum = read_u64(page, 24)?;
    if minimum == 0 || minimum > maximum {
        return Err(Exp0002Error::InvalidPageRange);
    }
    require_zero(&page[40..64], "page header")?;
    Ok(PageHeader {
        kind,
        level,
        count,
        entry_size,
        minimum,
        maximum,
    })
}

fn select_leaf(
    page: &[u8; PAGE_SIZE],
    header: &PageHeader,
    object_id: u64,
) -> Result<Option<SelectedObject>, Exp0002Error> {
    let capacity = (PAGE_SIZE - PAGE_HEADER_LEN) / LEAF_ENTRY_LEN;
    if header.level != 0 || header.entry_size != LEAF_ENTRY_LEN || header.count > capacity {
        return Err(Exp0002Error::InvalidEntrySize);
    }
    let used = PAGE_HEADER_LEN
        .checked_add(
            header
                .count
                .checked_mul(LEAF_ENTRY_LEN)
                .ok_or(Exp0002Error::ArithmeticOverflow)?,
        )
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    require_zero(&page[used..], "page padding")?;
    let mut previous = None;
    let mut selected = None;
    for index in 0..header.count {
        let start = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
        let entry = &page[start..start + LEAF_ENTRY_LEN];
        let key = read_u64(entry, 0)?;
        if key == 0 || previous.is_some_and(|value| value >= key) {
            return Err(Exp0002Error::UnorderedEntries);
        }
        let kind = read_u16(entry, 8)?;
        if kind == 0 || read_u16(entry, 10)? != 0 || read_u32(entry, 12)? != 0 {
            return Err(Exp0002Error::InvalidFlags("leaf entry"));
        }
        require_zero(&entry[72..88], "leaf entry")?;
        if key == object_id {
            selected = Some(SelectedObject {
                object_id: key,
                kind,
                record_offset: read_u64(entry, 16)?,
                record_len: read_u64(entry, 24)?,
                logical_len: read_u64(entry, 32)?,
                digest: read_array(entry, 40)?,
            });
        }
        previous = Some(key);
    }
    let first = read_u64(page, PAGE_HEADER_LEN)?;
    let last_offset = PAGE_HEADER_LEN + (header.count - 1) * LEAF_ENTRY_LEN;
    let last = read_u64(page, last_offset)?;
    if first != header.minimum || last != header.maximum {
        return Err(Exp0002Error::InvalidPageRange);
    }
    Ok(selected)
}

fn select_internal(
    page: &[u8; PAGE_SIZE],
    header: &PageHeader,
    object_id: u64,
) -> Result<Option<ExpectedPage>, Exp0002Error> {
    let capacity = (PAGE_SIZE - PAGE_HEADER_LEN) / INTERNAL_ENTRY_LEN;
    if header.level == 0 || header.entry_size != INTERNAL_ENTRY_LEN || header.count > capacity {
        return Err(Exp0002Error::InvalidEntrySize);
    }
    let used = PAGE_HEADER_LEN
        .checked_add(
            header
                .count
                .checked_mul(INTERNAL_ENTRY_LEN)
                .ok_or(Exp0002Error::ArithmeticOverflow)?,
        )
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    require_zero(&page[used..], "page padding")?;
    let mut previous_max = None;
    let mut selected = None;
    let mut first_min = None;
    let mut last_max = None;
    for index in 0..header.count {
        let start = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
        let entry = &page[start..start + INTERNAL_ENTRY_LEN];
        let minimum = read_u64(entry, 0)?;
        let maximum = read_u64(entry, 8)?;
        if minimum == 0 || minimum > maximum || previous_max.is_some_and(|value| value >= minimum) {
            return Err(Exp0002Error::OverlappingRanges);
        }
        if read_u32(entry, 24)? as usize != PAGE_SIZE
            || read_u16(entry, 28)?.checked_add(1) != Some(u16::from(header.level))
            || read_u16(entry, 30)? != 0
        {
            return Err(Exp0002Error::InvalidPageReference);
        }
        first_min.get_or_insert(minimum);
        last_max = Some(maximum);
        if minimum <= object_id && object_id <= maximum {
            selected = Some(ExpectedPage {
                offset: read_u64(entry, 16)?,
                digest: read_array(entry, 32)?,
                level: read_u16(entry, 28)?,
                minimum: Some(minimum),
                maximum: Some(maximum),
            });
        }
        previous_max = Some(maximum);
    }
    if first_min != Some(header.minimum) || last_max != Some(header.maximum) {
        return Err(Exp0002Error::InvalidPageRange);
    }
    Ok(selected)
}

fn validate_selected_object<S: Exp0002ReadAt>(
    reader: &mut BudgetedSource<'_, S>,
    selected: &SelectedObject,
    limits: &Exp0002SourceLimits,
) -> Result<(), Exp0002SourceError> {
    if selected.record_len < OBJECT_HEADER_LEN as u64 {
        return Err(Exp0002Error::InvalidLength("object record").into());
    }
    let header = reader.read_array::<OBJECT_HEADER_LEN>(selected.record_offset, "object header")?;
    if &header[0..4] != OBJECT_MAGIC
        || usize::from(read_u16(&header, 4)?) != OBJECT_HEADER_LEN
        || read_u16(&header, 6)? != selected.kind
        || read_u32(&header, 8)? != 0
        || read_u64(&header, 12)? != selected.object_id
    {
        return Err(Exp0002Error::InvalidLength("object header").into());
    }
    let payload_len = read_u64(&header, 20)?;
    let logical_len = read_u64(&header, 28)?;
    if payload_len != logical_len
        || logical_len != selected.logical_len
        || payload_len > limits.validation.max_payload_bytes
        || (OBJECT_HEADER_LEN as u64)
            .checked_add(payload_len)
            .ok_or(Exp0002Error::ArithmeticOverflow)?
            != selected.record_len
    {
        return Err(Exp0002Error::LogicalLengthMismatch.into());
    }
    require_zero(&header[36..48], "object")?;
    let mut hasher = Sha256::new();
    hasher.update(OBJECT_DOMAIN);
    hasher.update(header);
    let payload_offset = selected
        .record_offset
        .checked_add(OBJECT_HEADER_LEN as u64)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    reader.hash_range(&mut hasher, payload_offset, payload_len, "object payload")?;
    reader.add_hashed(OBJECT_HEADER_LEN as u64)?;
    let actual: [u8; 32] = hasher.finalize().into();
    if actual != selected.digest {
        return Err(Exp0002Error::DigestMismatch("object").into());
    }
    Ok(())
}

fn digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    hasher.finalize().into()
}

fn to_range(offset: u64, length: u64, total: u64) -> Result<Range<u64>, Exp0002Error> {
    let end = offset
        .checked_add(length)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    if end > total || offset > end {
        return Err(Exp0002Error::Truncated);
    }
    Ok(offset..end)
}

fn ranges_overlap(left: &Range<u64>, right: &Range<u64>) -> bool {
    left.start < right.end && right.start < left.end
}

fn require_zero(bytes: &[u8], name: &'static str) -> Result<(), Exp0002Error> {
    if bytes.iter().any(|byte| *byte != 0) {
        Err(Exp0002Error::InvalidReserved(name))
    } else {
        Ok(())
    }
}

fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], Exp0002Error> {
    let end = offset
        .checked_add(N)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    bytes
        .get(offset..end)
        .ok_or(Exp0002Error::Truncated)?
        .try_into()
        .map_err(|_| Exp0002Error::Truncated)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, Exp0002Error> {
    Ok(u16::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Exp0002Error> {
    Ok(u32::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Exp0002Error> {
    Ok(u64::from_le_bytes(read_array(bytes, offset)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exp0002::{build_append, build_genesis, validate_strict, ObjectInput};

    fn header() -> FileHeader {
        FileHeader {
            file_id: *b"exp0002-file-id!",
            creation_nonce: *b"fixed-nonce-0002",
        }
    }

    fn object(id: u64, payload: Vec<u8>, root: bool) -> ObjectInput {
        ObjectInput {
            object_id: id,
            kind: 1,
            payload,
            is_root: root,
        }
    }

    #[derive(Debug)]
    struct RecordingSource<'a> {
        bytes: &'a [u8],
        ranges: Vec<Range<u64>>,
    }

    impl<'a> RecordingSource<'a> {
        fn new(bytes: &'a [u8]) -> Self {
            Self {
                bytes,
                ranges: Vec::new(),
            }
        }
    }

    impl Exp0002ReadAt for RecordingSource<'_> {
        fn len(&mut self) -> io::Result<u64> {
            Ok(self.bytes.len() as u64)
        }

        fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
            let start = usize::try_from(offset)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "offset"))?;
            let end = start
                .checked_add(buffer.len())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "range"))?;
            let source = self
                .bytes
                .get(start..end)
                .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "range"))?;
            buffer.copy_from_slice(source);
            self.ranges.push(offset..offset + buffer.len() as u64);
            Ok(())
        }
    }

    #[test]
    fn slice_and_seek_sources_match() {
        let bytes = build_genesis(
            header(),
            vec![
                object(1, b"one".to_vec(), true),
                object(2, b"two".to_vec(), false),
            ],
        )
        .expect("genesis");
        let mut slice = Exp0002SliceSource::new(&bytes);
        let slice_result = lookup_authenticated_at(&mut slice, 2, &Exp0002SourceLimits::default())
            .expect("slice")
            .expect("object");
        let cursor = std::io::Cursor::new(bytes);
        let mut seek = Exp0002SeekSource::new(cursor);
        let seek_result = lookup_authenticated_at(&mut seek, 2, &Exp0002SourceLimits::default())
            .expect("seek")
            .expect("object");
        assert_eq!(slice_result, seek_result);
    }

    #[test]
    fn range_lookup_skips_unrelated_large_historical_payload() {
        let large = vec![7_u8; 1024 * 1024];
        let genesis = build_genesis(
            header(),
            vec![object(1, b"root".to_vec(), true), object(2, large, false)],
        )
        .expect("genesis");
        let genesis_report =
            validate_strict(&genesis, &ValidationLimits::default()).expect("valid");
        let large_entry = genesis_report
            .objects
            .iter()
            .find(|entry| entry.object_id == 2)
            .expect("large entry");
        let large_payload = large_entry.record_offset + OBJECT_HEADER_LEN as u64
            ..large_entry.record_offset + large_entry.record_len;
        let appended = build_append(
            &genesis,
            vec![object(3, b"new".to_vec(), false)],
            vec![1, 3],
            &ValidationLimits::default(),
        )
        .expect("append");
        let mut source = RecordingSource::new(&appended);
        let result = lookup_authenticated_at(
            &mut source,
            1,
            &Exp0002SourceLimits {
                max_source_bytes_read: 256 * 1024,
                hash_block_bytes: 4096,
                max_read_request_bytes: PAGE_SIZE,
                ..Exp0002SourceLimits::default()
            },
        )
        .expect("lookup")
        .expect("root");
        assert_eq!(result.object_id, 1);
        assert!(result.stats.bytes_read < 128 * 1024);
        assert!(source
            .ranges
            .iter()
            .all(|range| !ranges_overlap(range, &large_payload)));
        assert!(result.stats.largest_request <= PAGE_SIZE as u64);
    }

    #[test]
    fn source_read_budget_fails_closed() {
        let bytes =
            build_genesis(header(), vec![object(1, b"one".to_vec(), true)]).expect("genesis");
        let mut source = Exp0002SliceSource::new(&bytes);
        let error = lookup_authenticated_at(
            &mut source,
            1,
            &Exp0002SourceLimits {
                max_source_bytes_read: 64,
                ..Exp0002SourceLimits::default()
            },
        )
        .expect_err("read budget");
        assert_eq!(
            error,
            Exp0002SourceError::Format(Exp0002Error::ResourceLimit("source bytes read"))
        );
    }

    #[test]
    fn source_lookup_reports_authenticated_absence() {
        let bytes = build_genesis(
            header(),
            vec![
                object(1, b"one".to_vec(), true),
                object(3, b"three".to_vec(), false),
            ],
        )
        .expect("genesis");
        let mut source = Exp0002SliceSource::new(&bytes);
        assert_eq!(
            lookup_authenticated_at(&mut source, 2, &Exp0002SourceLimits::default())
                .expect("absence"),
            None
        );
    }

    #[test]
    fn selected_historical_object_tamper_fails() {
        let genesis = build_genesis(
            header(),
            vec![
                object(1, b"one".to_vec(), true),
                object(2, b"two".to_vec(), false),
            ],
        )
        .expect("genesis");
        let mut appended = build_append(
            &genesis,
            vec![object(3, b"new".to_vec(), false)],
            vec![1, 3],
            &ValidationLimits::default(),
        )
        .expect("append");
        appended[64 + OBJECT_HEADER_LEN] ^= 1;
        let mut source = Exp0002SliceSource::new(&appended);
        assert!(lookup_authenticated_at(&mut source, 1, &Exp0002SourceLimits::default()).is_err());
    }
}
