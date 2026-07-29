//! Authenticated single-object lookup for the provisional EXP-0002 candidate.
//!
//! This API verifies the active commit, snapshot, one root-to-leaf directory
//! path, and the selected object record. It does not claim that unrelated
//! historical object records have been rehashed.

use crate::exp0002::{
    Exp0002Error, FileHeader, Snapshot, ValidationLimits, ABSENT_OFFSET, FOOTER_LEN,
    INTERNAL_ENTRY_LEN, LEAF_ENTRY_LEN, OBJECT_HEADER_LEN, PAGE_HEADER_LEN, PAGE_SIZE,
};
use sha2::{Digest, Sha256};

const PAGE_DOMAIN: &[u8] = b"UCOF-EXP-0002-PAGE\0";
const OBJECT_DOMAIN: &[u8] = b"UCOF-EXP-0002-OBJECT\0";
const SNAPSHOT_DOMAIN: &[u8] = b"UCOF-EXP-0002-SNAPSHOT\0";
const COMMIT_DOMAIN: &[u8] = b"UCOF-EXP-0002-COMMIT\0";
const FOOTER_MAGIC: &[u8; 8] = b"UCOF2END";
const PAGE_MAGIC: &[u8; 4] = b"PG02";
const OBJECT_MAGIC: &[u8; 4] = b"OBJ2";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedLookupLimits {
    pub validation: ValidationLimits,
    pub max_page_reads: usize,
}

impl Default for AuthenticatedLookupLimits {
    fn default() -> Self {
        Self {
            validation: ValidationLimits::default(),
            max_page_reads: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedObjectLookup {
    pub object_id: u64,
    pub kind: u16,
    pub record_offset: u64,
    pub record_len: u64,
    pub payload_offset: u64,
    pub payload_len: u64,
    pub logical_len: u64,
    pub sequence: u64,
    pub pages_read: usize,
    pub bytes_hashed: u64,
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
struct SelectedLeaf {
    object_id: u64,
    kind: u16,
    record_offset: u64,
    record_len: u64,
    logical_len: u64,
    digest: [u8; 32],
}

pub fn lookup_authenticated(
    bytes: &[u8],
    object_id: u64,
    limits: &AuthenticatedLookupLimits,
) -> Result<Option<AuthenticatedObjectLookup>, Exp0002Error> {
    if object_id == 0 {
        return Err(Exp0002Error::InvalidObjectId);
    }
    let file_len = to_u64(bytes.len())?;
    if file_len > limits.validation.max_file_bytes {
        return Err(Exp0002Error::ResourceLimit("file bytes"));
    }
    if bytes.len() < 64 + FOOTER_LEN {
        return Err(Exp0002Error::Truncated);
    }
    FileHeader::parse(bytes)?;

    let footer_offset = bytes
        .len()
        .checked_sub(FOOTER_LEN)
        .ok_or(Exp0002Error::Truncated)?;
    let footer = &bytes[footer_offset..];
    validate_footer_structure(footer)?;
    let commit_start = read_u64(footer, 16)?;
    let commit_len = read_u64(footer, 24)?;
    let snapshot_offset = read_u64(footer, 32)?;
    let snapshot_len = read_u64(footer, 40)?;
    let sequence = read_u64(footer, 48)?;
    let previous_footer_offset = read_u64(footer, 56)?;
    let snapshot_digest: [u8; 32] = read_array(footer, 80)?;
    let commit_digest: [u8; 32] = read_array(footer, 112)?;

    if commit_start
        .checked_add(commit_len)
        .ok_or(Exp0002Error::ArithmeticOverflow)?
        != to_u64(footer_offset)?
    {
        return Err(Exp0002Error::InvalidCommitRange);
    }
    if commit_len > limits.validation.max_commit_bytes {
        return Err(Exp0002Error::ResourceLimit("commit bytes"));
    }
    let mut bytes_hashed = commit_len;
    check_hashed(bytes_hashed, limits)?;
    let commit_range = checked_range(commit_start, commit_len, bytes.len())?;
    let mut commit_hasher = Sha256::new();
    commit_hasher.update(COMMIT_DOMAIN);
    commit_hasher.update(&bytes[commit_range]);
    commit_hasher.update(&footer[8..112]);
    let actual_commit: [u8; 32] = commit_hasher.finalize().into();
    if actual_commit != commit_digest {
        return Err(Exp0002Error::DigestMismatch("commit"));
    }

    let snapshot_range = checked_range(snapshot_offset, snapshot_len, bytes.len())?;
    if snapshot_offset < commit_start || snapshot_range.end > footer_offset {
        return Err(Exp0002Error::InvalidCommitRange);
    }
    let snapshot_bytes = &bytes[snapshot_range];
    if digest(SNAPSHOT_DOMAIN, snapshot_bytes) != snapshot_digest {
        return Err(Exp0002Error::DigestMismatch("snapshot"));
    }
    bytes_hashed = bytes_hashed
        .checked_add(snapshot_len)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    check_hashed(bytes_hashed, limits)?;
    let snapshot = Snapshot::parse(snapshot_bytes, &limits.validation)?;
    if snapshot.sequence != sequence
        || snapshot.previous_footer_offset != previous_footer_offset
    {
        return Err(Exp0002Error::InvalidSnapshotSequence);
    }
    validate_parent_link(
        bytes,
        footer_offset,
        commit_start,
        sequence,
        previous_footer_offset,
        snapshot.parent_snapshot_digest,
    )?;

    let mut expected = ExpectedPage {
        offset: snapshot.directory_root_offset,
        digest: snapshot.directory_root_digest,
        level: snapshot.directory_root_level,
        minimum: None,
        maximum: None,
    };
    let mut pages_read = 0_usize;
    loop {
        pages_read = pages_read
            .checked_add(1)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if pages_read > limits.max_page_reads
            || pages_read > limits.validation.max_page_depth
            || pages_read > limits.validation.max_pages
        {
            return Err(Exp0002Error::ResourceLimit("lookup pages"));
        }
        let page_range = checked_range(expected.offset, PAGE_SIZE as u64, bytes.len())?;
        let page = &bytes[page_range];
        bytes_hashed = bytes_hashed
            .checked_add(PAGE_SIZE as u64)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        check_hashed(bytes_hashed, limits)?;
        if digest(PAGE_DOMAIN, page) != expected.digest {
            return Err(Exp0002Error::DigestMismatch("page"));
        }
        let header = parse_page_header(page, sequence)?;
        if u16::from(header.level) != expected.level
            || expected.minimum.is_some_and(|value| value != header.minimum)
            || expected.maximum.is_some_and(|value| value != header.maximum)
        {
            return Err(Exp0002Error::InvalidPageReference);
        }
        if object_id < header.minimum || object_id > header.maximum {
            return Ok(None);
        }
        match header.kind {
            1 => {
                let selected = select_leaf(page, &header, object_id)?;
                let Some(selected) = selected else {
                    return Ok(None);
                };
                let record_range = checked_range(
                    selected.record_offset,
                    selected.record_len,
                    bytes.len(),
                )?;
                let record = &bytes[record_range];
                bytes_hashed = bytes_hashed
                    .checked_add(selected.record_len)
                    .ok_or(Exp0002Error::ArithmeticOverflow)?;
                check_hashed(bytes_hashed, limits)?;
                validate_selected_object(record, &selected, &limits.validation)?;
                return Ok(Some(AuthenticatedObjectLookup {
                    object_id: selected.object_id,
                    kind: selected.kind,
                    record_offset: selected.record_offset,
                    record_len: selected.record_len,
                    payload_offset: selected
                        .record_offset
                        .checked_add(OBJECT_HEADER_LEN as u64)
                        .ok_or(Exp0002Error::ArithmeticOverflow)?,
                    payload_len: selected
                        .record_len
                        .checked_sub(OBJECT_HEADER_LEN as u64)
                        .ok_or(Exp0002Error::InvalidLength("object record"))?,
                    logical_len: selected.logical_len,
                    sequence,
                    pages_read,
                    bytes_hashed,
                }));
            }
            2 => {
                let Some(child) = select_internal(page, &header, object_id)? else {
                    return Ok(None);
                };
                expected = child;
            }
            _ => return Err(Exp0002Error::InvalidPageKind),
        }
    }
}

#[derive(Clone)]
struct LookupPageHeader {
    kind: u8,
    level: u8,
    count: usize,
    entry_size: usize,
    minimum: u64,
    maximum: u64,
}

fn parse_page_header(
    page: &[u8],
    sequence: u64,
) -> Result<LookupPageHeader, Exp0002Error> {
    if page.len() != PAGE_SIZE || &page[0..4] != PAGE_MAGIC {
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
    Ok(LookupPageHeader {
        kind,
        level,
        count,
        entry_size,
        minimum,
        maximum,
    })
}

fn select_leaf(
    page: &[u8],
    header: &LookupPageHeader,
    object_id: u64,
) -> Result<Option<SelectedLeaf>, Exp0002Error> {
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
        if read_u16(entry, 8)? == 0
            || read_u16(entry, 10)? != 0
            || read_u32(entry, 12)? != 0
        {
            return Err(Exp0002Error::InvalidFlags("leaf entry"));
        }
        require_zero(&entry[72..88], "leaf entry")?;
        if key == object_id {
            selected = Some(SelectedLeaf {
                object_id: key,
                kind: read_u16(entry, 8)?,
                record_offset: read_u64(entry, 16)?,
                record_len: read_u64(entry, 24)?,
                logical_len: read_u64(entry, 32)?,
                digest: read_array(entry, 40)?,
            });
        }
        previous = Some(key);
    }
    if read_u64(&page[PAGE_HEADER_LEN..], 0)? != header.minimum {
        return Err(Exp0002Error::InvalidPageRange);
    }
    let last = PAGE_HEADER_LEN + (header.count - 1) * LEAF_ENTRY_LEN;
    if read_u64(page, last)? != header.maximum {
        return Err(Exp0002Error::InvalidPageRange);
    }
    Ok(selected)
}

fn select_internal(
    page: &[u8],
    header: &LookupPageHeader,
    object_id: u64,
) -> Result<Option<ExpectedPage>, Exp0002Error> {
    let capacity = (PAGE_SIZE - PAGE_HEADER_LEN) / INTERNAL_ENTRY_LEN;
    if header.level == 0
        || header.entry_size != INTERNAL_ENTRY_LEN
        || header.count > capacity
    {
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
        if minimum == 0
            || minimum > maximum
            || previous_max.is_some_and(|value| value >= minimum)
        {
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

fn validate_selected_object(
    record: &[u8],
    selected: &SelectedLeaf,
    limits: &ValidationLimits,
) -> Result<(), Exp0002Error> {
    if record.len() < OBJECT_HEADER_LEN || &record[0..4] != OBJECT_MAGIC {
        return Err(Exp0002Error::InvalidMagic("object"));
    }
    if usize::from(read_u16(record, 4)?) != OBJECT_HEADER_LEN
        || read_u16(record, 6)? != selected.kind
        || read_u32(record, 8)? != 0
        || read_u64(record, 12)? != selected.object_id
    {
        return Err(Exp0002Error::InvalidLength("object header"));
    }
    let payload_len = read_u64(record, 20)?;
    let logical_len = read_u64(record, 28)?;
    if payload_len != logical_len
        || logical_len != selected.logical_len
        || payload_len > limits.max_payload_bytes
        || (OBJECT_HEADER_LEN as u64)
            .checked_add(payload_len)
            .ok_or(Exp0002Error::ArithmeticOverflow)?
            != selected.record_len
    {
        return Err(Exp0002Error::LogicalLengthMismatch);
    }
    require_zero(&record[36..48], "object")?;
    if digest(OBJECT_DOMAIN, record) != selected.digest {
        return Err(Exp0002Error::DigestMismatch("object"));
    }
    Ok(())
}

fn validate_footer_structure(footer: &[u8]) -> Result<(), Exp0002Error> {
    if footer.len() != FOOTER_LEN || &footer[0..8] != FOOTER_MAGIC {
        return Err(Exp0002Error::InvalidMagic("footer"));
    }
    if usize::from(read_u16(footer, 8)?) != FOOTER_LEN || read_u16(footer, 10)? != 2 {
        return Err(Exp0002Error::InvalidVersion);
    }
    if read_u32(footer, 12)? != 0 || read_u16(footer, 72)? != 1 {
        return Err(Exp0002Error::InvalidFlags("footer"));
    }
    require_zero(&footer[74..80], "footer")?;
    require_zero(&footer[144..160], "footer")?;
    Ok(())
}

fn validate_parent_link(
    bytes: &[u8],
    footer_offset: usize,
    commit_start: u64,
    sequence: u64,
    previous_footer_offset: u64,
    parent_snapshot_digest: [u8; 32],
) -> Result<(), Exp0002Error> {
    if previous_footer_offset == ABSENT_OFFSET {
        if sequence != 0 || commit_start != 0 || parent_snapshot_digest != [0_u8; 32] {
            return Err(Exp0002Error::InvalidParent);
        }
        return Ok(());
    }
    if previous_footer_offset >= to_u64(footer_offset)?
        || commit_start
            != previous_footer_offset
                .checked_add(FOOTER_LEN as u64)
                .ok_or(Exp0002Error::ArithmeticOverflow)?
    {
        return Err(Exp0002Error::InvalidPreviousFooter);
    }
    let previous_range = checked_range(previous_footer_offset, FOOTER_LEN as u64, bytes.len())?;
    let previous = &bytes[previous_range];
    validate_footer_structure(previous)?;
    if read_u64(previous, 48)?
        .checked_add(1)
        .ok_or(Exp0002Error::ArithmeticOverflow)?
        != sequence
        || read_array::<32>(previous, 80)? != parent_snapshot_digest
    {
        return Err(Exp0002Error::InvalidParent);
    }
    Ok(())
}

fn check_hashed(
    bytes_hashed: u64,
    limits: &AuthenticatedLookupLimits,
) -> Result<(), Exp0002Error> {
    if bytes_hashed > limits.validation.max_hashed_bytes {
        Err(Exp0002Error::ResourceLimit("hashed bytes"))
    } else {
        Ok(())
    }
}

fn digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    hasher.finalize().into()
}

fn checked_range(
    offset: u64,
    length: u64,
    total: usize,
) -> Result<std::ops::Range<usize>, Exp0002Error> {
    let end = offset
        .checked_add(length)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    let start = usize::try_from(offset).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
    let end = usize::try_from(end).map_err(|_| Exp0002Error::ArithmeticOverflow)?;
    if start > end || end > total {
        return Err(Exp0002Error::Truncated);
    }
    Ok(start..end)
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

fn to_u64(value: usize) -> Result<u64, Exp0002Error> {
    u64::try_from(value).map_err(|_| Exp0002Error::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exp0002::{build_append, build_genesis, FileHeader, ObjectInput};

    fn header() -> FileHeader {
        FileHeader {
            file_id: *b"exp0002-file-id!",
            creation_nonce: *b"fixed-nonce-0002",
        }
    }

    fn object(id: u64, payload: &[u8], root: bool) -> ObjectInput {
        ObjectInput {
            object_id: id,
            kind: 1,
            payload: payload.to_vec(),
            is_root: root,
        }
    }

    #[test]
    fn finds_selected_object_through_multi_leaf_tree() {
        let objects = (1_u64..=500)
            .map(|id| object(id, &[u8::try_from(id % 251).expect("byte")], id == 1))
            .collect();
        let bytes = build_genesis(header(), objects).expect("genesis");
        let result = lookup_authenticated(&bytes, 499, &AuthenticatedLookupLimits::default())
            .expect("lookup")
            .expect("object");
        assert_eq!(result.object_id, 499);
        assert!(result.pages_read <= 2);
        assert_eq!(result.payload_len, 1);
    }

    #[test]
    fn proves_absence_on_authenticated_path() {
        let bytes = build_genesis(
            header(),
            vec![object(1, b"one", true), object(3, b"three", false)],
        )
        .expect("genesis");
        assert_eq!(
            lookup_authenticated(&bytes, 2, &AuthenticatedLookupLimits::default())
                .expect("lookup"),
            None
        );
    }

    #[test]
    fn selected_old_object_is_rehashed_after_append() {
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
        let mut damaged = appended.clone();
        damaged[64 + OBJECT_HEADER_LEN] ^= 1;
        assert!(lookup_authenticated(
            &damaged,
            1,
            &AuthenticatedLookupLimits::default()
        )
        .is_err());
    }

    #[test]
    fn page_read_limit_fails_before_path_completion() {
        let objects = (1_u64..=500)
            .map(|id| object(id, b"x", id == 1))
            .collect();
        let bytes = build_genesis(header(), objects).expect("genesis");
        assert_eq!(
            lookup_authenticated(
                &bytes,
                499,
                &AuthenticatedLookupLimits {
                    max_page_reads: 1,
                    ..AuthenticatedLookupLimits::default()
                }
            ),
            Err(Exp0002Error::ResourceLimit("lookup pages"))
        );
    }
}
