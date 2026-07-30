//! Full strict validation and explicit recovery over bounded random-access sources.
//!
//! Unlike targeted lookup, strict validation authenticates every reachable page
//! and every object referenced by the active snapshot. Recovery is a separate
//! operation that scans a bounded suffix and validates each candidate as an
//! exact-end prefix before reporting it.

use crate::exp0002::{
    Exp0002Error, FileHeader, Footer, LeafEntry, Snapshot, ValidationLimits, ABSENT_OFFSET,
    DIGEST_SHA256, FILE_HEADER_LEN, FOOTER_LEN, INTERNAL_CAPACITY, INTERNAL_ENTRY_LEN,
    LEAF_CAPACITY, LEAF_ENTRY_LEN, OBJECT_HEADER_LEN, PAGE_HEADER_LEN, PAGE_SIZE,
};
use crate::exp0002_source::{
    Exp0002ReadAt, Exp0002SourceError, Exp0002SourceLimits, Exp0002SourceStats,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io;

const FOOTER_MAGIC: &[u8; 8] = b"UCOF2END";
const PAGE_MAGIC: &[u8; 4] = b"PG02";
const OBJECT_MAGIC: &[u8; 4] = b"OBJ2";
const COMMIT_DOMAIN: &[u8] = b"UCOF-EXP-0002-COMMIT\0";
const SNAPSHOT_DOMAIN: &[u8] = b"UCOF-EXP-0002-SNAPSHOT\0";
const PAGE_DOMAIN: &[u8] = b"UCOF-EXP-0002-PAGE\0";
const OBJECT_DOMAIN: &[u8] = b"UCOF-EXP-0002-OBJECT\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedExp0002Source {
    pub header: FileHeader,
    pub footer_offset: u64,
    pub footer: Footer,
    pub snapshot: Snapshot,
    pub objects: Vec<LeafEntry>,
    pub pages_verified: usize,
    pub stats: Exp0002SourceStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002RecoveryLimits {
    pub candidate: Exp0002SourceLimits,
    pub max_scan_bytes: u64,
    pub max_magic_matches: usize,
    pub max_candidate_validations: usize,
    pub max_results: usize,
    pub max_total_candidate_bytes_read: u64,
}

impl Default for Exp0002RecoveryLimits {
    fn default() -> Self {
        Self {
            candidate: Exp0002SourceLimits::default(),
            max_scan_bytes: 16 * 1024 * 1024,
            max_magic_matches: 4096,
            max_candidate_validations: 1024,
            max_results: 64,
            max_total_candidate_bytes_read: 64 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredExp0002Prefix {
    pub prefix_len: u64,
    pub footer_offset: u64,
    pub sequence: u64,
    pub snapshot_digest: [u8; 32],
    pub validation_stats: Exp0002SourceStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exp0002RecoveryReport {
    pub file_len: u64,
    pub scan_start: u64,
    pub scan_bytes_read: u64,
    pub magic_matches: usize,
    pub candidates_validated: usize,
    pub total_candidate_bytes_read: u64,
    pub results: Vec<RecoveredExp0002Prefix>,
}

#[derive(Clone)]
struct ParsedFooter {
    footer: Footer,
    semantics: [u8; 104],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    fn new(offset: u64, length: u64, total: u64) -> Result<Self, Exp0002Error> {
        let end = offset
            .checked_add(length)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if end > total || offset > end {
            return Err(Exp0002Error::Truncated);
        }
        Ok(Self { start: offset, end })
    }

    fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[derive(Clone)]
struct ExpectedPage {
    offset: u64,
    digest: [u8; 32],
    level: u16,
    minimum: Option<u64>,
    maximum: Option<u64>,
    depth: usize,
}

#[derive(Clone)]
struct InternalEntry {
    minimum: u64,
    maximum: u64,
    offset: u64,
    level: u16,
    digest: [u8; 32],
}

struct ParsedPage {
    kind: u8,
    level: u8,
    minimum: u64,
    maximum: u64,
    sequence: u64,
    leaves: Vec<LeafEntry>,
    children: Vec<InternalEntry>,
}

struct DirectoryValidation {
    entries: Vec<LeafEntry>,
    page_ranges: Vec<ByteRange>,
    pages: usize,
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
            let advance = u64::try_from(take).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
            cursor = cursor
                .checked_add(advance)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            remaining -= advance;
        }
        self.add_hashed(length)
    }

    fn digest_range(
        &mut self,
        domain: &[u8],
        offset: u64,
        length: u64,
        context: &'static str,
    ) -> Result<[u8; 32], Exp0002SourceError> {
        let mut hasher = Sha256::new();
        hasher.update(domain);
        self.hash_range(&mut hasher, offset, length, context)?;
        Ok(hasher.finalize().into())
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

pub fn validate_strict_at<S: Exp0002ReadAt>(
    source: &mut S,
    limits: &Exp0002SourceLimits,
) -> Result<VerifiedExp0002Source, Exp0002SourceError> {
    validate_limit_configuration(limits)?;
    let file_len = source
        .len()
        .map_err(|_| Exp0002SourceError::Io("source length"))?;
    if file_len > limits.validation.max_file_bytes {
        return Err(Exp0002Error::ResourceLimit("file bytes").into());
    }
    let minimum = u64::try_from(FILE_HEADER_LEN + FOOTER_LEN)
        .map_err(|_| Exp0002Error::ArithmeticOverflow)?;
    if file_len < minimum {
        return Err(Exp0002Error::Truncated.into());
    }

    let mut reader = BudgetedSource::new(source, file_len, limits);
    let header_bytes = reader.read_array::<FILE_HEADER_LEN>(0, "file header")?;
    let header = FileHeader::parse(&header_bytes)?;
    let footer_offset = file_len
        .checked_sub(u64::try_from(FOOTER_LEN).map_err(|_| Exp0002Error::ArithmeticOverflow)?)
        .ok_or(Exp0002Error::Truncated)?;
    let footer_bytes = reader.read_array::<FOOTER_LEN>(footer_offset, "footer")?;
    let parsed_footer = parse_footer(&footer_bytes)?;
    let footer = parsed_footer.footer.clone();

    let expected_commit_end = footer
        .commit_start
        .checked_add(footer.commit_len)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    if expected_commit_end != footer_offset {
        return Err(Exp0002Error::InvalidCommitRange.into());
    }
    if footer.commit_len > limits.validation.max_commit_bytes {
        return Err(Exp0002Error::ResourceLimit("commit bytes").into());
    }
    if footer.snapshot_offset < footer.commit_start {
        return Err(Exp0002Error::InvalidCommitRange.into());
    }

    let snapshot_range = ByteRange::new(footer.snapshot_offset, footer.snapshot_len, file_len)?;
    if snapshot_range.end > footer_offset {
        return Err(Exp0002Error::InvalidCommitRange.into());
    }
    if footer.snapshot_len > limits.validation.max_snapshot_bytes {
        return Err(Exp0002Error::ResourceLimit("snapshot bytes").into());
    }
    let snapshot_bytes = reader.read_vec(
        footer.snapshot_offset,
        footer.snapshot_len,
        "snapshot",
    )?;
    reader.add_hashed(footer.snapshot_len)?;
    if digest_bytes(SNAPSHOT_DOMAIN, &snapshot_bytes) != footer.snapshot_digest {
        return Err(Exp0002Error::DigestMismatch("snapshot").into());
    }
    let snapshot = Snapshot::parse(&snapshot_bytes, &limits.validation)?;
    if snapshot.sequence != footer.sequence
        || snapshot.previous_footer_offset != footer.previous_footer_offset
    {
        return Err(Exp0002Error::InvalidSnapshotSequence.into());
    }
    validate_parent_link(&mut reader, footer_offset, &footer, &snapshot)?;

    let footer_range = ByteRange::new(
        footer_offset,
        u64::try_from(FOOTER_LEN).map_err(|_| Exp0002Error::ArithmeticOverflow)?,
        file_len,
    )?;
    let directory = validate_directory(
        &mut reader,
        &snapshot,
        snapshot_range,
        footer_range,
    )?;
    validate_objects(
        &mut reader,
        &directory.entries,
        &directory.page_ranges,
        snapshot_range,
        footer_range,
    )?;

    let object_ids: BTreeSet<u64> = directory
        .entries
        .iter()
        .map(|entry| entry.object_id)
        .collect();
    if snapshot.roots.is_empty() || snapshot.roots.iter().any(|root| !object_ids.contains(root)) {
        return Err(Exp0002Error::InvalidRoot.into());
    }

    let mut commit_hasher = Sha256::new();
    commit_hasher.update(COMMIT_DOMAIN);
    reader.hash_range(
        &mut commit_hasher,
        footer.commit_start,
        footer.commit_len,
        "commit",
    )?;
    commit_hasher.update(parsed_footer.semantics);
    let actual_commit: [u8; 32] = commit_hasher.finalize().into();
    if actual_commit != footer.commit_digest {
        return Err(Exp0002Error::DigestMismatch("commit").into());
    }

    Ok(VerifiedExp0002Source {
        header,
        footer_offset,
        footer,
        snapshot,
        objects: directory.entries,
        pages_verified: directory.pages,
        stats: reader.stats,
    })
}

pub fn scan_valid_prefixes_at<S: Exp0002ReadAt>(
    source: &mut S,
    limits: &Exp0002RecoveryLimits,
) -> Result<Exp0002RecoveryReport, Exp0002SourceError> {
    validate_limit_configuration(&limits.candidate)?;
    if limits.max_scan_bytes == 0
        || limits.max_magic_matches == 0
        || limits.max_candidate_validations == 0
        || limits.max_results == 0
    {
        return Err(Exp0002Error::ResourceLimit("recovery configuration").into());
    }
    let file_len = source
        .len()
        .map_err(|_| Exp0002SourceError::Io("recovery source length"))?;
    let scan_len = file_len.min(limits.max_scan_bytes);
    let scan_start = file_len
        .checked_sub(scan_len)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    let scan_len_usize = usize::try_from(scan_len).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
    let mut scan = vec![0_u8; scan_len_usize];
    source
        .read_exact_at(scan_start, &mut scan)
        .map_err(|_| Exp0002SourceError::Io("recovery scan"))?;

    let mut report = Exp0002RecoveryReport {
        file_len,
        scan_start,
        scan_bytes_read: scan_len,
        magic_matches: 0,
        candidates_validated: 0,
        total_candidate_bytes_read: 0,
        results: Vec::new(),
    };
    if scan.len() < FOOTER_MAGIC.len() {
        return Ok(report);
    }

    let mut positions = Vec::new();
    for index in 0..=scan.len() - FOOTER_MAGIC.len() {
        if &scan[index..index + FOOTER_MAGIC.len()] == FOOTER_MAGIC {
            report.magic_matches = report
                .magic_matches
                .checked_add(1)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            if report.magic_matches > limits.max_magic_matches {
                return Err(Exp0002Error::ResourceLimit("recovery magic matches").into());
            }
            positions.push(index);
        }
    }

    for index in positions.into_iter().rev() {
        if report.candidates_validated >= limits.max_candidate_validations {
            return Err(Exp0002Error::ResourceLimit("recovery candidates").into());
        }
        let relative = u64::try_from(index).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
        let footer_offset = scan_start
            .checked_add(relative)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        let prefix_len = footer_offset
            .checked_add(u64::try_from(FOOTER_LEN).map_err(|_| Exp0002Error::ArithmeticOverflow)?)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if prefix_len > file_len {
            continue;
        }
        report.candidates_validated += 1;
        let mut prefix = PrefixSource {
            source: &mut *source,
            len: prefix_len,
        };
        if let Ok(verified) = validate_strict_at(&mut prefix, &limits.candidate) {
            report.total_candidate_bytes_read = report
                .total_candidate_bytes_read
                .checked_add(verified.stats.bytes_read)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            if report.total_candidate_bytes_read > limits.max_total_candidate_bytes_read {
                return Err(Exp0002Error::ResourceLimit("recovery validation bytes").into());
            }
            report.results.push(RecoveredExp0002Prefix {
                prefix_len,
                footer_offset: verified.footer_offset,
                sequence: verified.footer.sequence,
                snapshot_digest: verified.footer.snapshot_digest,
                validation_stats: verified.stats,
            });
            if report.results.len() > limits.max_results {
                return Err(Exp0002Error::ResourceLimit("recovery results").into());
            }
        }
    }
    Ok(report)
}

struct PrefixSource<'a, S> {
    source: &'a mut S,
    len: u64,
}

impl<S: Exp0002ReadAt> Exp0002ReadAt for PrefixSource<'_, S> {
    fn len(&mut self) -> io::Result<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        let length = u64::try_from(buffer.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "range length exceeds u64"))?;
        let end = offset
            .checked_add(length)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "prefix range overflow"))?;
        if end > self.len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "prefix range is truncated",
            ));
        }
        self.source.read_exact_at(offset, buffer)
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

fn parse_footer(bytes: &[u8; FOOTER_LEN]) -> Result<ParsedFooter, Exp0002Error> {
    if &bytes[0..8] != FOOTER_MAGIC {
        return Err(Exp0002Error::InvalidMagic("footer"));
    }
    if usize::from(read_u16(bytes, 8)?) != FOOTER_LEN {
        return Err(Exp0002Error::InvalidLength("footer"));
    }
    if read_u16(bytes, 10)? != 2 {
        return Err(Exp0002Error::InvalidVersion);
    }
    if read_u32(bytes, 12)? != 0 {
        return Err(Exp0002Error::InvalidFlags("footer"));
    }
    if read_u16(bytes, 72)? != DIGEST_SHA256 {
        return Err(Exp0002Error::UnsupportedDigest);
    }
    require_zero(&bytes[74..80], "footer")?;
    require_zero(&bytes[144..160], "footer")?;
    Ok(ParsedFooter {
        footer: Footer {
            commit_start: read_u64(bytes, 16)?,
            commit_len: read_u64(bytes, 24)?,
            snapshot_offset: read_u64(bytes, 32)?,
            snapshot_len: read_u64(bytes, 40)?,
            sequence: read_u64(bytes, 48)?,
            previous_footer_offset: read_u64(bytes, 56)?,
            record_count: read_u64(bytes, 64)?,
            snapshot_digest: read_array(bytes, 80)?,
            commit_digest: read_array(bytes, 112)?,
        },
        semantics: read_array(bytes, 8)?,
    })
}

fn validate_parent_link<S: Exp0002ReadAt>(
    reader: &mut BudgetedSource<'_, S>,
    footer_offset: u64,
    footer: &Footer,
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
                .checked_add(u64::try_from(FOOTER_LEN).map_err(|_| Exp0002Error::ArithmeticOverflow)?)
                .ok_or(Exp0002Error::ArithmeticOverflow)?
    {
        return Err(Exp0002Error::InvalidPreviousFooter.into());
    }
    let previous_bytes =
        reader.read_array::<FOOTER_LEN>(footer.previous_footer_offset, "previous footer")?;
    let previous = parse_footer(&previous_bytes)?.footer;
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

fn validate_directory<S: Exp0002ReadAt>(
    reader: &mut BudgetedSource<'_, S>,
    snapshot: &Snapshot,
    snapshot_range: ByteRange,
    footer_range: ByteRange,
) -> Result<DirectoryValidation, Exp0002SourceError> {
    let mut stack = vec![ExpectedPage {
        offset: snapshot.directory_root_offset,
        digest: snapshot.directory_root_digest,
        level: snapshot.directory_root_level,
        minimum: None,
        maximum: None,
        depth: 1,
    }];
    let mut visited = BTreeSet::new();
    let mut entries = Vec::new();
    let mut page_ranges = Vec::new();

    while let Some(expected) = stack.pop() {
        if expected.depth > reader.limits.validation.max_page_depth {
            return Err(Exp0002Error::ResourceLimit("page depth").into());
        }
        if !visited.insert(expected.offset) {
            return Err(Exp0002Error::PageCycle.into());
        }
        if visited.len() > reader.limits.validation.max_pages {
            return Err(Exp0002Error::ResourceLimit("pages").into());
        }
        reader.stats.pages_read = reader
            .stats
            .pages_read
            .checked_add(1)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if reader.stats.pages_read > reader.limits.max_page_reads {
            return Err(Exp0002Error::ResourceLimit("source page reads").into());
        }

        let range = ByteRange::new(
            expected.offset,
            u64::try_from(PAGE_SIZE).map_err(|_| Exp0002Error::ArithmeticOverflow)?,
            reader.file_len,
        )?;
        if range.overlaps(snapshot_range)
            || range.overlaps(footer_range)
            || page_ranges.iter().any(|other| range.overlaps(*other))
        {
            return Err(Exp0002Error::PhysicalOverlap.into());
        }
        let page = reader.read_array::<PAGE_SIZE>(expected.offset, "directory page")?;
        reader.add_hashed(u64::try_from(PAGE_SIZE).map_err(|_| Exp0002Error::ArithmeticOverflow)?)?;
        if digest_bytes(PAGE_DOMAIN, &page) != expected.digest {
            return Err(Exp0002Error::DigestMismatch("page").into());
        }
        let parsed = parse_page(&page)?;
        if u16::from(parsed.level) != expected.level
            || expected.minimum.is_some_and(|value| value != parsed.minimum)
            || expected.maximum.is_some_and(|value| value != parsed.maximum)
            || parsed.sequence != snapshot.sequence
        {
            return Err(Exp0002Error::InvalidPageReference.into());
        }
        page_ranges.push(range);

        match parsed.kind {
            1 => {
                entries.extend(parsed.leaves);
                if entries.len() > reader.limits.validation.max_objects {
                    return Err(Exp0002Error::ResourceLimit("objects").into());
                }
            }
            2 => {
                for child in parsed.children.into_iter().rev() {
                    stack.push(ExpectedPage {
                        offset: child.offset,
                        digest: child.digest,
                        level: child.level,
                        minimum: Some(child.minimum),
                        maximum: Some(child.maximum),
                        depth: expected
                            .depth
                            .checked_add(1)
                            .ok_or(Exp0002Error::ArithmeticOverflow)?,
                    });
                }
            }
            _ => return Err(Exp0002Error::InvalidPageKind.into()),
        }
    }

    entries.sort_by_key(|entry| entry.object_id);
    if let Some(pair) = entries
        .windows(2)
        .find(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(Exp0002Error::DuplicateObjectId(pair[1].object_id).into());
    }
    Ok(DirectoryValidation {
        entries,
        page_ranges,
        pages: visited.len(),
    })
}

fn parse_page(bytes: &[u8; PAGE_SIZE]) -> Result<ParsedPage, Exp0002Error> {
    if &bytes[0..4] != PAGE_MAGIC {
        return Err(Exp0002Error::InvalidMagic("page"));
    }
    let kind = bytes[4];
    let level = bytes[5];
    if usize::from(read_u16(bytes, 6)?) != PAGE_HEADER_LEN {
        return Err(Exp0002Error::InvalidLength("page header"));
    }
    let count = usize::from(read_u16(bytes, 8)?);
    if count == 0 {
        return Err(Exp0002Error::InvalidEntryCount);
    }
    let entry_size = usize::from(read_u16(bytes, 10)?);
    if read_u32(bytes, 12)? != 0 {
        return Err(Exp0002Error::InvalidFlags("page"));
    }
    let minimum = read_u64(bytes, 16)?;
    let maximum = read_u64(bytes, 24)?;
    if minimum == 0 || minimum > maximum {
        return Err(Exp0002Error::InvalidPageRange);
    }
    require_zero(&bytes[40..64], "page header")?;
    let sequence = read_u64(bytes, 32)?;

    match kind {
        1 => {
            if level != 0 || entry_size != LEAF_ENTRY_LEN || count > LEAF_CAPACITY {
                return Err(if level != 0 {
                    Exp0002Error::InvalidPageLevel
                } else if entry_size != LEAF_ENTRY_LEN {
                    Exp0002Error::InvalidEntrySize
                } else {
                    Exp0002Error::InvalidEntryCount
                });
            }
            let used = PAGE_HEADER_LEN
                .checked_add(
                    count
                        .checked_mul(LEAF_ENTRY_LEN)
                        .ok_or(Exp0002Error::ArithmeticOverflow)?,
                )
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            require_zero(&bytes[used..], "page padding")?;
            let mut leaves = Vec::with_capacity(count);
            for index in 0..count {
                let start = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                leaves.push(parse_leaf_entry(&bytes[start..start + LEAF_ENTRY_LEN])?);
            }
            if leaves.first().map(|entry| entry.object_id) != Some(minimum)
                || leaves.last().map(|entry| entry.object_id) != Some(maximum)
                || leaves
                    .windows(2)
                    .any(|pair| pair[0].object_id >= pair[1].object_id)
            {
                return Err(Exp0002Error::UnorderedEntries);
            }
            Ok(ParsedPage {
                kind,
                level,
                minimum,
                maximum,
                sequence,
                leaves,
                children: Vec::new(),
            })
        }
        2 => {
            if level == 0 || entry_size != INTERNAL_ENTRY_LEN || count > INTERNAL_CAPACITY {
                return Err(if level == 0 {
                    Exp0002Error::InvalidPageLevel
                } else if entry_size != INTERNAL_ENTRY_LEN {
                    Exp0002Error::InvalidEntrySize
                } else {
                    Exp0002Error::InvalidEntryCount
                });
            }
            let used = PAGE_HEADER_LEN
                .checked_add(
                    count
                        .checked_mul(INTERNAL_ENTRY_LEN)
                        .ok_or(Exp0002Error::ArithmeticOverflow)?,
                )
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            require_zero(&bytes[used..], "page padding")?;
            let mut children = Vec::with_capacity(count);
            for index in 0..count {
                let start = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                children.push(parse_internal_entry(
                    &bytes[start..start + INTERNAL_ENTRY_LEN],
                )?);
            }
            if children.first().map(|entry| entry.minimum) != Some(minimum)
                || children.last().map(|entry| entry.maximum) != Some(maximum)
                || children
                    .windows(2)
                    .any(|pair| pair[0].maximum >= pair[1].minimum)
                || children.iter().any(|child| {
                    child.level.checked_add(1) != Some(u16::from(level))
                })
            {
                return Err(Exp0002Error::InvalidPageReference);
            }
            Ok(ParsedPage {
                kind,
                level,
                minimum,
                maximum,
                sequence,
                leaves: Vec::new(),
                children,
            })
        }
        _ => Err(Exp0002Error::InvalidPageKind),
    }
}

fn parse_leaf_entry(bytes: &[u8]) -> Result<LeafEntry, Exp0002Error> {
    if bytes.len() != LEAF_ENTRY_LEN {
        return Err(Exp0002Error::InvalidEntrySize);
    }
    let object_id = read_u64(bytes, 0)?;
    let kind = read_u16(bytes, 8)?;
    if object_id == 0 {
        return Err(Exp0002Error::InvalidObjectId);
    }
    if kind == 0 {
        return Err(Exp0002Error::InvalidObjectKind);
    }
    if read_u16(bytes, 10)? != 0 || read_u32(bytes, 12)? != 0 {
        return Err(Exp0002Error::InvalidFlags("leaf entry"));
    }
    require_zero(&bytes[72..88], "leaf entry")?;
    Ok(LeafEntry {
        object_id,
        kind,
        record_offset: read_u64(bytes, 16)?,
        record_len: read_u64(bytes, 24)?,
        logical_len: read_u64(bytes, 32)?,
        record_digest: read_array(bytes, 40)?,
    })
}

fn parse_internal_entry(bytes: &[u8]) -> Result<InternalEntry, Exp0002Error> {
    if bytes.len() != INTERNAL_ENTRY_LEN {
        return Err(Exp0002Error::InvalidEntrySize);
    }
    let minimum = read_u64(bytes, 0)?;
    let maximum = read_u64(bytes, 8)?;
    if minimum == 0 || minimum > maximum {
        return Err(Exp0002Error::InvalidPageRange);
    }
    if usize::try_from(read_u32(bytes, 24)?).map_err(|_| Exp0002Error::ArithmeticOverflow)?
        != PAGE_SIZE
    {
        return Err(Exp0002Error::InvalidLength("child page"));
    }
    if read_u16(bytes, 30)? != 0 {
        return Err(Exp0002Error::InvalidFlags("internal entry"));
    }
    Ok(InternalEntry {
        minimum,
        maximum,
        offset: read_u64(bytes, 16)?,
        level: read_u16(bytes, 28)?,
        digest: read_array(bytes, 32)?,
    })
}

fn validate_objects<S: Exp0002ReadAt>(
    reader: &mut BudgetedSource<'_, S>,
    entries: &[LeafEntry],
    page_ranges: &[ByteRange],
    snapshot_range: ByteRange,
    footer_range: ByteRange,
) -> Result<(), Exp0002SourceError> {
    let mut physical = Vec::with_capacity(entries.len());
    let mut payload_total = 0_u64;
    for entry in entries {
        if entry.record_len < u64::try_from(OBJECT_HEADER_LEN)
            .map_err(|_| Exp0002Error::ArithmeticOverflow)?
        {
            return Err(Exp0002Error::InvalidLength("object record").into());
        }
        let range = ByteRange::new(entry.record_offset, entry.record_len, reader.file_len)?;
        if range.overlaps(snapshot_range)
            || range.overlaps(footer_range)
            || page_ranges.iter().any(|page| range.overlaps(*page))
        {
            return Err(Exp0002Error::PhysicalOverlap.into());
        }
        let header = reader.read_array::<OBJECT_HEADER_LEN>(entry.record_offset, "object header")?;
        let parsed = parse_object_header(&header)?;
        let expected_len = u64::try_from(OBJECT_HEADER_LEN)
            .map_err(|_| Exp0002Error::ArithmeticOverflow)?
            .checked_add(parsed.payload_len)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if parsed.object_id != entry.object_id
            || parsed.kind != entry.kind
            || parsed.logical_len != entry.logical_len
            || expected_len != entry.record_len
        {
            return Err(Exp0002Error::InvalidLength("object locator").into());
        }
        payload_total = payload_total
            .checked_add(parsed.payload_len)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if payload_total > reader.limits.validation.max_payload_bytes {
            return Err(Exp0002Error::ResourceLimit("payload bytes").into());
        }
        if reader.digest_range(
            OBJECT_DOMAIN,
            entry.record_offset,
            entry.record_len,
            "object record",
        )? != entry.record_digest
        {
            return Err(Exp0002Error::DigestMismatch("object").into());
        }
        physical.push(range);
    }
    physical.sort_by_key(|range| range.start);
    if physical
        .windows(2)
        .any(|pair| pair[0].overlaps(pair[1]))
    {
        return Err(Exp0002Error::PhysicalOverlap.into());
    }
    Ok(())
}

struct ParsedObjectHeader {
    object_id: u64,
    kind: u16,
    payload_len: u64,
    logical_len: u64,
}

fn parse_object_header(
    bytes: &[u8; OBJECT_HEADER_LEN],
) -> Result<ParsedObjectHeader, Exp0002Error> {
    if &bytes[0..4] != OBJECT_MAGIC {
        return Err(Exp0002Error::InvalidMagic("object"));
    }
    if usize::from(read_u16(bytes, 4)?) != OBJECT_HEADER_LEN {
        return Err(Exp0002Error::InvalidLength("object header"));
    }
    let kind = read_u16(bytes, 6)?;
    if kind == 0 {
        return Err(Exp0002Error::InvalidObjectKind);
    }
    if read_u32(bytes, 8)? != 0 {
        return Err(Exp0002Error::InvalidFlags("object"));
    }
    let object_id = read_u64(bytes, 12)?;
    if object_id == 0 {
        return Err(Exp0002Error::InvalidObjectId);
    }
    let payload_len = read_u64(bytes, 20)?;
    let logical_len = read_u64(bytes, 28)?;
    if payload_len != logical_len {
        return Err(Exp0002Error::LogicalLengthMismatch);
    }
    require_zero(&bytes[36..48], "object")?;
    Ok(ParsedObjectHeader {
        object_id,
        kind,
        payload_len,
        logical_len,
    })
}

fn digest_bytes(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    hasher.finalize().into()
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
    use crate::exp0002::{build_append, build_genesis, ObjectInput};
    use crate::exp0002_source::Exp0002SliceSource;

    fn header() -> FileHeader {
        FileHeader {
            file_id: *b"source-strict-id",
            creation_nonce: *b"source-nonce-002",
        }
    }

    fn object(id: u64, payload: &[u8], is_root: bool) -> ObjectInput {
        ObjectInput {
            object_id: id,
            kind: 1,
            payload: payload.to_vec(),
            is_root,
        }
    }

    #[test]
    fn strict_source_validates_every_page_and_object() {
        let bytes = build_genesis(
            header(),
            (1..=500)
                .map(|id| object(id, &[u8::try_from(id % 251).expect("bounded")], id == 1))
                .collect(),
        )
        .expect("genesis");
        let mut source = Exp0002SliceSource::new(&bytes);
        let report = validate_strict_at(&mut source, &Exp0002SourceLimits::default())
            .expect("strict source validation");
        assert_eq!(report.objects.len(), 500);
        assert!(report.pages_verified >= 4);
        assert!(report.stats.pages_read >= 4);
        assert!(report.stats.bytes_hashed >= u64::try_from(bytes.len()).expect("length"));
    }

    #[test]
    fn strict_source_rejects_interrupted_append_and_recovery_finds_genesis() {
        let genesis = build_genesis(
            header(),
            vec![object(1, b"one", true), object(2, b"two", false)],
        )
        .expect("genesis");
        let appended = build_append(
            &genesis,
            vec![object(3, b"three", false)],
            vec![1, 3],
            &ValidationLimits::default(),
        )
        .expect("append");
        let interrupted = &appended[..appended.len() - FOOTER_LEN / 2];
        let mut strict_source = Exp0002SliceSource::new(interrupted);
        assert!(validate_strict_at(
            &mut strict_source,
            &Exp0002SourceLimits::default()
        )
        .is_err());

        let mut recovery_source = Exp0002SliceSource::new(interrupted);
        let report = scan_valid_prefixes_at(
            &mut recovery_source,
            &Exp0002RecoveryLimits::default(),
        )
        .expect("recovery");
        assert!(report
            .results
            .iter()
            .any(|candidate| candidate.prefix_len == u64::try_from(genesis.len()).expect("len")));
        assert!(report.results.iter().all(|candidate| candidate.sequence == 0));
    }

    #[test]
    fn recovery_magic_storm_is_bounded() {
        let mut bytes = build_genesis(header(), vec![object(1, b"root", true)])
            .expect("genesis");
        for _ in 0..32 {
            bytes.extend_from_slice(FOOTER_MAGIC);
        }
        let mut source = Exp0002SliceSource::new(&bytes);
        let error = scan_valid_prefixes_at(
            &mut source,
            &Exp0002RecoveryLimits {
                max_magic_matches: 4,
                ..Exp0002RecoveryLimits::default()
            },
        )
        .expect_err("storm must fail");
        assert_eq!(
            error,
            Exp0002SourceError::Format(Exp0002Error::ResourceLimit(
                "recovery magic matches"
            ))
        );
    }

    #[test]
    fn strict_source_enforces_cumulative_read_budget() {
        let bytes = build_genesis(header(), vec![object(1, &[7_u8; 1024], true)])
            .expect("genesis");
        let mut source = Exp0002SliceSource::new(&bytes);
        let error = validate_strict_at(
            &mut source,
            &Exp0002SourceLimits {
                max_source_bytes_read: 128,
                ..Exp0002SourceLimits::default()
            },
        )
        .expect_err("read budget");
        assert_eq!(
            error,
            Exp0002SourceError::Format(Exp0002Error::ResourceLimit(
                "source bytes read"
            ))
        );
    }
}
