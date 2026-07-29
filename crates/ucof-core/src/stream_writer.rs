use crate::format::{
    push_u16_le, push_u32_le, push_u64_le, FILE_MAGIC, FOOTER_LEN, FOOTER_MAGIC, HEADER_LEN,
    RECORD_HEADER_LEN, RECORD_MAGIC,
};
use crate::model::text;
use crate::{
    encode_canonical, CborValue, DirectoryEntry, Error, Limits, Manifest, RecordKind,
    EXPERIMENTAL_EPOCH,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom, Write};

/// Finalized output and its exact physical length.
#[derive(Debug)]
pub struct FinishedWriter<W> {
    pub inner: W,
    pub bytes_written: u64,
}

/// Deterministic writer that publishes bytes to a non-seeking sink.
///
/// Record payload lengths are supplied before writing. The footer is emitted
/// only by [`StreamingWriter::finish`], after the selected manifest and
/// generated directory have been validated. Any write or input failure makes
/// the writer terminal and prevents root publication.
#[derive(Debug)]
pub struct StreamingWriter<W> {
    inner: W,
    limits: Limits,
    hasher: Sha256,
    entries: Vec<DirectoryEntry>,
    object_ids: BTreeSet<u64>,
    offset: u64,
    failed: bool,
}

impl<W: Write> StreamingWriter<W> {
    pub fn new(inner: W, limits: Limits) -> Result<Self, Error> {
        let mut writer = Self {
            inner,
            limits,
            hasher: Sha256::new(),
            entries: Vec::new(),
            object_ids: BTreeSet::new(),
            offset: 0,
            failed: false,
        };
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(&FILE_MAGIC);
        push_u32_le(&mut header, EXPERIMENTAL_EPOCH);
        push_u32_le(&mut header, 0);
        push_u32_le(
            &mut header,
            u32::try_from(HEADER_LEN).expect("fixed header length"),
        );
        header.extend_from_slice(&[0_u8; 12]);
        writer.write_hashed(&header, "file header")?;
        Ok(writer)
    }

    pub fn with_default_limits(inner: W) -> Result<Self, Error> {
        Self::new(inner, Limits::default())
    }

    #[must_use]
    pub fn bytes_written(&self) -> u64 {
        self.offset
    }

    pub fn add_opaque(&mut self, object_id: u64, payload: &[u8]) -> Result<(), Error> {
        let length = u64::try_from(payload.len())
            .map_err(|_| Error::InvalidLength("record payload"))?;
        self.begin_record(RecordKind::Opaque, object_id, length)?;
        if let Err(error) = self.write_hashed(payload, "opaque payload") {
            self.failed = true;
            return Err(error);
        }
        Ok(())
    }

    pub fn add_opaque_from_reader<R: Read>(
        &mut self,
        object_id: u64,
        length: u64,
        source: &mut R,
    ) -> Result<(), Error> {
        self.begin_record(RecordKind::Opaque, object_id, length)?;
        let chunk_limit = self
            .limits
            .max_stream_chunk_bytes
            .min(self.limits.max_allocation_bytes);
        if chunk_limit == 0 {
            self.failed = true;
            return Err(Error::LimitExceeded("stream chunk bytes"));
        }
        let capacity = usize::try_from(chunk_limit)
            .map_err(|_| Error::LimitExceeded("stream chunk bytes"))?;
        let mut buffer = vec![0_u8; capacity];
        let mut remaining = length;
        while remaining > 0 {
            let amount = remaining.min(chunk_limit);
            let amount = usize::try_from(amount)
                .map_err(|_| Error::LimitExceeded("stream chunk bytes"))?;
            if let Err(error) = source.read_exact(&mut buffer[..amount]) {
                self.failed = true;
                return Err(if error.kind() == std::io::ErrorKind::UnexpectedEof {
                    Error::Truncated("writer payload source")
                } else {
                    Error::Io("writer payload source")
                });
            }
            if let Err(error) = self.write_hashed(&buffer[..amount], "opaque payload") {
                self.failed = true;
                return Err(error);
            }
            remaining = remaining
                .checked_sub(u64::try_from(amount).expect("bounded chunk length"))
                .ok_or(Error::InvalidLength("writer payload"))?;
        }
        Ok(())
    }

    pub fn add_manifest(&mut self, object_id: u64, manifest: &Manifest) -> Result<(), Error> {
        manifest.validate_shape()?;
        let payload = encode_canonical(&manifest.to_cbor())?;
        let length = u64::try_from(payload.len())
            .map_err(|_| Error::InvalidLength("manifest payload"))?;
        self.begin_record(RecordKind::Manifest, object_id, length)?;
        if let Err(error) = self.write_hashed(&payload, "manifest payload") {
            self.failed = true;
            return Err(error);
        }
        Ok(())
    }

    pub fn finish(mut self, manifest_id: u64) -> Result<FinishedWriter<W>, Error> {
        self.ensure_open()?;
        let selected = self
            .entries
            .iter()
            .find(|entry| entry.id == manifest_id)
            .ok_or(Error::MissingManifest(manifest_id))?;
        if selected.kind != RecordKind::Manifest {
            return Err(Error::MissingManifest(manifest_id));
        }

        let directory = CborValue::Map(vec![(
            text("entries"),
            CborValue::Array(self.entries.iter().map(DirectoryEntry::to_cbor).collect()),
        )]);
        let directory_payload = encode_canonical(&directory)?;
        let directory_len = u64::try_from(directory_payload.len())
            .map_err(|_| Error::InvalidLength("directory payload"))?;
        if directory_len > self.limits.max_metadata_bytes {
            return Err(Error::LimitExceeded("directory metadata bytes"));
        }
        let directory_offset = self.offset;
        self.write_record_header(RecordKind::Directory, 0, directory_len)?;
        self.write_hashed(&directory_payload, "directory payload")?;
        let directory_total_len = u64::try_from(RECORD_HEADER_LEN)
            .expect("fixed record header length")
            .checked_add(directory_len)
            .ok_or(Error::InvalidLength("directory record"))?;
        let record_count = u64::try_from(self.entries.len())
            .map_err(|_| Error::InvalidLength("record count"))?
            .checked_add(1)
            .ok_or(Error::InvalidLength("record count"))?;
        if record_count > self.limits.max_records {
            return Err(Error::LimitExceeded("record count"));
        }

        let digest = self.hasher.clone().finalize();
        let mut footer = Vec::with_capacity(FOOTER_LEN);
        footer.extend_from_slice(&FOOTER_MAGIC);
        push_u32_le(
            &mut footer,
            u32::try_from(FOOTER_LEN).expect("fixed footer length"),
        );
        push_u32_le(&mut footer, 0);
        push_u64_le(&mut footer, directory_offset);
        push_u64_le(&mut footer, directory_total_len);
        push_u64_le(&mut footer, manifest_id);
        push_u64_le(&mut footer, record_count);
        footer.extend_from_slice(&digest);
        self.write_unhashed(&footer, "footer")?;

        Ok(FinishedWriter {
            inner: self.inner,
            bytes_written: self.offset,
        })
    }

    fn begin_record(
        &mut self,
        kind: RecordKind,
        object_id: u64,
        payload_len: u64,
    ) -> Result<(), Error> {
        self.ensure_open()?;
        if matches!(kind, RecordKind::Directory) {
            return Err(Error::InvalidRecordOrder(
                "directory is created during finalization",
            ));
        }
        if object_id == 0 {
            return Err(Error::InvalidMetadataSchema(
                "object identifier zero is reserved",
            ));
        }
        if payload_len > self.limits.max_payload_bytes {
            return Err(Error::LimitExceeded("record payload bytes"));
        }
        if u64::try_from(self.entries.len())
            .map_err(|_| Error::LimitExceeded("record count"))?
            >= self.limits.max_records.saturating_sub(1)
        {
            return Err(Error::LimitExceeded("record count"));
        }
        if !self.object_ids.insert(object_id) {
            return Err(Error::DuplicateObjectId(object_id));
        }
        let offset = self.offset;
        if let Err(error) = self.write_record_header(kind, object_id, payload_len) {
            self.failed = true;
            return Err(error);
        }
        self.entries.push(DirectoryEntry {
            id: object_id,
            kind,
            offset,
            stored_len: payload_len,
            logical_len: payload_len,
        });
        Ok(())
    }

    fn write_record_header(
        &mut self,
        kind: RecordKind,
        object_id: u64,
        payload_len: u64,
    ) -> Result<(), Error> {
        let mut header = Vec::with_capacity(RECORD_HEADER_LEN);
        header.extend_from_slice(&RECORD_MAGIC);
        push_u16_le(&mut header, u16::from(kind));
        push_u16_le(&mut header, 0);
        push_u32_le(
            &mut header,
            u32::try_from(RECORD_HEADER_LEN).expect("fixed record header length"),
        );
        push_u64_le(&mut header, payload_len);
        push_u64_le(&mut header, payload_len);
        push_u64_le(&mut header, object_id);
        push_u32_le(&mut header, 0);
        self.write_hashed(&header, "record header")
    }

    fn ensure_open(&self) -> Result<(), Error> {
        if self.failed {
            Err(Error::InvalidRecordOrder("writer used after failure"))
        } else {
            Ok(())
        }
    }

    fn write_hashed(&mut self, bytes: &[u8], context: &'static str) -> Result<(), Error> {
        self.write_checked(bytes, context)?;
        self.hasher.update(bytes);
        Ok(())
    }

    fn write_unhashed(&mut self, bytes: &[u8], context: &'static str) -> Result<(), Error> {
        self.write_checked(bytes, context)
    }

    fn write_checked(&mut self, bytes: &[u8], context: &'static str) -> Result<(), Error> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| Error::RangeOutOfBounds("writer length"))?;
        let next = self
            .offset
            .checked_add(length)
            .ok_or(Error::RangeOutOfBounds("writer offset"))?;
        if next > self.limits.max_file_bytes {
            return Err(Error::LimitExceeded("file bytes"));
        }
        self.inner
            .write_all(bytes)
            .map_err(|_| Error::Io(context))?;
        self.offset = next;
        Ok(())
    }
}

/// Seekable output mode with deterministic bytes and convenient readback.
#[derive(Debug)]
pub struct SeekableWriter<W> {
    inner: StreamingWriter<W>,
}

impl<W: Write + Seek> SeekableWriter<W> {
    pub fn new(mut sink: W, limits: Limits) -> Result<Self, Error> {
        let position = sink
            .stream_position()
            .map_err(|_| Error::Io("seekable writer position"))?;
        if position != 0 {
            return Err(Error::InvalidRecordOrder(
                "seekable output must begin at offset zero",
            ));
        }
        Ok(Self {
            inner: StreamingWriter::new(sink, limits)?,
        })
    }

    pub fn with_default_limits(sink: W) -> Result<Self, Error> {
        Self::new(sink, Limits::default())
    }

    pub fn add_opaque(&mut self, object_id: u64, payload: &[u8]) -> Result<(), Error> {
        self.inner.add_opaque(object_id, payload)
    }

    pub fn add_opaque_from_reader<R: Read>(
        &mut self,
        object_id: u64,
        length: u64,
        source: &mut R,
    ) -> Result<(), Error> {
        self.inner
            .add_opaque_from_reader(object_id, length, source)
    }

    pub fn add_manifest(&mut self, object_id: u64, manifest: &Manifest) -> Result<(), Error> {
        self.inner.add_manifest(object_id, manifest)
    }

    pub fn finish_and_rewind(self, manifest_id: u64) -> Result<FinishedWriter<W>, Error> {
        let mut finished = self.inner.finish(manifest_id)?;
        finished
            .inner
            .seek(SeekFrom::Start(0))
            .map_err(|_| Error::Io("rewind finalized output"))?;
        Ok(finished)
    }
}
