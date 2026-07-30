//! Non-normative immutable-page successor byte experiment.
//!
//! This module has no compatibility promise and does not allocate a new UCOF
//! epoch. Strict validation is exact-end and never invokes recovery.

use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::error::Error;
use std::fmt;

pub const FILE_HEADER_LEN: usize = 64;
pub const OBJECT_HEADER_LEN: usize = 48;
pub const PAGE_SIZE: usize = 16 * 1024;
pub const PAGE_HEADER_LEN: usize = 64;
pub const LEAF_ENTRY_LEN: usize = 88;
pub const INTERNAL_ENTRY_LEN: usize = 64;
pub const SNAPSHOT_LEN: usize = 96;
pub const FOOTER_LEN: usize = 128;
pub const ABSENT_OFFSET: u64 = u64::MAX;
pub const LEAF_CAPACITY: usize = (PAGE_SIZE - PAGE_HEADER_LEN) / LEAF_ENTRY_LEN;
pub const INTERNAL_FANOUT: usize = (PAGE_SIZE - PAGE_HEADER_LEN) / INTERNAL_ENTRY_LEN;

const FILE_MAGIC: &[u8; 8] = b"UCOFIM02";
const OBJECT_MAGIC: &[u8; 8] = b"UCOBOBJ2";
const PAGE_MAGIC: &[u8; 8] = b"UCPGIM02";
const SNAPSHOT_MAGIC: &[u8; 8] = b"UCSNIM02";
const FOOTER_MAGIC: &[u8; 8] = b"UCFTIM02";

const OBJECT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-OBJECT\0";
const PAGE_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-PAGE\0";
const SNAPSHOT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-SNAPSHOT\0";
const COMMIT_DOMAIN: &[u8] = b"UCOF-IMMUTABLE-COMMIT\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableError {
    Invalid(&'static str),
    Limit(&'static str),
    MissingObject(u64),
    DuplicateObject(u64),
}

impl fmt::Display for ImmutableError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(label) => write!(formatter, "invalid {label}"),
            Self::Limit(label) => write!(formatter, "{label} limit exceeded"),
            Self::MissingObject(object_id) => write!(formatter, "missing object {object_id}"),
            Self::DuplicateObject(object_id) => write!(formatter, "duplicate object {object_id}"),
        }
    }
}

impl Error for ImmutableError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableLimits {
    pub max_file_bytes: usize,
    pub max_objects: usize,
    pub max_pages: usize,
    pub max_depth: u8,
    pub max_allocation_bytes: usize,
    pub max_output_bytes: usize,
}

impl Default for ImmutableLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 512 * 1024 * 1024,
            max_objects: 1_000_000,
            max_pages: 100_000,
            max_depth: 8,
            max_allocation_bytes: 128 * 1024 * 1024,
            max_output_bytes: 512 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableObjectInput {
    pub object_id: u64,
    pub kind: u16,
    pub payload: Vec<u8>,
}

impl ImmutableObjectInput {
    pub fn new(object_id: u64, kind: u16, payload: impl Into<Vec<u8>>) -> Self {
        Self {
            object_id,
            kind,
            payload: payload.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableReport {
    pub sequence: u64,
    pub object_count: usize,
    pub page_count: usize,
    pub root_level: u8,
    pub snapshot_digest: [u8; 32],
    pub commit_digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Locator {
    object_id: u64,
    kind: u16,
    record_offset: u64,
    record_len: u64,
    logical_len: u64,
    digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PageRef {
    minimum: u64,
    maximum: u64,
    offset: u64,
    level: u8,
    digest: [u8; 32],
}

#[derive(Clone, Debug)]
struct Footer {
    sequence: u64,
    snapshot_offset: u64,
    snapshot_len: u64,
    previous_footer_offset: u64,
    page_count_current: u64,
    snapshot_digest: [u8; 32],
    commit_digest: [u8; 32],
}

#[derive(Debug)]
struct InternalReport {
    public: ImmutableReport,
    locators: Vec<Locator>,
    footer_offset: usize,
}

fn checked_range<'a>(
    data: &'a [u8],
    offset: usize,
    length: usize,
    label: &'static str,
) -> Result<&'a [u8], ImmutableError> {
    let end = offset
        .checked_add(length)
        .ok_or(ImmutableError::Invalid(label))?;
    data.get(offset..end).ok_or(ImmutableError::Invalid(label))
}

fn array<const N: usize>(
    data: &[u8],
    offset: usize,
    label: &'static str,
) -> Result<[u8; N], ImmutableError> {
    checked_range(data, offset, N, label)?
        .try_into()
        .map_err(|_| ImmutableError::Invalid(label))
}

fn u16_at(data: &[u8], offset: usize, label: &'static str) -> Result<u16, ImmutableError> {
    Ok(u16::from_le_bytes(array(data, offset, label)?))
}

fn u32_at(data: &[u8], offset: usize, label: &'static str) -> Result<u32, ImmutableError> {
    Ok(u32::from_le_bytes(array(data, offset, label)?))
}

fn u64_at(data: &[u8], offset: usize, label: &'static str) -> Result<u64, ImmutableError> {
    Ok(u64::from_le_bytes(array(data, offset, label)?))
}

fn usize_from_u64(value: u64, label: &'static str) -> Result<usize, ImmutableError> {
    usize::try_from(value).map_err(|_| ImmutableError::Invalid(label))
}

fn usize_at(data: &[u8], offset: usize, label: &'static str) -> Result<usize, ImmutableError> {
    usize_from_u64(u64_at(data, offset, label)?, label)
}

fn u64_from_usize(value: usize) -> Result<u64, ImmutableError> {
    u64::try_from(value).map_err(|_| ImmutableError::Limit("integer conversion"))
}

fn u32_from_usize(value: usize) -> Result<u32, ImmutableError> {
    u32::try_from(value).map_err(|_| ImmutableError::Limit("integer conversion"))
}

fn put_u16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn digest(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn allocation_check<T>(count: usize, limits: ImmutableLimits) -> Result<(), ImmutableError> {
    let bytes = count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(ImmutableError::Limit("allocation"))?;
    if bytes > limits.max_allocation_bytes {
        return Err(ImmutableError::Limit("allocation"));
    }
    Ok(())
}

fn parse_footer(data: &[u8], offset: usize) -> Result<Footer, ImmutableError> {
    let raw = checked_range(data, offset, FOOTER_LEN, "footer")?;
    if &raw[..8] != FOOTER_MAGIC || raw[112..].iter().any(|byte| *byte != 0) {
        return Err(ImmutableError::Invalid("footer"));
    }
    Ok(Footer {
        sequence: u64_at(raw, 8, "footer")?,
        snapshot_offset: u64_at(raw, 16, "footer")?,
        snapshot_len: u64_at(raw, 24, "footer")?,
        previous_footer_offset: u64_at(raw, 32, "footer")?,
        page_count_current: u64_at(raw, 40, "footer")?,
        snapshot_digest: array(raw, 48, "footer")?,
        commit_digest: array(raw, 80, "footer")?,
    })
}

fn footer_semantics(footer: &Footer) -> Vec<u8> {
    let mut result = vec![0_u8; 72];
    put_u64(&mut result, 0, footer.sequence);
    put_u64(&mut result, 8, footer.snapshot_offset);
    put_u64(&mut result, 16, footer.snapshot_len);
    put_u64(&mut result, 24, footer.previous_footer_offset);
    put_u64(&mut result, 32, footer.page_count_current);
    result[40..].copy_from_slice(&footer.snapshot_digest);
    result
}

fn root_reference(
    data: &[u8],
    snapshot: &[u8],
    limits: ImmutableLimits,
) -> Result<PageRef, ImmutableError> {
    let root_offset = usize_at(snapshot, 16, "snapshot root")?;
    let root_level_u64 = u64_at(snapshot, 24, "snapshot root")?;
    let root_level =
        u8::try_from(root_level_u64).map_err(|_| ImmutableError::Invalid("snapshot root level"))?;
    if root_level > limits.max_depth {
        return Err(ImmutableError::Limit("page depth"));
    }
    let page = checked_range(data, root_offset, PAGE_SIZE, "root page")?;
    Ok(PageRef {
        minimum: u64_at(page, 20, "root page")?,
        maximum: u64_at(page, 28, "root page")?,
        offset: u64_from_usize(root_offset)?,
        level: root_level,
        digest: array(snapshot, 32, "snapshot root")?,
    })
}

// The traversal state remains explicit so every bounded collection is visible.
#[allow(clippy::too_many_arguments)]
fn parse_page(
    data: &[u8],
    reference: &PageRef,
    snapshot_offset: usize,
    limits: ImmutableLimits,
    seen: &mut HashSet<usize>,
    stack: &mut Vec<PageRef>,
    locators: &mut Vec<Locator>,
    structural_ranges: &mut Vec<(usize, usize)>,
) -> Result<(), ImmutableError> {
    let offset = usize_from_u64(reference.offset, "page offset")?;
    if offset < FILE_HEADER_LEN {
        return Err(ImmutableError::Invalid("page range"));
    }
    let end = offset
        .checked_add(PAGE_SIZE)
        .ok_or(ImmutableError::Invalid("page range"))?;
    if end > snapshot_offset {
        return Err(ImmutableError::Invalid("page range"));
    }
    if structural_ranges
        .iter()
        .any(|(start, stop)| offset < *stop && *start < end)
    {
        return Err(ImmutableError::Invalid("page overlap"));
    }
    if seen.len() >= limits.max_pages {
        return Err(ImmutableError::Limit("page count"));
    }
    if !seen.insert(offset) {
        return Err(ImmutableError::Invalid("page cycle"));
    }

    let page = checked_range(data, offset, PAGE_SIZE, "page")?;
    if digest(&[PAGE_DOMAIN, page]) != reference.digest {
        return Err(ImmutableError::Invalid("page digest"));
    }
    if &page[..8] != PAGE_MAGIC {
        return Err(ImmutableError::Invalid("page header"));
    }
    let kind = page[8];
    let level = page[9];
    let reserved = u16_at(page, 10, "page header")?;
    let count = usize::try_from(u32_at(page, 12, "page header")?)
        .map_err(|_| ImmutableError::Invalid("page count"))?;
    let entry_size = usize::try_from(u32_at(page, 16, "page header")?)
        .map_err(|_| ImmutableError::Invalid("page entry size"))?;
    let minimum = u64_at(page, 20, "page header")?;
    let maximum = u64_at(page, 28, "page header")?;
    if reserved != 0 || page[36..64].iter().any(|byte| *byte != 0) || count == 0 {
        return Err(ImmutableError::Invalid("page header"));
    }
    if level != reference.level || minimum != reference.minimum || maximum != reference.maximum {
        return Err(ImmutableError::Invalid("page reference"));
    }
    structural_ranges.push((offset, end));

    match kind {
        1 => {
            if level != 0 || entry_size != LEAF_ENTRY_LEN || count > LEAF_CAPACITY {
                return Err(ImmutableError::Invalid("leaf shape"));
            }
            if locators
                .len()
                .checked_add(count)
                .ok_or(ImmutableError::Limit("object count"))?
                > limits.max_objects
            {
                return Err(ImmutableError::Limit("object count"));
            }
            allocation_check::<Locator>(locators.len() + count, limits)?;
            let before = locators.len();
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                let object_id = u64_at(page, entry, "leaf entry")?;
                let kind = u16_at(page, entry + 8, "leaf entry")?;
                if object_id == 0
                    || kind == 0
                    || page[entry + 10..entry + 16].iter().any(|byte| *byte != 0)
                    || page[entry + 72..entry + 88].iter().any(|byte| *byte != 0)
                {
                    return Err(ImmutableError::Invalid("leaf entry"));
                }
                locators.push(Locator {
                    object_id,
                    kind,
                    record_offset: u64_at(page, entry + 16, "leaf entry")?,
                    record_len: u64_at(page, entry + 24, "leaf entry")?,
                    logical_len: u64_at(page, entry + 32, "leaf entry")?,
                    digest: array(page, entry + 40, "leaf entry")?,
                });
            }
            let added = &locators[before..];
            if added
                .windows(2)
                .any(|pair| pair[0].object_id >= pair[1].object_id)
                || added.first().map(|entry| entry.object_id) != Some(minimum)
                || added.last().map(|entry| entry.object_id) != Some(maximum)
            {
                return Err(ImmutableError::Invalid("leaf order"));
            }
            let used = PAGE_HEADER_LEN + count * LEAF_ENTRY_LEN;
            if page[used..].iter().any(|byte| *byte != 0) {
                return Err(ImmutableError::Invalid("leaf padding"));
            }
        }
        2 => {
            if level == 0 || entry_size != INTERNAL_ENTRY_LEN || count > INTERNAL_FANOUT {
                return Err(ImmutableError::Invalid("internal shape"));
            }
            if level > limits.max_depth {
                return Err(ImmutableError::Limit("page depth"));
            }
            allocation_check::<PageRef>(stack.len() + count, limits)?;
            let mut children = Vec::with_capacity(count);
            for index in 0..count {
                let entry = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                let child_minimum = u64_at(page, entry, "child entry")?;
                let child_maximum = u64_at(page, entry + 8, "child entry")?;
                let child_len = usize_at(page, entry + 24, "child entry")?;
                if child_minimum > child_maximum || child_len != PAGE_SIZE {
                    return Err(ImmutableError::Invalid("child entry"));
                }
                children.push(PageRef {
                    minimum: child_minimum,
                    maximum: child_maximum,
                    offset: u64_at(page, entry + 16, "child entry")?,
                    level: level - 1,
                    digest: array(page, entry + 32, "child entry")?,
                });
            }
            if children
                .windows(2)
                .any(|pair| pair[0].maximum >= pair[1].minimum)
                || children.first().map(|entry| entry.minimum) != Some(minimum)
                || children.last().map(|entry| entry.maximum) != Some(maximum)
            {
                return Err(ImmutableError::Invalid("child order"));
            }
            let used = PAGE_HEADER_LEN + count * INTERNAL_ENTRY_LEN;
            if page[used..].iter().any(|byte| *byte != 0) {
                return Err(ImmutableError::Invalid("internal padding"));
            }
            stack.extend(children.into_iter().rev());
        }
        _ => return Err(ImmutableError::Invalid("page kind")),
    }
    Ok(())
}

fn validate_internal(
    data: &[u8],
    limits: ImmutableLimits,
) -> Result<InternalReport, ImmutableError> {
    if data.len() > limits.max_file_bytes {
        return Err(ImmutableError::Limit("file size"));
    }
    if data.len() < FILE_HEADER_LEN + OBJECT_HEADER_LEN + PAGE_SIZE + SNAPSHOT_LEN + FOOTER_LEN {
        return Err(ImmutableError::Invalid("file length"));
    }
    if &data[..8] != FILE_MAGIC || data[8..FILE_HEADER_LEN].iter().any(|byte| *byte != 0) {
        return Err(ImmutableError::Invalid("header"));
    }

    let footer_offset = data.len() - FOOTER_LEN;
    let footer = parse_footer(data, footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot_len = usize_from_u64(footer.snapshot_len, "snapshot range")?;
    if snapshot_len != SNAPSHOT_LEN
        || snapshot_offset
            .checked_add(snapshot_len)
            .ok_or(ImmutableError::Invalid("snapshot range"))?
            != footer_offset
    {
        return Err(ImmutableError::Invalid("snapshot range"));
    }
    let snapshot = checked_range(data, snapshot_offset, snapshot_len, "snapshot")?;
    if digest(&[SNAPSHOT_DOMAIN, snapshot]) != footer.snapshot_digest {
        return Err(ImmutableError::Invalid("snapshot digest"));
    }
    if &snapshot[..8] != SNAPSHOT_MAGIC || u64_at(snapshot, 8, "snapshot")? != footer.sequence {
        return Err(ImmutableError::Invalid("snapshot"));
    }
    let parent_snapshot_digest = array::<32>(snapshot, 64, "snapshot parent")?;
    let commit_start = if footer.previous_footer_offset == ABSENT_OFFSET {
        if footer.sequence != 0 || parent_snapshot_digest.iter().any(|byte| *byte != 0) {
            return Err(ImmutableError::Invalid("genesis linkage"));
        }
        0
    } else {
        let previous_offset = usize_from_u64(footer.previous_footer_offset, "previous footer")?;
        let previous_end = previous_offset
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableError::Invalid("previous footer"))?;
        if previous_end > snapshot_offset {
            return Err(ImmutableError::Invalid("previous footer"));
        }
        let previous = parse_footer(data, previous_offset)?;
        if footer.sequence != previous.sequence + 1
            || previous.snapshot_digest != parent_snapshot_digest
        {
            return Err(ImmutableError::Invalid("parent linkage"));
        }
        previous_end
    };
    let semantics = footer_semantics(&footer);
    if digest(&[
        COMMIT_DOMAIN,
        &data[commit_start..footer_offset],
        &semantics,
    ]) != footer.commit_digest
    {
        return Err(ImmutableError::Invalid("commit digest"));
    }

    let root = root_reference(data, snapshot, limits)?;
    let mut seen = HashSet::new();
    let mut stack = vec![root.clone()];
    let mut locators = Vec::new();
    let mut structural_ranges = vec![
        (snapshot_offset, footer_offset),
        (footer_offset, data.len()),
    ];
    while let Some(reference) = stack.pop() {
        parse_page(
            data,
            &reference,
            snapshot_offset,
            limits,
            &mut seen,
            &mut stack,
            &mut locators,
            &mut structural_ranges,
        )?;
    }
    let current_pages = seen
        .iter()
        .filter(|offset| **offset >= commit_start)
        .count();
    if footer.page_count_current != u64_from_usize(current_pages)? {
        return Err(ImmutableError::Invalid("page count"));
    }
    if locators.is_empty()
        || locators
            .windows(2)
            .any(|pair| pair[0].object_id >= pair[1].object_id)
        || locators.first().map(|entry| entry.object_id) != Some(root.minimum)
        || locators.last().map(|entry| entry.object_id) != Some(root.maximum)
    {
        return Err(ImmutableError::Invalid("object order"));
    }

    let mut object_ranges = Vec::with_capacity(locators.len());
    allocation_check::<(usize, usize)>(locators.len(), limits)?;
    for locator in &locators {
        let offset = usize_from_u64(locator.record_offset, "object range")?;
        let length = usize_from_u64(locator.record_len, "object range")?;
        let end = offset
            .checked_add(length)
            .ok_or(ImmutableError::Invalid("object range"))?;
        if offset < FILE_HEADER_LEN || end > snapshot_offset {
            return Err(ImmutableError::Invalid("object range"));
        }
        if structural_ranges
            .iter()
            .any(|(start, stop)| offset < *stop && *start < end)
        {
            return Err(ImmutableError::Invalid("object structural overlap"));
        }
        let record = checked_range(data, offset, length, "object")?;
        if length < OBJECT_HEADER_LEN
            || &record[..8] != OBJECT_MAGIC
            || usize::from(u16_at(record, 8, "object header")?) != OBJECT_HEADER_LEN
            || u32_at(record, 12, "object header")? != 0
            || record[40..OBJECT_HEADER_LEN].iter().any(|byte| *byte != 0)
        {
            return Err(ImmutableError::Invalid("object header"));
        }
        let kind = u16_at(record, 10, "object header")?;
        let object_id = u64_at(record, 16, "object header")?;
        let payload_len = usize_at(record, 24, "object length")?;
        let logical_len = u64_at(record, 32, "object length")?;
        if kind == 0
            || object_id == 0
            || OBJECT_HEADER_LEN
                .checked_add(payload_len)
                .ok_or(ImmutableError::Invalid("object length"))?
                != length
            || u64_from_usize(payload_len)? != logical_len
        {
            return Err(ImmutableError::Invalid("object length"));
        }
        if object_id != locator.object_id
            || kind != locator.kind
            || logical_len != locator.logical_len
        {
            return Err(ImmutableError::Invalid("object locator"));
        }
        if digest(&[OBJECT_DOMAIN, record]) != locator.digest {
            return Err(ImmutableError::Invalid("object digest"));
        }
        object_ranges.push((offset, end));
    }
    object_ranges.sort_unstable();
    if object_ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(ImmutableError::Invalid("object overlap"));
    }

    Ok(InternalReport {
        public: ImmutableReport {
            sequence: footer.sequence,
            object_count: locators.len(),
            page_count: seen.len(),
            root_level: root.level,
            snapshot_digest: footer.snapshot_digest,
            commit_digest: footer.commit_digest,
        },
        locators,
        footer_offset,
    })
}

pub fn validate(data: &[u8], limits: ImmutableLimits) -> Result<ImmutableReport, ImmutableError> {
    Ok(validate_internal(data, limits)?.public)
}

fn encode_object(input: &ImmutableObjectInput) -> Result<Vec<u8>, ImmutableError> {
    if input.object_id == 0 || input.kind == 0 {
        return Err(ImmutableError::Invalid("object input"));
    }
    let length = OBJECT_HEADER_LEN
        .checked_add(input.payload.len())
        .ok_or(ImmutableError::Limit("object size"))?;
    let mut record = vec![0_u8; length];
    record[..8].copy_from_slice(OBJECT_MAGIC);
    put_u16(
        &mut record,
        8,
        u16::try_from(OBJECT_HEADER_LEN).map_err(|_| ImmutableError::Limit("object header"))?,
    );
    put_u16(&mut record, 10, input.kind);
    put_u64(&mut record, 16, input.object_id);
    put_u64(&mut record, 24, u64_from_usize(input.payload.len())?);
    put_u64(&mut record, 32, u64_from_usize(input.payload.len())?);
    record[OBJECT_HEADER_LEN..].copy_from_slice(&input.payload);
    Ok(record)
}

fn append_object(
    output: &mut Vec<u8>,
    input: &ImmutableObjectInput,
    limits: ImmutableLimits,
) -> Result<Locator, ImmutableError> {
    let record = encode_object(input)?;
    if output
        .len()
        .checked_add(record.len())
        .ok_or(ImmutableError::Limit("output"))?
        > limits.max_output_bytes
    {
        return Err(ImmutableError::Limit("output"));
    }
    let offset = u64_from_usize(output.len())?;
    output.extend_from_slice(&record);
    Ok(Locator {
        object_id: input.object_id,
        kind: input.kind,
        record_offset: offset,
        record_len: u64_from_usize(record.len())?,
        logical_len: u64_from_usize(input.payload.len())?,
        digest: digest(&[OBJECT_DOMAIN, &record]),
    })
}

fn encode_leaf(entries: &[Locator]) -> Result<Vec<u8>, ImmutableError> {
    if entries.is_empty() || entries.len() > LEAF_CAPACITY {
        return Err(ImmutableError::Invalid("leaf input"));
    }
    if entries
        .windows(2)
        .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(ImmutableError::Invalid("leaf input order"));
    }
    let mut page = vec![0_u8; PAGE_SIZE];
    page[..8].copy_from_slice(PAGE_MAGIC);
    page[8] = 1;
    put_u32(&mut page, 12, u32_from_usize(entries.len())?);
    put_u32(&mut page, 16, u32_from_usize(LEAF_ENTRY_LEN)?);
    put_u64(&mut page, 20, entries[0].object_id);
    put_u64(
        &mut page,
        28,
        entries
            .last()
            .ok_or(ImmutableError::Invalid("leaf input"))?
            .object_id,
    );
    for (index, entry) in entries.iter().enumerate() {
        let offset = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
        put_u64(&mut page, offset, entry.object_id);
        put_u16(&mut page, offset + 8, entry.kind);
        put_u64(&mut page, offset + 16, entry.record_offset);
        put_u64(&mut page, offset + 24, entry.record_len);
        put_u64(&mut page, offset + 32, entry.logical_len);
        page[offset + 40..offset + 72].copy_from_slice(&entry.digest);
    }
    Ok(page)
}

fn encode_internal(children: &[PageRef], level: u8) -> Result<Vec<u8>, ImmutableError> {
    if children.is_empty() || children.len() > INTERNAL_FANOUT || level == 0 {
        return Err(ImmutableError::Invalid("internal input"));
    }
    if children
        .windows(2)
        .any(|pair| pair[0].maximum >= pair[1].minimum)
        || children.iter().any(|child| child.level + 1 != level)
    {
        return Err(ImmutableError::Invalid("internal input order"));
    }
    let mut page = vec![0_u8; PAGE_SIZE];
    page[..8].copy_from_slice(PAGE_MAGIC);
    page[8] = 2;
    page[9] = level;
    put_u32(&mut page, 12, u32_from_usize(children.len())?);
    put_u32(&mut page, 16, u32_from_usize(INTERNAL_ENTRY_LEN)?);
    put_u64(&mut page, 20, children[0].minimum);
    put_u64(
        &mut page,
        28,
        children
            .last()
            .ok_or(ImmutableError::Invalid("internal input"))?
            .maximum,
    );
    for (index, child) in children.iter().enumerate() {
        let offset = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
        put_u64(&mut page, offset, child.minimum);
        put_u64(&mut page, offset + 8, child.maximum);
        put_u64(&mut page, offset + 16, child.offset);
        put_u64(&mut page, offset + 24, u64_from_usize(PAGE_SIZE)?);
        page[offset + 32..offset + 64].copy_from_slice(&child.digest);
    }
    Ok(page)
}

fn append_page(
    output: &mut Vec<u8>,
    page: &[u8],
    limits: ImmutableLimits,
) -> Result<PageRef, ImmutableError> {
    if output
        .len()
        .checked_add(PAGE_SIZE)
        .ok_or(ImmutableError::Limit("output"))?
        > limits.max_output_bytes
    {
        return Err(ImmutableError::Limit("output"));
    }
    let reference = PageRef {
        minimum: u64_at(page, 20, "page")?,
        maximum: u64_at(page, 28, "page")?,
        offset: u64_from_usize(output.len())?,
        level: page[9],
        digest: digest(&[PAGE_DOMAIN, page]),
    };
    output.extend_from_slice(page);
    Ok(reference)
}

fn build_tree(
    output: &mut Vec<u8>,
    locators: &mut [Locator],
    limits: ImmutableLimits,
) -> Result<(PageRef, usize), ImmutableError> {
    if locators.is_empty() || locators.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count"));
    }
    locators.sort_by_key(|locator| locator.object_id);
    if locators
        .windows(2)
        .any(|pair| pair[0].object_id == pair[1].object_id)
    {
        return Err(ImmutableError::DuplicateObject(
            locators
                .windows(2)
                .find(|pair| pair[0].object_id == pair[1].object_id)
                .map(|pair| pair[0].object_id)
                .unwrap_or(0),
        ));
    }
    let mut pages = 0_usize;
    let mut level = Vec::new();
    for chunk in locators.chunks(LEAF_CAPACITY) {
        if pages >= limits.max_pages {
            return Err(ImmutableError::Limit("page count"));
        }
        level.push(append_page(output, &encode_leaf(chunk)?, limits)?);
        pages += 1;
    }
    while level.len() > 1 {
        let parent_level = level[0]
            .level
            .checked_add(1)
            .ok_or(ImmutableError::Limit("page depth"))?;
        if parent_level > limits.max_depth {
            return Err(ImmutableError::Limit("page depth"));
        }
        let mut next = Vec::new();
        for chunk in level.chunks(INTERNAL_FANOUT) {
            if pages >= limits.max_pages {
                return Err(ImmutableError::Limit("page count"));
            }
            next.push(append_page(
                output,
                &encode_internal(chunk, parent_level)?,
                limits,
            )?);
            pages += 1;
        }
        level = next;
    }
    Ok((
        level.pop().ok_or(ImmutableError::Invalid("empty tree"))?,
        pages,
    ))
}

fn publish(
    output: &mut Vec<u8>,
    sequence: u64,
    root: &PageRef,
    parent_snapshot_digest: [u8; 32],
    previous_footer_offset: u64,
    page_count: usize,
    limits: ImmutableLimits,
) -> Result<(), ImmutableError> {
    let required = SNAPSHOT_LEN
        .checked_add(FOOTER_LEN)
        .and_then(|value| output.len().checked_add(value))
        .ok_or(ImmutableError::Limit("output"))?;
    if required > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
    let snapshot_offset = u64_from_usize(output.len())?;
    let mut snapshot = vec![0_u8; SNAPSHOT_LEN];
    snapshot[..8].copy_from_slice(SNAPSHOT_MAGIC);
    put_u64(&mut snapshot, 8, sequence);
    put_u64(&mut snapshot, 16, root.offset);
    put_u64(&mut snapshot, 24, u64::from(root.level));
    snapshot[32..64].copy_from_slice(&root.digest);
    snapshot[64..].copy_from_slice(&parent_snapshot_digest);
    let snapshot_digest = digest(&[SNAPSHOT_DOMAIN, &snapshot]);
    output.extend_from_slice(&snapshot);

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
    let commit_start = if previous_footer_offset == ABSENT_OFFSET {
        0
    } else {
        usize_from_u64(previous_footer_offset, "previous footer")?
            .checked_add(FOOTER_LEN)
            .ok_or(ImmutableError::Invalid("previous footer"))?
    };
    let commit_digest = digest(&[COMMIT_DOMAIN, &output[commit_start..], &semantics]);

    let mut raw = vec![0_u8; FOOTER_LEN];
    raw[..8].copy_from_slice(FOOTER_MAGIC);
    put_u64(&mut raw, 8, sequence);
    put_u64(&mut raw, 16, snapshot_offset);
    put_u64(&mut raw, 24, u64_from_usize(SNAPSHOT_LEN)?);
    put_u64(&mut raw, 32, previous_footer_offset);
    put_u64(&mut raw, 40, u64_from_usize(page_count)?);
    raw[48..80].copy_from_slice(&snapshot_digest);
    raw[80..112].copy_from_slice(&commit_digest);
    output.extend_from_slice(&raw);
    Ok(())
}

pub fn build_genesis(
    inputs: &[ImmutableObjectInput],
    limits: ImmutableLimits,
) -> Result<Vec<u8>, ImmutableError> {
    if inputs.is_empty() || inputs.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count"));
    }
    allocation_check::<Locator>(inputs.len(), limits)?;
    let mut ordered = inputs.to_vec();
    ordered.sort_by_key(|input| input.object_id);
    if let Some(pair) = ordered
        .windows(2)
        .find(|pair| pair[0].object_id == pair[1].object_id)
    {
        return Err(ImmutableError::DuplicateObject(pair[0].object_id));
    }

    let mut output = vec![0_u8; FILE_HEADER_LEN];
    output[..8].copy_from_slice(FILE_MAGIC);
    let mut locators = Vec::with_capacity(ordered.len());
    for input in &ordered {
        locators.push(append_object(&mut output, input, limits)?);
    }
    let (root, pages) = build_tree(&mut output, &mut locators, limits)?;
    publish(
        &mut output,
        0,
        &root,
        [0_u8; 32],
        ABSENT_OFFSET,
        pages,
        limits,
    )?;
    validate(&output, limits)?;
    Ok(output)
}

pub fn append_replacement(
    data: &[u8],
    replacement: &ImmutableObjectInput,
    limits: ImmutableLimits,
) -> Result<Vec<u8>, ImmutableError> {
    let previous = validate_internal(data, limits)?;
    let index = previous
        .locators
        .iter()
        .position(|locator| locator.object_id == replacement.object_id)
        .ok_or(ImmutableError::MissingObject(replacement.object_id))?;
    let mut output = data.to_vec();
    let locator = append_object(&mut output, replacement, limits)?;
    let mut locators = previous.locators;
    locators[index] = locator;
    let (root, pages) = build_tree(&mut output, &mut locators, limits)?;
    publish(
        &mut output,
        previous.public.sequence + 1,
        &root,
        previous.public.snapshot_digest,
        u64_from_usize(previous.footer_offset)?,
        pages,
        limits,
    )?;
    validate(&output, limits)?;
    Ok(output)
}
