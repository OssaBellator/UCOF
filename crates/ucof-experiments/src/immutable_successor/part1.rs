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
    pub max_history_entries: usize,
    pub max_recovery_scan_bytes: usize,
    pub max_recovery_attempts: usize,
    pub max_recovery_candidates: usize,
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
            max_history_entries: 1_024,
            max_recovery_scan_bytes: 4 * 1024 * 1024,
            max_recovery_attempts: 4_096,
            max_recovery_candidates: 64,
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
pub struct ImmutableHistoryEntry {
    pub footer_offset: u64,
    pub report: ImmutableReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableHistoryReport {
    /// Strictly validated entries ordered from newest to oldest.
    pub entries: Vec<ImmutableHistoryEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableRecoveryCandidate {
    pub footer_offset: u64,
    pub prefix_len: u64,
    pub report: ImmutableReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableRecoveryReport {
    pub scan_start: u64,
    pub scanned_bytes: usize,
    pub attempted_footers: usize,
    pub attempts_truncated: bool,
    pub candidates_truncated: bool,
    /// Strictly validated prefixes ordered from newest to oldest. No candidate is selected.
    pub candidates: Vec<ImmutableRecoveryCandidate>,
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
