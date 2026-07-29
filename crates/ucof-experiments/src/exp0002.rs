//! Executable byte candidate for `UCOF-EXP-0002`.
//!
//! This module is deliberately isolated in the unpublished experiments crate.
//! Its bytes are disposable and must not be treated as a stable format.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const FILE_HEADER_LEN: usize = 64;
pub const OBJECT_HEADER_LEN: usize = 48;
pub const PAGE_HEADER_LEN: usize = 64;
pub const PAGE_SIZE: usize = 16 * 1024;
pub const LEAF_ENTRY_LEN: usize = 88;
pub const INTERNAL_ENTRY_LEN: usize = 64;
pub const LEAF_CAPACITY: usize = (PAGE_SIZE - PAGE_HEADER_LEN) / LEAF_ENTRY_LEN;
pub const INTERNAL_CAPACITY: usize = (PAGE_SIZE - PAGE_HEADER_LEN) / INTERNAL_ENTRY_LEN;
pub const SNAPSHOT_HEADER_LEN: usize = 160;
pub const FOOTER_LEN: usize = 160;
pub const ABSENT_OFFSET: u64 = u64::MAX;
pub const DIGEST_SHA256: u16 = 1;

const FILE_MAGIC: [u8; 8] = *b"UCOF2\r\n\x1a";
const OBJECT_MAGIC: [u8; 4] = *b"OBJ2";
const PAGE_MAGIC: [u8; 4] = *b"PG02";
const SNAPSHOT_MAGIC: [u8; 4] = *b"SNP2";
const FOOTER_MAGIC: [u8; 8] = *b"UCOF2END";

const OBJECT_DOMAIN: &[u8] = b"UCOF-EXP-0002-OBJECT\0";
const PAGE_DOMAIN: &[u8] = b"UCOF-EXP-0002-PAGE\0";
const SNAPSHOT_DOMAIN: &[u8] = b"UCOF-EXP-0002-SNAPSHOT\0";
const COMMIT_DOMAIN: &[u8] = b"UCOF-EXP-0002-COMMIT\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exp0002Error {
    ArithmeticOverflow,
    Truncated,
    InvalidMagic(&'static str),
    InvalidVersion,
    InvalidLength(&'static str),
    InvalidFlags(&'static str),
    InvalidReserved(&'static str),
    UnsupportedDigest,
    InvalidObjectKind,
    InvalidObjectId,
    DuplicateObjectId(u64),
    LogicalLengthMismatch,
    InvalidPageKind,
    InvalidPageLevel,
    InvalidEntryCount,
    InvalidEntrySize,
    InvalidPageRange,
    UnorderedEntries,
    OverlappingRanges,
    InvalidPageReference,
    PageCycle,
    DigestMismatch(&'static str),
    InvalidSnapshotSequence,
    InvalidParent,
    InvalidPreviousFooter,
    InvalidCommitRange,
    InvalidRoot,
    InvalidCapabilitySet,
    PhysicalOverlap,
    ResourceLimit(&'static str),
    EmptyObjectSet,
    NoRootObjects,
}

impl fmt::Display for Exp0002Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArithmeticOverflow => write!(formatter, "arithmetic overflow"),
            Self::Truncated => write!(formatter, "truncated input"),
            Self::InvalidMagic(name) => write!(formatter, "invalid {name} magic"),
            Self::InvalidVersion => write!(formatter, "invalid experimental version"),
            Self::InvalidLength(name) => write!(formatter, "invalid {name} length"),
            Self::InvalidFlags(name) => write!(formatter, "invalid {name} flags"),
            Self::InvalidReserved(name) => write!(formatter, "non-zero reserved {name} bytes"),
            Self::UnsupportedDigest => write!(formatter, "unsupported digest algorithm"),
            Self::InvalidObjectKind => write!(formatter, "invalid object kind"),
            Self::InvalidObjectId => write!(formatter, "invalid object identifier"),
            Self::DuplicateObjectId(id) => write!(formatter, "duplicate object identifier {id}"),
            Self::LogicalLengthMismatch => write!(formatter, "logical and payload lengths differ"),
            Self::InvalidPageKind => write!(formatter, "invalid page kind"),
            Self::InvalidPageLevel => write!(formatter, "invalid page level"),
            Self::InvalidEntryCount => write!(formatter, "invalid page entry count"),
            Self::InvalidEntrySize => write!(formatter, "invalid page entry size"),
            Self::InvalidPageRange => write!(formatter, "invalid page range"),
            Self::UnorderedEntries => write!(formatter, "unordered entries"),
            Self::OverlappingRanges => write!(formatter, "overlapping ranges"),
            Self::InvalidPageReference => write!(formatter, "invalid page reference"),
            Self::PageCycle => write!(formatter, "directory page cycle"),
            Self::DigestMismatch(name) => write!(formatter, "{name} digest mismatch"),
            Self::InvalidSnapshotSequence => write!(formatter, "invalid snapshot sequence"),
            Self::InvalidParent => write!(formatter, "invalid parent snapshot"),
            Self::InvalidPreviousFooter => write!(formatter, "invalid previous footer"),
            Self::InvalidCommitRange => write!(formatter, "invalid commit range"),
            Self::InvalidRoot => write!(formatter, "invalid root identifier"),
            Self::InvalidCapabilitySet => write!(formatter, "invalid capability set"),
            Self::PhysicalOverlap => write!(formatter, "overlapping physical ranges"),
            Self::ResourceLimit(name) => write!(formatter, "resource limit exceeded: {name}"),
            Self::EmptyObjectSet => write!(formatter, "snapshot contains no objects"),
            Self::NoRootObjects => write!(formatter, "snapshot contains no roots"),
        }
    }
}

impl std::error::Error for Exp0002Error {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationLimits {
    pub max_file_bytes: u64,
    pub max_commit_bytes: u64,
    pub max_snapshot_bytes: u64,
    pub max_pages: usize,
    pub max_page_depth: usize,
    pub max_objects: usize,
    pub max_payload_bytes: u64,
    pub max_hashed_bytes: u64,
    pub max_roots: usize,
    pub max_capabilities: usize,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 16 * 1024 * 1024 * 1024,
            max_commit_bytes: 16 * 1024 * 1024 * 1024,
            max_snapshot_bytes: 16 * 1024 * 1024,
            max_pages: 1_000_000,
            max_page_depth: 32,
            max_objects: 10_000_000,
            max_payload_bytes: 16 * 1024 * 1024 * 1024,
            max_hashed_bytes: 32 * 1024 * 1024 * 1024,
            max_roots: 1_000_000,
            max_capabilities: 65_536,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHeader {
    pub file_id: [u8; 16],
    pub creation_nonce: [u8; 16],
}

impl FileHeader {
    pub fn encode(&self) -> [u8; FILE_HEADER_LEN] {
        let mut bytes = [0_u8; FILE_HEADER_LEN];
        bytes[0..8].copy_from_slice(&FILE_MAGIC);
        put_u16(&mut bytes, 8, 2);
        put_u16(&mut bytes, 10, FILE_HEADER_LEN as u16);
        put_u32(&mut bytes, 12, 0);
        put_u32(&mut bytes, 16, PAGE_SIZE as u32);
        put_u16(&mut bytes, 20, DIGEST_SHA256);
        bytes[24..40].copy_from_slice(&self.file_id);
        bytes[40..56].copy_from_slice(&self.creation_nonce);
        bytes
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, Exp0002Error> {
        require_len(bytes, FILE_HEADER_LEN)?;
        if bytes[0..8] != FILE_MAGIC {
            return Err(Exp0002Error::InvalidMagic("file header"));
        }
        if read_u16(bytes, 8)? != 2 {
            return Err(Exp0002Error::InvalidVersion);
        }
        if usize::from(read_u16(bytes, 10)?) != FILE_HEADER_LEN {
            return Err(Exp0002Error::InvalidLength("file header"));
        }
        if read_u32(bytes, 12)? != 0 {
            return Err(Exp0002Error::InvalidFlags("file header"));
        }
        if read_u32(bytes, 16)? != PAGE_SIZE as u32 {
            return Err(Exp0002Error::InvalidLength("directory page"));
        }
        if read_u16(bytes, 20)? != DIGEST_SHA256 {
            return Err(Exp0002Error::UnsupportedDigest);
        }
        require_zero(&bytes[22..24], "file header")?;
        require_zero(&bytes[56..64], "file header")?;
        Ok(Self {
            file_id: read_array(bytes, 24)?,
            creation_nonce: read_array(bytes, 40)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInput {
    pub object_id: u64,
    pub kind: u16,
    pub payload: Vec<u8>,
    pub is_root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHeader {
    pub object_id: u64,
    pub kind: u16,
    pub payload_len: u64,
    pub logical_len: u64,
}

impl ObjectHeader {
    fn encode(&self) -> [u8; OBJECT_HEADER_LEN] {
        let mut bytes = [0_u8; OBJECT_HEADER_LEN];
        bytes[0..4].copy_from_slice(&OBJECT_MAGIC);
        put_u16(&mut bytes, 4, OBJECT_HEADER_LEN as u16);
        put_u16(&mut bytes, 6, self.kind);
        put_u32(&mut bytes, 8, 0);
        put_u64(&mut bytes, 12, self.object_id);
        put_u64(&mut bytes, 20, self.payload_len);
        put_u64(&mut bytes, 28, self.logical_len);
        bytes
    }

    fn parse(bytes: &[u8]) -> Result<Self, Exp0002Error> {
        require_len(bytes, OBJECT_HEADER_LEN)?;
        if bytes[0..4] != OBJECT_MAGIC {
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
        Ok(Self {
            object_id,
            kind,
            payload_len,
            logical_len,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeafEntry {
    pub object_id: u64,
    pub kind: u16,
    pub record_offset: u64,
    pub record_len: u64,
    pub logical_len: u64,
    pub record_digest: [u8; 32],
}

impl LeafEntry {
    fn encode_into(&self, bytes: &mut [u8]) {
        put_u64(bytes, 0, self.object_id);
        put_u16(bytes, 8, self.kind);
        put_u16(bytes, 10, 0);
        put_u32(bytes, 12, 0);
        put_u64(bytes, 16, self.record_offset);
        put_u64(bytes, 24, self.record_len);
        put_u64(bytes, 32, self.logical_len);
        bytes[40..72].copy_from_slice(&self.record_digest);
    }

    fn parse(bytes: &[u8]) -> Result<Self, Exp0002Error> {
        require_len(bytes, LEAF_ENTRY_LEN)?;
        let object_id = read_u64(bytes, 0)?;
        if object_id == 0 {
            return Err(Exp0002Error::InvalidObjectId);
        }
        let kind = read_u16(bytes, 8)?;
        if kind == 0 {
            return Err(Exp0002Error::InvalidObjectKind);
        }
        if read_u16(bytes, 10)? != 0 || read_u32(bytes, 12)? != 0 {
            return Err(Exp0002Error::InvalidFlags("leaf entry"));
        }
        require_zero(&bytes[72..88], "leaf entry")?;
        Ok(Self {
            object_id,
            kind,
            record_offset: read_u64(bytes, 16)?,
            record_len: read_u64(bytes, 24)?,
            logical_len: read_u64(bytes, 32)?,
            record_digest: read_array(bytes, 40)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternalEntry {
    min_key: u64,
    max_key: u64,
    page_offset: u64,
    page_len: u32,
    level: u16,
    page_digest: [u8; 32],
}

impl InternalEntry {
    fn encode_into(&self, bytes: &mut [u8]) {
        put_u64(bytes, 0, self.min_key);
        put_u64(bytes, 8, self.max_key);
        put_u64(bytes, 16, self.page_offset);
        put_u32(bytes, 24, self.page_len);
        put_u16(bytes, 28, self.level);
        put_u16(bytes, 30, 0);
        bytes[32..64].copy_from_slice(&self.page_digest);
    }

    fn parse(bytes: &[u8]) -> Result<Self, Exp0002Error> {
        require_len(bytes, INTERNAL_ENTRY_LEN)?;
        let min_key = read_u64(bytes, 0)?;
        let max_key = read_u64(bytes, 8)?;
        if min_key == 0 || min_key > max_key {
            return Err(Exp0002Error::InvalidPageRange);
        }
        if read_u32(bytes, 24)? as usize != PAGE_SIZE {
            return Err(Exp0002Error::InvalidLength("child page"));
        }
        if read_u16(bytes, 30)? != 0 {
            return Err(Exp0002Error::InvalidFlags("internal entry"));
        }
        Ok(Self {
            min_key,
            max_key,
            page_offset: read_u64(bytes, 16)?,
            page_len: read_u32(bytes, 24)?,
            level: read_u16(bytes, 28)?,
            page_digest: read_array(bytes, 32)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PageHeader {
    kind: u8,
    level: u8,
    entry_count: u16,
    min_key: u64,
    max_key: u64,
    sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedPage {
    Leaf(PageHeader, Vec<LeafEntry>),
    Internal(PageHeader, Vec<InternalEntry>),
}

impl ParsedPage {
    fn header(&self) -> &PageHeader {
        match self {
            Self::Leaf(header, _) | Self::Internal(header, _) => header,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PageLocator {
    min_key: u64,
    max_key: u64,
    offset: u64,
    level: u16,
    digest: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub sequence: u64,
    pub parent_snapshot_digest: [u8; 32],
    pub previous_footer_offset: u64,
    pub directory_root_offset: u64,
    pub directory_root_len: u32,
    pub directory_root_level: u16,
    pub directory_root_digest: [u8; 32],
    pub roots: Vec<u64>,
    pub required_capabilities: Vec<u64>,
    pub optional_capabilities: Vec<u64>,
}

impl Snapshot {
    pub fn encode(&self) -> Result<Vec<u8>, Exp0002Error> {
        validate_sorted_unique(&self.roots, Exp0002Error::InvalidRoot)?;
        validate_sorted_unique(
            &self.required_capabilities,
            Exp0002Error::InvalidCapabilitySet,
        )?;
        validate_sorted_unique(
            &self.optional_capabilities,
            Exp0002Error::InvalidCapabilitySet,
        )?;
        if self
            .required_capabilities
            .iter()
            .any(|value| self.optional_capabilities.binary_search(value).is_ok())
        {
            return Err(Exp0002Error::InvalidCapabilitySet);
        }
        let count = self
            .roots
            .len()
            .checked_add(self.required_capabilities.len())
            .and_then(|value| value.checked_add(self.optional_capabilities.len()))
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        let array_bytes = count
            .checked_mul(8)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        let total_len = SNAPSHOT_HEADER_LEN
            .checked_add(array_bytes)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        let mut bytes = vec![0_u8; total_len];
        bytes[0..4].copy_from_slice(&SNAPSHOT_MAGIC);
        put_u16(&mut bytes, 4, SNAPSHOT_HEADER_LEN as u16);
        put_u16(&mut bytes, 6, 0);
        put_u64(&mut bytes, 8, self.sequence);
        bytes[16..48].copy_from_slice(&self.parent_snapshot_digest);
        put_u64(&mut bytes, 48, self.previous_footer_offset);
        put_u64(&mut bytes, 56, self.directory_root_offset);
        put_u32(&mut bytes, 64, self.directory_root_len);
        put_u16(&mut bytes, 68, self.directory_root_level);
        put_u16(&mut bytes, 70, DIGEST_SHA256);
        bytes[72..104].copy_from_slice(&self.directory_root_digest);
        put_u32(
            &mut bytes,
            104,
            u32::try_from(self.roots.len()).map_err(|_| Exp0002Error::ArithmeticOverflow)?,
        );
        put_u32(
            &mut bytes,
            108,
            u32::try_from(self.required_capabilities.len())
                .map_err(|_| Exp0002Error::ArithmeticOverflow)?,
        );
        put_u32(
            &mut bytes,
            112,
            u32::try_from(self.optional_capabilities.len())
                .map_err(|_| Exp0002Error::ArithmeticOverflow)?,
        );
        let mut cursor = SNAPSHOT_HEADER_LEN;
        for value in self
            .roots
            .iter()
            .chain(&self.required_capabilities)
            .chain(&self.optional_capabilities)
        {
            put_u64(&mut bytes, cursor, *value);
            cursor += 8;
        }
        Ok(bytes)
    }

    pub fn parse(bytes: &[u8], limits: &ValidationLimits) -> Result<Self, Exp0002Error> {
        require_len(bytes, SNAPSHOT_HEADER_LEN)?;
        if bytes[0..4] != SNAPSHOT_MAGIC {
            return Err(Exp0002Error::InvalidMagic("snapshot"));
        }
        if usize::from(read_u16(bytes, 4)?) != SNAPSHOT_HEADER_LEN {
            return Err(Exp0002Error::InvalidLength("snapshot header"));
        }
        if read_u16(bytes, 6)? != 0 {
            return Err(Exp0002Error::InvalidFlags("snapshot"));
        }
        if read_u16(bytes, 70)? != DIGEST_SHA256 {
            return Err(Exp0002Error::UnsupportedDigest);
        }
        if read_u32(bytes, 64)? as usize != PAGE_SIZE {
            return Err(Exp0002Error::InvalidLength("directory root"));
        }
        require_zero(&bytes[116..160], "snapshot")?;
        let root_count = usize::try_from(read_u32(bytes, 104)?)
            .map_err(|_| Exp0002Error::ArithmeticOverflow)?;
        let required_count = usize::try_from(read_u32(bytes, 108)?)
            .map_err(|_| Exp0002Error::ArithmeticOverflow)?;
        let optional_count = usize::try_from(read_u32(bytes, 112)?)
            .map_err(|_| Exp0002Error::ArithmeticOverflow)?;
        if root_count > limits.max_roots {
            return Err(Exp0002Error::ResourceLimit("snapshot roots"));
        }
        if required_count
            .checked_add(optional_count)
            .ok_or(Exp0002Error::ArithmeticOverflow)?
            > limits.max_capabilities
        {
            return Err(Exp0002Error::ResourceLimit("capabilities"));
        }
        let count = root_count
            .checked_add(required_count)
            .and_then(|value| value.checked_add(optional_count))
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        let expected_len = SNAPSHOT_HEADER_LEN
            .checked_add(
                count
                    .checked_mul(8)
                    .ok_or(Exp0002Error::ArithmeticOverflow)?,
            )
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if bytes.len() != expected_len {
            return Err(Exp0002Error::InvalidLength("snapshot"));
        }
        if u64::try_from(expected_len).map_err(|_| Exp0002Error::ArithmeticOverflow)?
            > limits.max_snapshot_bytes
        {
            return Err(Exp0002Error::ResourceLimit("snapshot bytes"));
        }
        let mut cursor = SNAPSHOT_HEADER_LEN;
        let roots = read_u64_array(bytes, &mut cursor, root_count)?;
        let required_capabilities = read_u64_array(bytes, &mut cursor, required_count)?;
        let optional_capabilities = read_u64_array(bytes, &mut cursor, optional_count)?;
        validate_sorted_unique(&roots, Exp0002Error::InvalidRoot)?;
        validate_sorted_unique(
            &required_capabilities,
            Exp0002Error::InvalidCapabilitySet,
        )?;
        validate_sorted_unique(
            &optional_capabilities,
            Exp0002Error::InvalidCapabilitySet,
        )?;
        if required_capabilities
            .iter()
            .any(|value| optional_capabilities.binary_search(value).is_ok())
        {
            return Err(Exp0002Error::InvalidCapabilitySet);
        }
        Ok(Self {
            sequence: read_u64(bytes, 8)?,
            parent_snapshot_digest: read_array(bytes, 16)?,
            previous_footer_offset: read_u64(bytes, 48)?,
            directory_root_offset: read_u64(bytes, 56)?,
            directory_root_len: read_u32(bytes, 64)?,
            directory_root_level: read_u16(bytes, 68)?,
            directory_root_digest: read_array(bytes, 72)?,
            roots,
            required_capabilities,
            optional_capabilities,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    pub commit_start: u64,
    pub commit_len: u64,
    pub snapshot_offset: u64,
    pub snapshot_len: u64,
    pub sequence: u64,
    pub previous_footer_offset: u64,
    pub record_count: u64,
    pub snapshot_digest: [u8; 32],
    pub commit_digest: [u8; 32],
}

impl Footer {
    fn semantics(&self) -> [u8; 104] {
        let mut bytes = [0_u8; 104];
        put_u16(&mut bytes, 0, FOOTER_LEN as u16);
        put_u16(&mut bytes, 2, 2);
        put_u32(&mut bytes, 4, 0);
        put_u64(&mut bytes, 8, self.commit_start);
        put_u64(&mut bytes, 16, self.commit_len);
        put_u64(&mut bytes, 24, self.snapshot_offset);
        put_u64(&mut bytes, 32, self.snapshot_len);
        put_u64(&mut bytes, 40, self.sequence);
        put_u64(&mut bytes, 48, self.previous_footer_offset);
        put_u64(&mut bytes, 56, self.record_count);
        put_u16(&mut bytes, 64, DIGEST_SHA256);
        bytes[72..104].copy_from_slice(&self.snapshot_digest);
        bytes
    }

    fn encode(&self) -> [u8; FOOTER_LEN] {
        let mut bytes = [0_u8; FOOTER_LEN];
        bytes[0..8].copy_from_slice(&FOOTER_MAGIC);
        bytes[8..112].copy_from_slice(&self.semantics());
        bytes[112..144].copy_from_slice(&self.commit_digest);
        bytes
    }

    fn parse(bytes: &[u8]) -> Result<Self, Exp0002Error> {
        require_len(bytes, FOOTER_LEN)?;
        if bytes.len() != FOOTER_LEN {
            return Err(Exp0002Error::InvalidLength("footer"));
        }
        if bytes[0..8] != FOOTER_MAGIC {
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
        Ok(Self {
            commit_start: read_u64(bytes, 16)?,
            commit_len: read_u64(bytes, 24)?,
            snapshot_offset: read_u64(bytes, 32)?,
            snapshot_len: read_u64(bytes, 40)?,
            sequence: read_u64(bytes, 48)?,
            previous_footer_offset: read_u64(bytes, 56)?,
            record_count: read_u64(bytes, 64)?,
            snapshot_digest: read_array(bytes, 80)?,
            commit_digest: read_array(bytes, 112)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedExp0002 {
    pub header: FileHeader,
    pub footer_offset: u64,
    pub footer: Footer,
    pub snapshot: Snapshot,
    pub objects: Vec<LeafEntry>,
    pub pages_verified: usize,
    pub bytes_hashed: u64,
}

pub fn build_genesis(
    header: FileHeader,
    objects: Vec<ObjectInput>,
) -> Result<Vec<u8>, Exp0002Error> {
    build_commit(Vec::new(), Some(header), None, objects)
}

pub fn build_append(
    previous_file: &[u8],
    new_objects: Vec<ObjectInput>,
    roots: Vec<u64>,
    limits: &ValidationLimits,
) -> Result<Vec<u8>, Exp0002Error> {
    let previous = validate_strict(previous_file, limits)?;
    let roots = canonical_values(roots, Exp0002Error::InvalidRoot)?;
    if roots.is_empty() {
        return Err(Exp0002Error::NoRootObjects);
    }
    let mut existing = previous.objects.clone();
    let mut seen: BTreeSet<u64> = existing.iter().map(|entry| entry.object_id).collect();
    for object in &new_objects {
        if !seen.insert(object.object_id) {
            return Err(Exp0002Error::DuplicateObjectId(object.object_id));
        }
    }
    let mut bytes = previous_file.to_vec();
    let commit_start = to_u64(bytes.len())?;
    let mut added = write_object_records(&mut bytes, new_objects)?;
    existing.append(&mut added);
    existing.sort_by_key(|entry| entry.object_id);
    let available: BTreeSet<u64> = existing.iter().map(|entry| entry.object_id).collect();
    if roots.iter().any(|root| !available.contains(root)) {
        return Err(Exp0002Error::InvalidRoot);
    }
    let sequence = previous
        .snapshot
        .sequence
        .checked_add(1)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    finish_commit(
        bytes,
        existing,
        roots,
        sequence,
        previous.footer.snapshot_digest,
        previous.footer_offset,
        commit_start,
        u64::try_from(seen.len() - previous.objects.len())
            .map_err(|_| Exp0002Error::ArithmeticOverflow)?,
    )
}

fn build_commit(
    mut bytes: Vec<u8>,
    header: Option<FileHeader>,
    previous: Option<&VerifiedExp0002>,
    objects: Vec<ObjectInput>,
) -> Result<Vec<u8>, Exp0002Error> {
    if objects.is_empty() {
        return Err(Exp0002Error::EmptyObjectSet);
    }
    let header = header.ok_or(Exp0002Error::InvalidParent)?;
    bytes.extend_from_slice(&header.encode());
    let mut roots = Vec::new();
    for object in &objects {
        if object.is_root {
            roots.push(object.object_id);
        }
    }
    roots = canonical_values(roots, Exp0002Error::InvalidRoot)?;
    if roots.is_empty() {
        return Err(Exp0002Error::NoRootObjects);
    }
    let entries = write_object_records(&mut bytes, objects)?;
    let (sequence, parent_digest, previous_footer_offset, commit_start) = match previous {
        None => (0, [0_u8; 32], ABSENT_OFFSET, 0),
        Some(value) => (
            value
                .snapshot
                .sequence
                .checked_add(1)
                .ok_or(Exp0002Error::ArithmeticOverflow)?,
            value.footer.snapshot_digest,
            value.footer_offset,
            to_u64(value.footer_offset as usize + FOOTER_LEN)?,
        ),
    };
    let record_count = to_u64(entries.len())?;
    finish_commit(
        bytes,
        entries,
        roots,
        sequence,
        parent_digest,
        previous_footer_offset,
        commit_start,
        record_count,
    )
}

fn write_object_records(
    bytes: &mut Vec<u8>,
    mut objects: Vec<ObjectInput>,
) -> Result<Vec<LeafEntry>, Exp0002Error> {
    objects.sort_by_key(|object| object.object_id);
    let mut previous_id = None;
    let mut entries = Vec::with_capacity(objects.len());
    for object in objects {
        if object.object_id == 0 {
            return Err(Exp0002Error::InvalidObjectId);
        }
        if object.kind == 0 {
            return Err(Exp0002Error::InvalidObjectKind);
        }
        if previous_id == Some(object.object_id) {
            return Err(Exp0002Error::DuplicateObjectId(object.object_id));
        }
        previous_id = Some(object.object_id);
        let payload_len = to_u64(object.payload.len())?;
        let header = ObjectHeader {
            object_id: object.object_id,
            kind: object.kind,
            payload_len,
            logical_len: payload_len,
        }
        .encode();
        let record_offset = to_u64(bytes.len())?;
        let mut hasher = Sha256::new();
        hasher.update(OBJECT_DOMAIN);
        hasher.update(header);
        hasher.update(&object.payload);
        let digest: [u8; 32] = hasher.finalize().into();
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(&object.payload);
        entries.push(LeafEntry {
            object_id: object.object_id,
            kind: object.kind,
            record_offset,
            record_len: u64::try_from(OBJECT_HEADER_LEN)
                .map_err(|_| Exp0002Error::ArithmeticOverflow)?
                .checked_add(payload_len)
                .ok_or(Exp0002Error::ArithmeticOverflow)?,
            logical_len: payload_len,
            record_digest: digest,
        });
    }
    Ok(entries)
}

#[allow(clippy::too_many_arguments)]
fn finish_commit(
    mut bytes: Vec<u8>,
    entries: Vec<LeafEntry>,
    roots: Vec<u64>,
    sequence: u64,
    parent_snapshot_digest: [u8; 32],
    previous_footer_offset: u64,
    commit_start: u64,
    record_count: u64,
) -> Result<Vec<u8>, Exp0002Error> {
    let root = write_directory(&mut bytes, entries, sequence)?;
    let snapshot = Snapshot {
        sequence,
        parent_snapshot_digest,
        previous_footer_offset,
        directory_root_offset: root.offset,
        directory_root_len: PAGE_SIZE as u32,
        directory_root_level: root.level,
        directory_root_digest: root.digest,
        roots,
        required_capabilities: Vec::new(),
        optional_capabilities: Vec::new(),
    };
    let snapshot_bytes = snapshot.encode()?;
    let snapshot_offset = to_u64(bytes.len())?;
    let snapshot_digest = digest_bytes(SNAPSHOT_DOMAIN, &snapshot_bytes);
    bytes.extend_from_slice(&snapshot_bytes);
    let footer_offset = to_u64(bytes.len())?;
    let commit_len = footer_offset
        .checked_sub(commit_start)
        .ok_or(Exp0002Error::InvalidCommitRange)?;
    let mut footer = Footer {
        commit_start,
        commit_len,
        snapshot_offset,
        snapshot_len: to_u64(snapshot_bytes.len())?,
        sequence,
        previous_footer_offset,
        record_count,
        snapshot_digest,
        commit_digest: [0_u8; 32],
    };
    let start = to_usize(commit_start)?;
    let end = to_usize(footer_offset)?;
    footer.commit_digest = digest_commit(&bytes[start..end], &footer);
    bytes.extend_from_slice(&footer.encode());
    Ok(bytes)
}

fn write_directory(
    bytes: &mut Vec<u8>,
    entries: Vec<LeafEntry>,
    sequence: u64,
) -> Result<PageLocator, Exp0002Error> {
    if entries.is_empty() {
        return Err(Exp0002Error::EmptyObjectSet);
    }
    let mut level = Vec::new();
    for chunk in entries.chunks(LEAF_CAPACITY) {
        let page = encode_leaf_page(chunk, sequence)?;
        let offset = to_u64(bytes.len())?;
        let digest = digest_bytes(PAGE_DOMAIN, &page);
        bytes.extend_from_slice(&page);
        level.push(PageLocator {
            min_key: chunk.first().expect("non-empty chunk").object_id,
            max_key: chunk.last().expect("non-empty chunk").object_id,
            offset,
            level: 0,
            digest,
        });
    }
    while level.len() > 1 {
        let mut next = Vec::new();
        for chunk in level.chunks(INTERNAL_CAPACITY) {
            let parent_level = chunk[0]
                .level
                .checked_add(1)
                .ok_or(Exp0002Error::ArithmeticOverflow)?;
            let page = encode_internal_page(chunk, sequence, parent_level)?;
            let offset = to_u64(bytes.len())?;
            let digest = digest_bytes(PAGE_DOMAIN, &page);
            bytes.extend_from_slice(&page);
            next.push(PageLocator {
                min_key: chunk.first().expect("non-empty chunk").min_key,
                max_key: chunk.last().expect("non-empty chunk").max_key,
                offset,
                level: parent_level,
                digest,
            });
        }
        level = next;
    }
    level.pop().ok_or(Exp0002Error::EmptyObjectSet)
}

fn encode_leaf_page(
    entries: &[LeafEntry],
    sequence: u64,
) -> Result<[u8; PAGE_SIZE], Exp0002Error> {
    if entries.is_empty() || entries.len() > LEAF_CAPACITY {
        return Err(Exp0002Error::InvalidEntryCount);
    }
    for pair in entries.windows(2) {
        if pair[0].object_id >= pair[1].object_id {
            return Err(Exp0002Error::UnorderedEntries);
        }
    }
    let mut page = [0_u8; PAGE_SIZE];
    encode_page_header(
        &mut page,
        1,
        0,
        entries.len(),
        LEAF_ENTRY_LEN,
        entries.first().expect("entries").object_id,
        entries.last().expect("entries").object_id,
        sequence,
    )?;
    for (index, entry) in entries.iter().enumerate() {
        let start = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
        entry.encode_into(&mut page[start..start + LEAF_ENTRY_LEN]);
    }
    Ok(page)
}

fn encode_internal_page(
    children: &[PageLocator],
    sequence: u64,
    level: u16,
) -> Result<[u8; PAGE_SIZE], Exp0002Error> {
    if children.is_empty() || children.len() > INTERNAL_CAPACITY {
        return Err(Exp0002Error::InvalidEntryCount);
    }
    if level == 0 || level > u16::from(u8::MAX) {
        return Err(Exp0002Error::InvalidPageLevel);
    }
    for pair in children.windows(2) {
        if pair[0].max_key >= pair[1].min_key || pair[0].level != pair[1].level {
            return Err(Exp0002Error::OverlappingRanges);
        }
    }
    let mut page = [0_u8; PAGE_SIZE];
    encode_page_header(
        &mut page,
        2,
        u8::try_from(level).map_err(|_| Exp0002Error::InvalidPageLevel)?,
        children.len(),
        INTERNAL_ENTRY_LEN,
        children.first().expect("children").min_key,
        children.last().expect("children").max_key,
        sequence,
    )?;
    for (index, child) in children.iter().enumerate() {
        let start = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
        InternalEntry {
            min_key: child.min_key,
            max_key: child.max_key,
            page_offset: child.offset,
            page_len: PAGE_SIZE as u32,
            level: child.level,
            page_digest: child.digest,
        }
        .encode_into(&mut page[start..start + INTERNAL_ENTRY_LEN]);
    }
    Ok(page)
}

#[allow(clippy::too_many_arguments)]
fn encode_page_header(
    page: &mut [u8],
    kind: u8,
    level: u8,
    entry_count: usize,
    entry_size: usize,
    min_key: u64,
    max_key: u64,
    sequence: u64,
) -> Result<(), Exp0002Error> {
    page[0..4].copy_from_slice(&PAGE_MAGIC);
    page[4] = kind;
    page[5] = level;
    put_u16(page, 6, PAGE_HEADER_LEN as u16);
    put_u16(
        page,
        8,
        u16::try_from(entry_count).map_err(|_| Exp0002Error::ArithmeticOverflow)?,
    );
    put_u16(
        page,
        10,
        u16::try_from(entry_size).map_err(|_| Exp0002Error::ArithmeticOverflow)?,
    );
    put_u32(page, 12, 0);
    put_u64(page, 16, min_key);
    put_u64(page, 24, max_key);
    put_u64(page, 32, sequence);
    Ok(())
}

fn parse_page(bytes: &[u8]) -> Result<ParsedPage, Exp0002Error> {
    if bytes.len() != PAGE_SIZE {
        return Err(Exp0002Error::InvalidLength("page"));
    }
    if bytes[0..4] != PAGE_MAGIC {
        return Err(Exp0002Error::InvalidMagic("page"));
    }
    let kind = bytes[4];
    let level = bytes[5];
    if usize::from(read_u16(bytes, 6)?) != PAGE_HEADER_LEN {
        return Err(Exp0002Error::InvalidLength("page header"));
    }
    let entry_count = read_u16(bytes, 8)?;
    if entry_count == 0 {
        return Err(Exp0002Error::InvalidEntryCount);
    }
    let entry_size = usize::from(read_u16(bytes, 10)?);
    if read_u32(bytes, 12)? != 0 {
        return Err(Exp0002Error::InvalidFlags("page"));
    }
    let min_key = read_u64(bytes, 16)?;
    let max_key = read_u64(bytes, 24)?;
    if min_key == 0 || min_key > max_key {
        return Err(Exp0002Error::InvalidPageRange);
    }
    require_zero(&bytes[40..64], "page header")?;
    let header = PageHeader {
        kind,
        level,
        entry_count,
        min_key,
        max_key,
        sequence: read_u64(bytes, 32)?,
    };
    let count = usize::from(entry_count);
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
            let used = PAGE_HEADER_LEN + count * LEAF_ENTRY_LEN;
            require_zero(&bytes[used..], "page padding")?;
            let mut entries = Vec::with_capacity(count);
            for index in 0..count {
                let start = PAGE_HEADER_LEN + index * LEAF_ENTRY_LEN;
                entries.push(LeafEntry::parse(&bytes[start..start + LEAF_ENTRY_LEN])?);
            }
            validate_leaf_order(&header, &entries)?;
            Ok(ParsedPage::Leaf(header, entries))
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
            let used = PAGE_HEADER_LEN + count * INTERNAL_ENTRY_LEN;
            require_zero(&bytes[used..], "page padding")?;
            let mut entries = Vec::with_capacity(count);
            for index in 0..count {
                let start = PAGE_HEADER_LEN + index * INTERNAL_ENTRY_LEN;
                entries.push(InternalEntry::parse(
                    &bytes[start..start + INTERNAL_ENTRY_LEN],
                )?);
            }
            validate_internal_order(&header, &entries)?;
            Ok(ParsedPage::Internal(header, entries))
        }
        _ => Err(Exp0002Error::InvalidPageKind),
    }
}

fn validate_leaf_order(
    header: &PageHeader,
    entries: &[LeafEntry],
) -> Result<(), Exp0002Error> {
    if entries.first().map(|entry| entry.object_id) != Some(header.min_key)
        || entries.last().map(|entry| entry.object_id) != Some(header.max_key)
    {
        return Err(Exp0002Error::InvalidPageRange);
    }
    if entries
        .windows(2)
        .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(Exp0002Error::UnorderedEntries);
    }
    Ok(())
}

fn validate_internal_order(
    header: &PageHeader,
    entries: &[InternalEntry],
) -> Result<(), Exp0002Error> {
    if entries.first().map(|entry| entry.min_key) != Some(header.min_key)
        || entries.last().map(|entry| entry.max_key) != Some(header.max_key)
    {
        return Err(Exp0002Error::InvalidPageRange);
    }
    for entry in entries {
        if entry.level.checked_add(1) != Some(u16::from(header.level)) {
            return Err(Exp0002Error::InvalidPageLevel);
        }
    }
    if entries
        .windows(2)
        .any(|pair| pair[0].max_key >= pair[1].min_key)
    {
        return Err(Exp0002Error::OverlappingRanges);
    }
    Ok(())
}

pub fn validate_strict(
    bytes: &[u8],
    limits: &ValidationLimits,
) -> Result<VerifiedExp0002, Exp0002Error> {
    let file_len = to_u64(bytes.len())?;
    if file_len > limits.max_file_bytes {
        return Err(Exp0002Error::ResourceLimit("file bytes"));
    }
    if bytes.len() < FILE_HEADER_LEN + FOOTER_LEN {
        return Err(Exp0002Error::Truncated);
    }
    let header = FileHeader::parse(&bytes[..FILE_HEADER_LEN])?;
    let footer_offset_usize = bytes
        .len()
        .checked_sub(FOOTER_LEN)
        .ok_or(Exp0002Error::Truncated)?;
    let footer_offset = to_u64(footer_offset_usize)?;
    let footer = Footer::parse(&bytes[footer_offset_usize..])?;
    let expected_commit_end = footer
        .commit_start
        .checked_add(footer.commit_len)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    if expected_commit_end != footer_offset || footer.commit_len > limits.max_commit_bytes {
        return Err(if footer.commit_len > limits.max_commit_bytes {
            Exp0002Error::ResourceLimit("commit bytes")
        } else {
            Exp0002Error::InvalidCommitRange
        });
    }
    if footer.snapshot_offset < footer.commit_start {
        return Err(Exp0002Error::InvalidCommitRange);
    }
    let snapshot_range = checked_range(
        footer.snapshot_offset,
        footer.snapshot_len,
        bytes.len(),
    )?;
    if snapshot_range.end > footer_offset_usize {
        return Err(Exp0002Error::InvalidCommitRange);
    }
    let snapshot_bytes = &bytes[snapshot_range.clone()];
    if digest_bytes(SNAPSHOT_DOMAIN, snapshot_bytes) != footer.snapshot_digest {
        return Err(Exp0002Error::DigestMismatch("snapshot"));
    }
    let snapshot = Snapshot::parse(snapshot_bytes, limits)?;
    if snapshot.sequence != footer.sequence
        || snapshot.previous_footer_offset != footer.previous_footer_offset
    {
        return Err(Exp0002Error::InvalidSnapshotSequence);
    }
    validate_parent_link(bytes, footer_offset, &footer, &snapshot)?;
    let mut hashed_bytes = footer.commit_len;
    if hashed_bytes > limits.max_hashed_bytes {
        return Err(Exp0002Error::ResourceLimit("hashed bytes"));
    }
    let commit_range = checked_range(footer.commit_start, footer.commit_len, bytes.len())?;
    if digest_commit(&bytes[commit_range], &footer) != footer.commit_digest {
        return Err(Exp0002Error::DigestMismatch("commit"));
    }
    let directory = validate_directory(bytes, &snapshot, limits, &mut hashed_bytes)?;
    validate_objects(
        bytes,
        &directory.entries,
        &directory.page_ranges,
        &snapshot_range,
        footer_offset_usize..bytes.len(),
        limits,
        &mut hashed_bytes,
    )?;
    let object_ids: BTreeSet<u64> = directory
        .entries
        .iter()
        .map(|entry| entry.object_id)
        .collect();
    if snapshot.roots.is_empty() || snapshot.roots.iter().any(|root| !object_ids.contains(root)) {
        return Err(Exp0002Error::InvalidRoot);
    }
    Ok(VerifiedExp0002 {
        header,
        footer_offset,
        footer,
        snapshot,
        objects: directory.entries,
        pages_verified: directory.pages,
        bytes_hashed: hashed_bytes,
    })
}

fn validate_parent_link(
    bytes: &[u8],
    footer_offset: u64,
    footer: &Footer,
    snapshot: &Snapshot,
) -> Result<(), Exp0002Error> {
    if footer.previous_footer_offset == ABSENT_OFFSET {
        if footer.sequence != 0
            || footer.commit_start != 0
            || snapshot.parent_snapshot_digest != [0_u8; 32]
        {
            return Err(Exp0002Error::InvalidParent);
        }
        return Ok(());
    }
    if footer.previous_footer_offset >= footer_offset {
        return Err(Exp0002Error::InvalidPreviousFooter);
    }
    let previous_range = checked_range(
        footer.previous_footer_offset,
        FOOTER_LEN as u64,
        bytes.len(),
    )?;
    if previous_range.end > to_usize(footer_offset)? {
        return Err(Exp0002Error::InvalidPreviousFooter);
    }
    let previous = Footer::parse(&bytes[previous_range])?;
    if previous.snapshot_digest != snapshot.parent_snapshot_digest
        || previous
            .sequence
            .checked_add(1)
            .ok_or(Exp0002Error::ArithmeticOverflow)?
            != footer.sequence
        || footer.commit_start
            != footer
                .previous_footer_offset
                .checked_add(FOOTER_LEN as u64)
                .ok_or(Exp0002Error::ArithmeticOverflow)?
    {
        return Err(Exp0002Error::InvalidParent);
    }
    Ok(())
}

struct DirectoryValidation {
    entries: Vec<LeafEntry>,
    page_ranges: Vec<std::ops::Range<usize>>,
    pages: usize,
}

fn validate_directory(
    bytes: &[u8],
    snapshot: &Snapshot,
    limits: &ValidationLimits,
    hashed_bytes: &mut u64,
) -> Result<DirectoryValidation, Exp0002Error> {
    #[derive(Clone)]
    struct Expected {
        offset: u64,
        digest: [u8; 32],
        level: u16,
        min_key: u64,
        max_key: u64,
        depth: usize,
    }
    let mut stack = vec![Expected {
        offset: snapshot.directory_root_offset,
        digest: snapshot.directory_root_digest,
        level: snapshot.directory_root_level,
        min_key: 0,
        max_key: u64::MAX,
        depth: 1,
    }];
    let mut visited = BTreeSet::new();
    let mut entries = Vec::new();
    let mut page_ranges = Vec::new();
    while let Some(expected) = stack.pop() {
        if expected.depth > limits.max_page_depth {
            return Err(Exp0002Error::ResourceLimit("page depth"));
        }
        if !visited.insert(expected.offset) {
            return Err(Exp0002Error::PageCycle);
        }
        if visited.len() > limits.max_pages {
            return Err(Exp0002Error::ResourceLimit("pages"));
        }
        let range = checked_range(expected.offset, PAGE_SIZE as u64, bytes.len())?;
        let page_bytes = &bytes[range.clone()];
        add_hashed(hashed_bytes, PAGE_SIZE as u64, limits)?;
        if digest_bytes(PAGE_DOMAIN, page_bytes) != expected.digest {
            return Err(Exp0002Error::DigestMismatch("page"));
        }
        let page = parse_page(page_bytes)?;
        let header = page.header();
        if u16::from(header.level) != expected.level
            || (expected.min_key != 0 && header.min_key != expected.min_key)
            || (expected.max_key != u64::MAX && header.max_key != expected.max_key)
            || header.sequence != snapshot.sequence
        {
            return Err(Exp0002Error::InvalidPageReference);
        }
        page_ranges.push(range);
        match page {
            ParsedPage::Leaf(_, mut page_entries) => {
                entries.append(&mut page_entries);
                if entries.len() > limits.max_objects {
                    return Err(Exp0002Error::ResourceLimit("objects"));
                }
            }
            ParsedPage::Internal(_, children) => {
                for child in children.into_iter().rev() {
                    stack.push(Expected {
                        offset: child.page_offset,
                        digest: child.page_digest,
                        level: child.level,
                        min_key: child.min_key,
                        max_key: child.max_key,
                        depth: expected.depth + 1,
                    });
                }
            }
        }
    }
    entries.sort_by_key(|entry| entry.object_id);
    if entries
        .windows(2)
        .any(|pair| pair[0].object_id >= pair[1].object_id)
    {
        return Err(Exp0002Error::DuplicateObjectId(
            entries
                .windows(2)
                .find(|pair| pair[0].object_id >= pair[1].object_id)
                .map(|pair| pair[1].object_id)
                .unwrap_or(0),
        ));
    }
    Ok(DirectoryValidation {
        entries,
        page_ranges,
        pages: visited.len(),
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_objects(
    bytes: &[u8],
    entries: &[LeafEntry],
    page_ranges: &[std::ops::Range<usize>],
    snapshot_range: &std::ops::Range<usize>,
    footer_range: std::ops::Range<usize>,
    limits: &ValidationLimits,
    hashed_bytes: &mut u64,
) -> Result<(), Exp0002Error> {
    let mut physical = Vec::with_capacity(entries.len());
    let mut payload_total = 0_u64;
    for entry in entries {
        if entry.record_len < OBJECT_HEADER_LEN as u64 {
            return Err(Exp0002Error::InvalidLength("object record"));
        }
        let range = checked_range(entry.record_offset, entry.record_len, bytes.len())?;
        if overlaps_any(&range, page_ranges)
            || ranges_overlap(&range, snapshot_range)
            || ranges_overlap(&range, &footer_range)
        {
            return Err(Exp0002Error::PhysicalOverlap);
        }
        let header = ObjectHeader::parse(&bytes[range.start..range.start + OBJECT_HEADER_LEN])?;
        let expected_len = (OBJECT_HEADER_LEN as u64)
            .checked_add(header.payload_len)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if header.object_id != entry.object_id
            || header.kind != entry.kind
            || header.logical_len != entry.logical_len
            || expected_len != entry.record_len
        {
            return Err(Exp0002Error::InvalidLength("object locator"));
        }
        payload_total = payload_total
            .checked_add(header.payload_len)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
        if payload_total > limits.max_payload_bytes {
            return Err(Exp0002Error::ResourceLimit("payload bytes"));
        }
        add_hashed(hashed_bytes, entry.record_len, limits)?;
        if digest_bytes(OBJECT_DOMAIN, &bytes[range.clone()]) != entry.record_digest {
            return Err(Exp0002Error::DigestMismatch("object"));
        }
        physical.push(range);
    }
    physical.sort_by_key(|range| range.start);
    if physical
        .windows(2)
        .any(|pair| ranges_overlap(&pair[0], &pair[1]))
    {
        return Err(Exp0002Error::PhysicalOverlap);
    }
    Ok(())
}

fn add_hashed(
    hashed: &mut u64,
    amount: u64,
    limits: &ValidationLimits,
) -> Result<(), Exp0002Error> {
    *hashed = hashed
        .checked_add(amount)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    if *hashed > limits.max_hashed_bytes {
        return Err(Exp0002Error::ResourceLimit("hashed bytes"));
    }
    Ok(())
}

fn digest_bytes(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    hasher.finalize().into()
}

fn digest_commit(commit_bytes: &[u8], footer: &Footer) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(COMMIT_DOMAIN);
    hasher.update(commit_bytes);
    hasher.update(footer.semantics());
    hasher.finalize().into()
}

fn canonical_values(
    mut values: Vec<u64>,
    error: Exp0002Error,
) -> Result<Vec<u64>, Exp0002Error> {
    values.sort_unstable();
    if values.first() == Some(&0) || values.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(error);
    }
    Ok(values)
}

fn validate_sorted_unique(values: &[u64], error: Exp0002Error) -> Result<(), Exp0002Error> {
    if values.first() == Some(&0) || values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(error);
    }
    Ok(())
}

fn read_u64_array(
    bytes: &[u8],
    cursor: &mut usize,
    count: usize,
) -> Result<Vec<u64>, Exp0002Error> {
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(read_u64(bytes, *cursor)?);
        *cursor = cursor
            .checked_add(8)
            .ok_or(Exp0002Error::ArithmeticOverflow)?;
    }
    Ok(values)
}

fn checked_range(
    offset: u64,
    len: u64,
    total: usize,
) -> Result<std::ops::Range<usize>, Exp0002Error> {
    let end = offset
        .checked_add(len)
        .ok_or(Exp0002Error::ArithmeticOverflow)?;
    let start = to_usize(offset)?;
    let end = to_usize(end)?;
    if end > total || start > end {
        return Err(Exp0002Error::Truncated);
    }
    Ok(start..end)
}

fn overlaps_any(
    range: &std::ops::Range<usize>,
    ranges: &[std::ops::Range<usize>],
) -> bool {
    ranges.iter().any(|other| ranges_overlap(range, other))
}

fn ranges_overlap(
    left: &std::ops::Range<usize>,
    right: &std::ops::Range<usize>,
) -> bool {
    left.start < right.end && right.start < left.end
}

fn require_len(bytes: &[u8], len: usize) -> Result<(), Exp0002Error> {
    if bytes.len() < len {
        Err(Exp0002Error::Truncated)
    } else {
        Ok(())
    }
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

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn to_u64(value: usize) -> Result<u64, Exp0002Error> {
    u64::try_from(value).map_err(|_| Exp0002Error::ArithmeticOverflow)
}

fn to_usize(value: u64) -> Result<usize, Exp0002Error> {
    usize::try_from(value).map_err(|_| Exp0002Error::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> FileHeader {
        FileHeader {
            file_id: *b"exp0002-file-id!",
            creation_nonce: *b"fixed-nonce-0002",
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
    fn genesis_round_trip_is_deterministic() {
        let objects = vec![object(2, b"second", false), object(1, b"first", true)];
        let first = build_genesis(header(), objects.clone()).expect("genesis");
        let second = build_genesis(header(), objects).expect("genesis");
        assert_eq!(first, second);
        let report = validate_strict(&first, &ValidationLimits::default()).expect("valid");
        assert_eq!(report.snapshot.sequence, 0);
        assert_eq!(report.snapshot.roots, vec![1]);
        assert_eq!(report.objects.len(), 2);
        assert_eq!(report.pages_verified, 1);
    }

    #[test]
    fn multi_leaf_directory_round_trips() {
        let objects = (1..=500)
            .map(|id| object(id, &[u8::try_from(id % 251).expect("bounded")], id == 1))
            .collect();
        let bytes = build_genesis(header(), objects).expect("genesis");
        let report = validate_strict(&bytes, &ValidationLimits::default()).expect("valid");
        assert_eq!(report.objects.len(), 500);
        assert!(report.pages_verified >= 4);
        assert!(report.snapshot.directory_root_level >= 1);
    }

    #[test]
    fn append_reuses_old_objects_and_links_parent() {
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
        assert!(appended.starts_with(&genesis));
        let report = validate_strict(&appended, &ValidationLimits::default()).expect("valid");
        assert_eq!(report.snapshot.sequence, 1);
        assert_eq!(report.snapshot.roots, vec![1, 3]);
        assert_eq!(report.objects.len(), 3);
        assert_eq!(report.footer.previous_footer_offset, genesis.len() as u64 - FOOTER_LEN as u64);
        assert_eq!(report.footer.commit_start, genesis.len() as u64);
    }

    #[test]
    fn every_truncated_append_fails_strict_latest_validation() {
        let genesis = build_genesis(header(), vec![object(1, b"one", true)]).expect("genesis");
        let appended = build_append(
            &genesis,
            vec![object(2, b"two", false)],
            vec![1],
            &ValidationLimits::default(),
        )
        .expect("append");
        for cut in genesis.len() + 1..appended.len() {
            assert!(validate_strict(&appended[..cut], &ValidationLimits::default()).is_err());
        }
        assert!(validate_strict(&genesis, &ValidationLimits::default()).is_ok());
        assert!(validate_strict(&appended, &ValidationLimits::default()).is_ok());
    }

    #[test]
    fn object_page_snapshot_and_commit_mutations_fail_closed() {
        let bytes = build_genesis(header(), vec![object(1, b"payload", true)]).expect("genesis");
        let report = validate_strict(&bytes, &ValidationLimits::default()).expect("valid");

        let mut object_mutation = bytes.clone();
        let payload_offset = report.objects[0].record_offset as usize + OBJECT_HEADER_LEN;
        object_mutation[payload_offset] ^= 1;
        assert!(validate_strict(&object_mutation, &ValidationLimits::default()).is_err());

        let mut page_mutation = bytes.clone();
        page_mutation[report.snapshot.directory_root_offset as usize + PAGE_HEADER_LEN] ^= 1;
        assert!(validate_strict(&page_mutation, &ValidationLimits::default()).is_err());

        let mut snapshot_mutation = bytes.clone();
        snapshot_mutation[report.footer.snapshot_offset as usize + 8] ^= 1;
        assert!(validate_strict(&snapshot_mutation, &ValidationLimits::default()).is_err());

        let mut commit_mutation = bytes.clone();
        commit_mutation[24] ^= 1;
        assert!(validate_strict(&commit_mutation, &ValidationLimits::default()).is_err());
    }

    #[test]
    fn configured_limits_fail_closed() {
        let bytes = build_genesis(header(), vec![object(1, b"payload", true)]).expect("genesis");
        assert_eq!(
            validate_strict(
                &bytes,
                &ValidationLimits {
                    max_pages: 0,
                    ..ValidationLimits::default()
                }
            ),
            Err(Exp0002Error::ResourceLimit("pages"))
        );
        assert_eq!(
            validate_strict(
                &bytes,
                &ValidationLimits {
                    max_payload_bytes: 1,
                    ..ValidationLimits::default()
                }
            ),
            Err(Exp0002Error::ResourceLimit("payload bytes"))
        );
    }

    #[test]
    fn duplicate_and_invalid_roots_are_rejected() {
        assert_eq!(
            build_genesis(
                header(),
                vec![object(1, b"one", true), object(1, b"again", false)]
            ),
            Err(Exp0002Error::DuplicateObjectId(1))
        );
        assert_eq!(
            build_genesis(header(), vec![object(1, b"one", false)]),
            Err(Exp0002Error::NoRootObjects)
        );
    }
}
