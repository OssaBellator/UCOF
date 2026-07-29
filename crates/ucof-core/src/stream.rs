use crate::format::{
    read_u16_le, read_u32_le, read_u64_le, FILE_MAGIC, FOOTER_LEN, FOOTER_MAGIC, HEADER_LEN,
    RECORD_HEADER_LEN, RECORD_MAGIC,
};
use crate::{decode_canonical, CborValue, DirectoryEntry, Error, IntegrityStatus, Limits, Manifest, RecordKind};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};

/// Work performed by a [`SequentialReader`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamStats {
    pub read_operations: u64,
    pub bytes_read: u64,
    pub bytes_hashed: u64,
    pub logical_bytes: u64,
    pub payload_chunks: u64,
    pub largest_allocation: u64,
}

/// Physical record information emitted before its payload chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamRecord {
    pub kind: RecordKind,
    pub object_id: u64,
    pub offset: u64,
    pub stored_len: u64,
    pub logical_len: u64,
}

/// A verified final commit reached through sequential reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamCommit {
    pub manifest_id: u64,
    pub record_count: u64,
    pub roots: Vec<u64>,
    pub unsupported_required_capabilities: Vec<u64>,
    pub integrity: IntegrityStatus,
    pub stats: StreamStats,
}

impl StreamCommit {
    #[must_use]
    pub fn is_fully_interpretable(&self) -> bool {
        self.unsupported_required_capabilities.is_empty()
    }
}

/// Events returned by [`SequentialReader::next_event`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    FileHeader { epoch: u32 },
    RecordStart(StreamRecord),
    PayloadChunk {
        kind: RecordKind,
        object_id: u64,
        bytes: Vec<u8>,
        remaining: u64,
    },
    RecordEnd { kind: RecordKind, object_id: u64 },
    Commit(StreamCommit),
}

/// Bounded, non-seeking reader for stream-compatible `UCOF-EXP-0001` files.
///
/// Opaque payloads are emitted in chunks no larger than
/// [`Limits::max_stream_chunk_bytes`]. Manifest and directory payloads are
/// retained only after their declared lengths pass metadata and allocation
/// limits. The committed prefix is hashed incrementally, and a commit event is
/// emitted only after footer, digest, directory, manifest, and exact-end checks
/// succeed.
#[derive(Debug)]
pub struct SequentialReader<R> {
    inner: R,
    limits: Limits,
    state: State,
    offset: u64,
    stats: StreamStats,
    hasher: Sha256,
    active: Option<ActiveRecord>,
    inventory: Vec<DirectoryEntry>,
    identifiers: BTreeSet<u64>,
    manifests: BTreeMap<u64, Manifest>,
    directory: Option<DirectoryState>,
    record_count: u64,
}

impl<R: Read> SequentialReader<R> {
    #[must_use]
    pub fn new(inner: R, limits: Limits) -> Self {
        Self {
            inner,
            limits,
            state: State::Header,
            offset: 0,
            stats: StreamStats::default(),
            hasher: Sha256::new(),
            active: None,
            inventory: Vec::new(),
            identifiers: BTreeSet::new(),
            manifests: BTreeMap::new(),
            directory: None,
            record_count: 0,
        }
    }

    #[must_use]
    pub fn with_default_limits(inner: R) -> Self {
        Self::new(inner, Limits::default())
    }

    #[must_use]
    pub fn stats(&self) -> StreamStats {
        self.stats
    }

    #[must_use]
    pub fn into_inner(self) -> R {
        self.inner
    }

    pub fn next_event(&mut self) -> Result<Option<StreamEvent>, Error> {
        match self.state {
            State::Header => self.read_header().map(Some),
            State::RecordHeader => self.read_record_header().map(Some),
            State::Payload => self.read_payload_event().map(Some),
            State::RecordEnd => self.finish_record().map(Some),
            State::Footer => self.read_footer().map(Some),
            State::Done => Ok(None),
            State::Failed => Err(Error::InvalidRecordOrder("reader used after failure")),
        }
        .inspect_err(|_| self.state = State::Failed)
    }

    fn read_header(&mut self) -> Result<StreamEvent, Error> {
        let bytes = self.read_exact_array::<HEADER_LEN>("file header", true)?;
        validate_header(&bytes)?;
        self.state = State::RecordHeader;
        Ok(StreamEvent::FileHeader {
            epoch: crate::EXPERIMENTAL_EPOCH,
        })
    }

    fn read_record_header(&mut self) -> Result<StreamEvent, Error> {
        if self.directory.is_some() {
            return Err(Error::InvalidRecordOrder("record follows directory"));
        }
        self.record_count = self
            .record_count
            .checked_add(1)
            .ok_or(Error::LimitExceeded("record count"))?;
        if self.record_count > self.limits.max_records {
            return Err(Error::LimitExceeded("record count"));
        }

        let record_offset = self.offset;
        let bytes = self.read_exact_array::<RECORD_HEADER_LEN>("record header", true)?;
        let header = parse_record_header(&bytes, record_offset, &self.limits)?;

        if header.kind == RecordKind::Directory {
            if header.object_id != 0 {
                return Err(Error::InvalidRecordOrder("directory identifier must be zero"));
            }
        } else if header.object_id == 0 {
            return Err(Error::InvalidRecordOrder("non-directory identifier is zero"));
        } else if !self.identifiers.insert(header.object_id) {
            return Err(Error::DuplicateObjectId(header.object_id));
        }

        let metadata = match header.kind {
            RecordKind::Manifest | RecordKind::Directory => {
                if header.stored_len > self.limits.max_metadata_bytes {
                    return Err(Error::LimitExceeded("record metadata bytes"));
                }
                if header.stored_len > self.limits.max_allocation_bytes {
                    return Err(Error::LimitExceeded("single allocation bytes"));
                }
                let capacity = usize::try_from(header.stored_len)
                    .map_err(|_| Error::LimitExceeded("single allocation bytes"))?;
                self.stats.largest_allocation = self.stats.largest_allocation.max(header.stored_len);
                Some(Vec::with_capacity(capacity))
            }
            RecordKind::Opaque => None,
        };

        let record = StreamRecord {
            kind: header.kind,
            object_id: header.object_id,
            offset: record_offset,
            stored_len: header.stored_len,
            logical_len: header.logical_len,
        };
        if record.kind != RecordKind::Directory {
            self.inventory.push(DirectoryEntry {
                id: record.object_id,
                kind: record.kind,
                offset: record.offset,
                stored_len: record.stored_len,
                logical_len: record.logical_len,
            });
        }

        self.active = Some(ActiveRecord {
            record,
            remaining: header.stored_len,
            metadata,
        });
        self.state = if header.stored_len == 0 {
            State::RecordEnd
        } else {
            State::Payload
        };
        Ok(StreamEvent::RecordStart(record))
    }

    fn read_payload_event(&mut self) -> Result<StreamEvent, Error> {
        let active = self
            .active
            .as_ref()
            .ok_or(Error::InvalidRecordOrder("missing active record"))?;
        if active.remaining == 0 {
            self.state = State::RecordEnd;
            return self.finish_record();
        }

        let chunk_limit = self
            .limits
            .max_stream_chunk_bytes
            .min(self.limits.max_allocation_bytes);
        if chunk_limit == 0 {
            return Err(Error::LimitExceeded("stream chunk bytes"));
        }
        let chunk_len = active.remaining.min(chunk_limit);
        let chunk_len_usize = usize::try_from(chunk_len)
            .map_err(|_| Error::LimitExceeded("stream chunk bytes"))?;
        let mut bytes = vec![0_u8; chunk_len_usize];
        self.stats.largest_allocation = self.stats.largest_allocation.max(chunk_len);
        self.read_exact_into(&mut bytes, "record payload", true)?;

        let active = self
            .active
            .as_mut()
            .ok_or(Error::InvalidRecordOrder("missing active record"))?;
        active.remaining = active
            .remaining
            .checked_sub(chunk_len)
            .ok_or(Error::InvalidLength("record payload"))?;
        if let Some(metadata) = &mut active.metadata {
            metadata.extend_from_slice(&bytes);
        }

        self.stats.logical_bytes = self
            .stats
            .logical_bytes
            .checked_add(chunk_len)
            .ok_or(Error::LimitExceeded("logical decoded bytes"))?;
        if self.stats.logical_bytes > self.limits.max_logical_decoded_bytes {
            return Err(Error::LimitExceeded("logical decoded bytes"));
        }
        self.stats.payload_chunks = self
            .stats
            .payload_chunks
            .checked_add(1)
            .ok_or(Error::LimitExceeded("payload chunks"))?;

        let event = StreamEvent::PayloadChunk {
            kind: active.record.kind,
            object_id: active.record.object_id,
            bytes,
            remaining: active.remaining,
        };
        if active.remaining == 0 {
            self.state = State::RecordEnd;
        }
        Ok(event)
    }

    fn finish_record(&mut self) -> Result<StreamEvent, Error> {
        let active = self
            .active
            .take()
            .ok_or(Error::InvalidRecordOrder("missing active record"))?;
        if active.remaining != 0 {
            return Err(Error::InvalidLength("unfinished record payload"));
        }

        match active.record.kind {
            RecordKind::Opaque => {}
            RecordKind::Manifest => {
                let metadata = active
                    .metadata
                    .ok_or(Error::InvalidMetadataSchema("missing manifest payload"))?;
                let value = decode_canonical(&metadata, &self.limits)?;
                let manifest = parse_manifest(&value)?;
                self.manifests.insert(active.record.object_id, manifest);
            }
            RecordKind::Directory => {
                let metadata = active
                    .metadata
                    .ok_or(Error::InvalidMetadataSchema("missing directory payload"))?;
                let value = decode_canonical(&metadata, &self.limits)?;
                let entries = parse_directory(&value)?;
                if entries != self.inventory {
                    return Err(Error::DirectoryMismatch("entries do not match streamed records"));
                }
                let record_len = u64::try_from(RECORD_HEADER_LEN)
                    .expect("fixed record header length")
                    .checked_add(active.record.stored_len)
                    .ok_or(Error::RangeOutOfBounds("directory record"))?;
                self.directory = Some(DirectoryState {
                    offset: active.record.offset,
                    len: record_len,
                });
            }
        }

        self.state = if active.record.kind == RecordKind::Directory {
            State::Footer
        } else {
            State::RecordHeader
        };
        Ok(StreamEvent::RecordEnd {
            kind: active.record.kind,
            object_id: active.record.object_id,
        })
    }

    fn read_footer(&mut self) -> Result<StreamEvent, Error> {
        let footer_bytes = self.read_exact_array::<FOOTER_LEN>("footer", false)?;
        let footer = parse_footer(&footer_bytes)?;
        let directory = self
            .directory
            .ok_or(Error::InvalidRecordOrder("missing directory record"))?;
        if footer.directory_offset != directory.offset || footer.directory_len != directory.len {
            return Err(Error::DirectoryMismatch("footer location"));
        }
        if footer.record_count != self.record_count {
            return Err(Error::InvalidLength("footer record count"));
        }

        let actual_digest = self.hasher.clone().finalize();
        if actual_digest.as_slice() != footer.digest {
            return Err(Error::DigestMismatch);
        }

        let manifest = self
            .manifests
            .get(&footer.manifest_id)
            .ok_or(Error::MissingManifest(footer.manifest_id))?;
        let available: BTreeSet<u64> = self.inventory.iter().map(|entry| entry.id).collect();
        for root in &manifest.roots {
            if !available.contains(root) {
                return Err(Error::InvalidMetadataSchema("manifest root does not exist"));
            }
        }

        self.ensure_exact_end()?;
        self.state = State::Done;
        Ok(StreamEvent::Commit(StreamCommit {
            manifest_id: footer.manifest_id,
            record_count: footer.record_count,
            roots: manifest.roots.clone(),
            unsupported_required_capabilities: manifest.required_capabilities.clone(),
            integrity: IntegrityStatus::Verified,
            stats: self.stats,
        }))
    }

    fn ensure_exact_end(&mut self) -> Result<(), Error> {
        self.check_file_budget(1)?;
        let mut byte = [0_u8; 1];
        loop {
            self.stats.read_operations = self
                .stats
                .read_operations
                .checked_add(1)
                .ok_or(Error::LimitExceeded("read operations"))?;
            match self.inner.read(&mut byte) {
                Ok(0) => return Ok(()),
                Ok(_) => {
                    self.stats.bytes_read = self
                        .stats
                        .bytes_read
                        .checked_add(1)
                        .ok_or(Error::LimitExceeded("total bytes read"))?;
                    return Err(Error::InvalidRecordOrder("trailing bytes after footer"));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Error::Io("exact-end check")),
            }
        }
    }

    fn read_exact_array<const N: usize>(
        &mut self,
        context: &'static str,
        hash: bool,
    ) -> Result<[u8; N], Error> {
        let mut bytes = [0_u8; N];
        self.read_exact_into(&mut bytes, context, hash)?;
        Ok(bytes)
    }

    fn read_exact_into(
        &mut self,
        bytes: &mut [u8],
        context: &'static str,
        hash: bool,
    ) -> Result<(), Error> {
        let length = u64::try_from(bytes.len()).map_err(|_| Error::LimitExceeded("total bytes read"))?;
        self.check_file_budget(length)?;
        let next_total = self
            .stats
            .bytes_read
            .checked_add(length)
            .ok_or(Error::LimitExceeded("total bytes read"))?;
        if next_total > self.limits.max_total_bytes_read {
            return Err(Error::LimitExceeded("total bytes read"));
        }

        self.inner.read_exact(bytes).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                Error::Truncated(context)
            } else {
                Error::Io(context)
            }
        })?;
        self.offset = self
            .offset
            .checked_add(length)
            .ok_or(Error::RangeOutOfBounds(context))?;
        self.stats.bytes_read = next_total;
        self.stats.read_operations = self
            .stats
            .read_operations
            .checked_add(1)
            .ok_or(Error::LimitExceeded("read operations"))?;
        if hash {
            self.hasher.update(bytes);
            self.stats.bytes_hashed = self
                .stats
                .bytes_hashed
                .checked_add(length)
                .ok_or(Error::LimitExceeded("bytes hashed"))?;
        }
        Ok(())
    }

    fn check_file_budget(&self, length: u64) -> Result<(), Error> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(Error::RangeOutOfBounds("stream offset"))?;
        if end > self.limits.max_file_bytes {
            return Err(Error::LimitExceeded("file bytes"));
        }
        Ok(())
    }
}

impl<R: Read> Iterator for SequentialReader<R> {
    type Item = Result<StreamEvent, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_event() {
            Ok(Some(event)) => Some(Ok(event)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Header,
    RecordHeader,
    Payload,
    RecordEnd,
    Footer,
    Done,
    Failed,
}

#[derive(Debug)]
struct ActiveRecord {
    record: StreamRecord,
    remaining: u64,
    metadata: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy)]
struct DirectoryState {
    offset: u64,
    len: u64,
}

#[derive(Debug, Clone, Copy)]
struct ParsedRecordHeader {
    kind: RecordKind,
    object_id: u64,
    stored_len: u64,
    logical_len: u64,
}

#[derive(Debug, Clone, Copy)]
struct ParsedFooter {
    directory_offset: u64,
    directory_len: u64,
    manifest_id: u64,
    record_count: u64,
    digest: [u8; 32],
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

fn parse_record_header(
    bytes: &[u8; RECORD_HEADER_LEN],
    _offset: u64,
    limits: &Limits,
) -> Result<ParsedRecordHeader, Error> {
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
    Ok(ParsedRecordHeader {
        kind,
        object_id,
        stored_len,
        logical_len,
    })
}

fn parse_footer(bytes: &[u8; FOOTER_LEN]) -> Result<ParsedFooter, Error> {
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
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(&bytes[48..80]);
    Ok(ParsedFooter {
        directory_offset: read_u64_le(bytes, 16, "directory offset")?,
        directory_len: read_u64_le(bytes, 24, "directory length")?,
        manifest_id: read_u64_le(bytes, 32, "manifest identifier")?,
        record_count: read_u64_le(bytes, 40, "record count")?,
        digest,
    })
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
