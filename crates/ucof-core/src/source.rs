use crate::format::{
    read_u16_le, read_u32_le, read_u64_le, FILE_MAGIC, FOOTER_LEN, FOOTER_MAGIC, HEADER_LEN,
    RECORD_HEADER_LEN, RECORD_MAGIC,
};
use crate::{decode_canonical, CborValue, DirectoryEntry, Error, Limits, Manifest, RecordKind};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Seek, SeekFrom};

/// Minimal synchronous random-access contract for hostile-input readers.
///
/// Implementations must either fill the complete buffer or return an I/O error.
/// The core library applies range and read-budget checks before calling this
/// method.
pub trait ReadAt {
    fn len(&mut self) -> io::Result<u64>;

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()>;

    fn is_empty(&mut self) -> io::Result<bool> {
        self.len().map(|length| length == 0)
    }
}

/// Zero-copy source over an existing byte slice.
#[derive(Debug, Clone, Copy)]
pub struct SliceSource<'a> {
    bytes: &'a [u8],
}

impl<'a> SliceSource<'a> {
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl ReadAt for SliceSource<'_> {
    fn len(&mut self) -> io::Result<u64> {
        u64::try_from(self.bytes.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "slice length cannot be represented as u64",
            )
        })
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        let start = usize::try_from(offset).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "source offset exceeds usize")
        })?;
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

/// Adapter for a seekable synchronous source.
#[derive(Debug)]
pub struct SeekSource<R> {
    inner: R,
}

impl<R> SeekSource<R> {
    #[must_use]
    pub const fn new(inner: R) -> Self {
        Self { inner }
    }

    #[must_use]
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read + Seek> ReadAt for SeekSource<R> {
    fn len(&mut self) -> io::Result<u64> {
        self.inner.seek(SeekFrom::End(0))
    }

    fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        self.inner.seek(SeekFrom::Start(offset))?;
        self.inner.read_exact(buffer)
    }
}

/// Work performed against a [`ReadAt`] source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadStats {
    pub read_operations: u64,
    pub bytes_read: u64,
    pub largest_allocation: u64,
}

/// Whether payload integrity was established by an inspection operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityStatus {
    /// Metadata-only inspection deliberately did not read and hash payload bodies.
    NotChecked,
}

/// Structurally validated metadata inventory that makes no payload-integrity claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectionReport {
    pub file_len: u64,
    pub epoch: u32,
    pub manifest_id: u64,
    pub manifest: Manifest,
    pub entries: Vec<DirectoryEntry>,
    pub unsupported_required_capabilities: Vec<u64>,
    pub integrity: IntegrityStatus,
    pub stats: ReadStats,
}

impl InspectionReport {
    #[must_use]
    pub fn is_fully_interpretable(&self) -> bool {
        self.unsupported_required_capabilities.is_empty()
    }
}

/// Metadata-only inspector for a bounded random-access source.
///
/// This API reads the bootstrap, footer, directory, every record header, and
/// the active manifest. It skips opaque payload bodies and therefore never
/// reports payload integrity as verified.
#[derive(Debug, Clone, Copy)]
pub struct MetadataInspector {
    limits: Limits,
}

impl MetadataInspector {
    #[must_use]
    pub const fn new(limits: Limits) -> Self {
        Self { limits }
    }

    pub fn inspect<S: ReadAt>(&self, source: &mut S) -> Result<InspectionReport, Error> {
        let file_len = source.len().map_err(|_| Error::Io("source length"))?;
        if file_len > self.limits.max_file_bytes {
            return Err(Error::LimitExceeded("file bytes"));
        }
        let minimum = u64::try_from(HEADER_LEN + FOOTER_LEN)
            .map_err(|_| Error::RangeOutOfBounds("minimum file length"))?;
        if file_len < minimum {
            return Err(Error::Truncated("file header or footer"));
        }

        let mut reader = BudgetedSource::new(source, file_len, &self.limits);
        let header = reader.read_array::<HEADER_LEN>(0, "file header")?;
        validate_header(&header)?;

        let footer_offset = file_len
            .checked_sub(u64::try_from(FOOTER_LEN).expect("fixed footer length"))
            .ok_or(Error::Truncated("footer"))?;
        let footer_bytes = reader.read_array::<FOOTER_LEN>(footer_offset, "footer")?;
        let footer = parse_footer(&footer_bytes)?;
        let directory_end = checked_end(
            footer.directory_offset,
            footer.directory_len,
            footer_offset,
            "directory",
        )?;
        if directory_end != footer_offset {
            return Err(Error::InvalidRecordOrder("directory must end at footer"));
        }
        if footer.directory_len
            < u64::try_from(RECORD_HEADER_LEN).expect("fixed record header length")
        {
            return Err(Error::InvalidLength("directory record"));
        }

        let directory_header_bytes = reader
            .read_array::<RECORD_HEADER_LEN>(footer.directory_offset, "directory record header")?;
        let directory_header = parse_record_header(
            &directory_header_bytes,
            footer.directory_offset,
            &self.limits,
        )?;
        if directory_header.kind != RecordKind::Directory || directory_header.object_id != 0 {
            return Err(Error::InvalidRecordOrder("invalid directory record"));
        }
        let directory_total = u64::try_from(RECORD_HEADER_LEN)
            .expect("fixed record header length")
            .checked_add(directory_header.stored_len)
            .ok_or(Error::RangeOutOfBounds("directory record"))?;
        if directory_total != footer.directory_len {
            return Err(Error::DirectoryMismatch("footer directory length"));
        }
        if directory_header.stored_len > self.limits.max_metadata_bytes {
            return Err(Error::LimitExceeded("directory metadata bytes"));
        }

        let directory_payload = reader.read_vec(
            directory_header.payload_offset,
            directory_header.stored_len,
            "directory payload",
        )?;
        let directory_value = decode_canonical(&directory_payload, &self.limits)?;
        let entries = parse_directory(&directory_value)?;
        let expected_record_count = u64::try_from(entries.len())
            .map_err(|_| Error::LimitExceeded("record count"))?
            .checked_add(1)
            .ok_or(Error::LimitExceeded("record count"))?;
        if expected_record_count != footer.record_count {
            return Err(Error::InvalidLength("footer record count"));
        }
        if expected_record_count > self.limits.max_records {
            return Err(Error::LimitExceeded("record count"));
        }

        validate_inventory_headers(&mut reader, &entries, footer.directory_offset, &self.limits)?;

        let manifest_entry = entries
            .iter()
            .find(|entry| entry.id == footer.manifest_id)
            .ok_or(Error::MissingManifest(footer.manifest_id))?;
        if manifest_entry.kind != RecordKind::Manifest {
            return Err(Error::MissingManifest(footer.manifest_id));
        }
        if manifest_entry.stored_len > self.limits.max_metadata_bytes {
            return Err(Error::LimitExceeded("manifest metadata bytes"));
        }
        let manifest_payload_offset = manifest_entry
            .offset
            .checked_add(u64::try_from(RECORD_HEADER_LEN).expect("fixed record header length"))
            .ok_or(Error::RangeOutOfBounds("manifest payload"))?;
        let manifest_payload = reader.read_vec(
            manifest_payload_offset,
            manifest_entry.stored_len,
            "manifest payload",
        )?;
        let manifest_value = decode_canonical(&manifest_payload, &self.limits)?;
        let manifest = parse_manifest(&manifest_value)?;

        let available: BTreeSet<u64> = entries.iter().map(|entry| entry.id).collect();
        for root in &manifest.roots {
            if !available.contains(root) {
                return Err(Error::InvalidMetadataSchema("manifest root does not exist"));
            }
        }

        Ok(InspectionReport {
            file_len,
            epoch: crate::EXPERIMENTAL_EPOCH,
            manifest_id: footer.manifest_id,
            unsupported_required_capabilities: manifest.required_capabilities.clone(),
            manifest,
            entries,
            integrity: IntegrityStatus::NotChecked,
            stats: reader.stats,
        })
    }
}

impl Default for MetadataInspector {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

struct BudgetedSource<'a, S> {
    source: &'a mut S,
    file_len: u64,
    limits: &'a Limits,
    stats: ReadStats,
}

impl<'a, S: ReadAt> BudgetedSource<'a, S> {
    fn new(source: &'a mut S, file_len: u64, limits: &'a Limits) -> Self {
        Self {
            source,
            file_len,
            limits,
            stats: ReadStats::default(),
        }
    }

    fn read_array<const N: usize>(
        &mut self,
        offset: u64,
        context: &'static str,
    ) -> Result<[u8; N], Error> {
        let mut bytes = [0_u8; N];
        self.read_exact(offset, &mut bytes, context)?;
        Ok(bytes)
    }

    fn read_vec(
        &mut self,
        offset: u64,
        length: u64,
        context: &'static str,
    ) -> Result<Vec<u8>, Error> {
        if length > self.limits.max_allocation_bytes {
            return Err(Error::LimitExceeded("single allocation bytes"));
        }
        let length_usize =
            usize::try_from(length).map_err(|_| Error::LimitExceeded("single allocation bytes"))?;
        let mut bytes = vec![0_u8; length_usize];
        self.read_exact(offset, &mut bytes, context)?;
        Ok(bytes)
    }

    fn read_exact(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
        context: &'static str,
    ) -> Result<(), Error> {
        let length =
            u64::try_from(buffer.len()).map_err(|_| Error::LimitExceeded("total bytes read"))?;
        checked_end(offset, length, self.file_len, context)?;
        let next_total = self
            .stats
            .bytes_read
            .checked_add(length)
            .ok_or(Error::LimitExceeded("total bytes read"))?;
        if next_total > self.limits.max_total_bytes_read {
            return Err(Error::LimitExceeded("total bytes read"));
        }
        self.source
            .read_exact_at(offset, buffer)
            .map_err(|_| Error::Io(context))?;
        self.stats.bytes_read = next_total;
        self.stats.read_operations = self
            .stats
            .read_operations
            .checked_add(1)
            .ok_or(Error::LimitExceeded("read operations"))?;
        self.stats.largest_allocation = self.stats.largest_allocation.max(length);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct Footer {
    directory_offset: u64,
    directory_len: u64,
    manifest_id: u64,
    record_count: u64,
}

#[derive(Debug, Clone, Copy)]
struct RecordHeader {
    kind: RecordKind,
    object_id: u64,
    stored_len: u64,
    logical_len: u64,
    payload_offset: u64,
}

fn validate_header(bytes: &[u8; HEADER_LEN]) -> Result<(), Error> {
    if bytes[..FILE_MAGIC.len()] != FILE_MAGIC {
        return Err(Error::InvalidMagic("file"));
    }
    let epoch = read_u32_le(bytes, 8, "epoch")?;
    if epoch != crate::EXPERIMENTAL_EPOCH {
        return Err(Error::UnsupportedEpoch(epoch));
    }
    let flags = read_u32_le(bytes, 12, "file flags")?;
    if flags != 0 {
        return Err(Error::UnsupportedFlags("file", u64::from(flags)));
    }
    if read_u32_le(bytes, 16, "file header length")?
        != u32::try_from(HEADER_LEN).expect("fixed header length")
    {
        return Err(Error::InvalidLength("file header"));
    }
    if bytes[20..].iter().any(|byte| *byte != 0) {
        return Err(Error::InvalidReserved("file header"));
    }
    Ok(())
}

fn parse_footer(bytes: &[u8; FOOTER_LEN]) -> Result<Footer, Error> {
    if bytes[..FOOTER_MAGIC.len()] != FOOTER_MAGIC {
        return Err(Error::InvalidMagic("footer"));
    }
    if read_u32_le(bytes, 8, "footer length")?
        != u32::try_from(FOOTER_LEN).expect("fixed footer length")
    {
        return Err(Error::InvalidLength("footer"));
    }
    let flags = read_u32_le(bytes, 12, "footer flags")?;
    if flags != 0 {
        return Err(Error::UnsupportedFlags("footer", u64::from(flags)));
    }
    Ok(Footer {
        directory_offset: read_u64_le(bytes, 16, "directory offset")?,
        directory_len: read_u64_le(bytes, 24, "directory length")?,
        manifest_id: read_u64_le(bytes, 32, "manifest identifier")?,
        record_count: read_u64_le(bytes, 40, "record count")?,
    })
}

fn parse_record_header(
    bytes: &[u8; RECORD_HEADER_LEN],
    offset: u64,
    limits: &Limits,
) -> Result<RecordHeader, Error> {
    if bytes[..RECORD_MAGIC.len()] != RECORD_MAGIC {
        return Err(Error::InvalidMagic("record"));
    }
    let kind = RecordKind::try_from(read_u16_le(bytes, 4, "record kind")?)?;
    let flags = read_u16_le(bytes, 6, "record flags")?;
    if flags != 0 {
        return Err(Error::UnsupportedFlags("record", u64::from(flags)));
    }
    if read_u32_le(bytes, 8, "record header length")?
        != u32::try_from(RECORD_HEADER_LEN).expect("fixed record header length")
    {
        return Err(Error::InvalidLength("record header"));
    }
    let stored_len = read_u64_le(bytes, 12, "stored length")?;
    let logical_len = read_u64_le(bytes, 20, "logical length")?;
    if stored_len != logical_len {
        return Err(Error::InvalidLength("transformed logical length"));
    }
    if stored_len > limits.max_payload_bytes {
        return Err(Error::LimitExceeded("record payload bytes"));
    }
    let object_id = read_u64_le(bytes, 28, "object identifier")?;
    if read_u32_le(bytes, 36, "record reserved")? != 0 {
        return Err(Error::InvalidReserved("record header"));
    }
    let payload_offset = offset
        .checked_add(u64::try_from(RECORD_HEADER_LEN).expect("fixed record header length"))
        .ok_or(Error::RangeOutOfBounds("record payload"))?;
    Ok(RecordHeader {
        kind,
        object_id,
        stored_len,
        logical_len,
        payload_offset,
    })
}

fn validate_inventory_headers<S: ReadAt>(
    reader: &mut BudgetedSource<'_, S>,
    entries: &[DirectoryEntry],
    directory_offset: u64,
    limits: &Limits,
) -> Result<(), Error> {
    let mut expected_offset = u64::try_from(HEADER_LEN).expect("fixed header length");
    let mut identifiers = BTreeSet::new();
    for entry in entries {
        if entry.id == 0 || !identifiers.insert(entry.id) {
            return Err(Error::DuplicateObjectId(entry.id));
        }
        if entry.offset != expected_offset {
            return Err(Error::DirectoryMismatch("non-contiguous record offset"));
        }
        let header_bytes =
            reader.read_array::<RECORD_HEADER_LEN>(entry.offset, "inventory record header")?;
        let header = parse_record_header(&header_bytes, entry.offset, limits)?;
        if header.kind == RecordKind::Directory || header.object_id == 0 {
            return Err(Error::InvalidRecordOrder(
                "directory entry describes structural record",
            ));
        }
        if header.kind != entry.kind
            || header.object_id != entry.id
            || header.stored_len != entry.stored_len
            || header.logical_len != entry.logical_len
        {
            return Err(Error::DirectoryMismatch(
                "entry does not match record header",
            ));
        }
        expected_offset = checked_end(
            header.payload_offset,
            header.stored_len,
            directory_offset,
            "inventory record payload",
        )?;
    }
    if expected_offset != directory_offset {
        return Err(Error::DirectoryMismatch(
            "inventory does not end at directory",
        ));
    }
    Ok(())
}

fn checked_end(
    offset: u64,
    length: u64,
    upper_bound: u64,
    context: &'static str,
) -> Result<u64, Error> {
    let end = offset
        .checked_add(length)
        .ok_or(Error::RangeOutOfBounds(context))?;
    if end > upper_bound {
        return Err(Error::RangeOutOfBounds(context));
    }
    Ok(end)
}

fn parse_directory(value: &CborValue) -> Result<Vec<DirectoryEntry>, Error> {
    let map = exact_map(value, &["entries"], "directory")?;
    let entries = map
        .get("entries")
        .ok_or(Error::InvalidMetadataSchema("directory entries"))?;
    let CborValue::Array(entries) = entries else {
        return Err(Error::InvalidMetadataSchema(
            "directory entries must be an array",
        ));
    };
    entries.iter().map(parse_directory_entry).collect()
}

fn parse_directory_entry(value: &CborValue) -> Result<DirectoryEntry, Error> {
    let map = exact_map(
        value,
        &["id", "kind", "offset", "stored_len", "logical_len"],
        "directory entry",
    )?;
    let raw_kind = unsigned(map.get("kind"), "directory kind")?;
    let raw_kind = u16::try_from(raw_kind)
        .map_err(|_| Error::InvalidMetadataSchema("directory kind range"))?;
    Ok(DirectoryEntry {
        id: unsigned(map.get("id"), "directory id")?,
        kind: RecordKind::try_from(raw_kind)?,
        offset: unsigned(map.get("offset"), "directory offset")?,
        stored_len: unsigned(map.get("stored_len"), "directory stored length")?,
        logical_len: unsigned(map.get("logical_len"), "directory logical length")?,
    })
}

fn parse_manifest(value: &CborValue) -> Result<Manifest, Error> {
    let map = exact_map(value, &["roots", "required", "optional"], "manifest")?;
    let manifest = Manifest {
        roots: unsigned_array(map.get("roots"), "manifest roots")?,
        required_capabilities: unsigned_array(
            map.get("required"),
            "manifest required capabilities",
        )?,
        optional_capabilities: unsigned_array(
            map.get("optional"),
            "manifest optional capabilities",
        )?,
    };
    manifest.validate_shape()?;
    Ok(manifest)
}

fn exact_map<'a>(
    value: &'a CborValue,
    expected: &[&str],
    context: &'static str,
) -> Result<BTreeMap<&'a str, &'a CborValue>, Error> {
    let CborValue::Map(entries) = value else {
        return Err(Error::InvalidMetadataSchema(context));
    };
    if entries.len() != expected.len() {
        return Err(Error::InvalidMetadataSchema(context));
    }
    let mut map = BTreeMap::new();
    for (key, value) in entries {
        let CborValue::Text(key) = key else {
            return Err(Error::InvalidMetadataSchema(
                "metadata map key must be text",
            ));
        };
        if !expected.contains(&key.as_str()) || map.insert(key.as_str(), value).is_some() {
            return Err(Error::InvalidMetadataSchema(context));
        }
    }
    Ok(map)
}

fn unsigned(value: Option<&&CborValue>, context: &'static str) -> Result<u64, Error> {
    match value {
        Some(CborValue::Unsigned(value)) => Ok(*value),
        _ => Err(Error::InvalidMetadataSchema(context)),
    }
}

fn unsigned_array(value: Option<&&CborValue>, context: &'static str) -> Result<Vec<u64>, Error> {
    let Some(CborValue::Array(values)) = value else {
        return Err(Error::InvalidMetadataSchema(context));
    };
    values
        .iter()
        .map(|value| match value {
            CborValue::Unsigned(value) => Ok(*value),
            _ => Err(Error::InvalidMetadataSchema(context)),
        })
        .collect()
}
