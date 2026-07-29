use crate::format::{read_u32_le, read_u64_le, FOOTER_LEN, FOOTER_MAGIC, HEADER_LEN, RECORD_HEADER_LEN};
use crate::{DirectoryEntry, Error, IntegrityStatus, Limits, Manifest, MetadataInspector, ReadAt};
use sha2::{Digest, Sha256};

/// Work performed by a strict source-backed validation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceValidationStats {
    pub read_operations: u64,
    pub bytes_read: u64,
    pub bytes_hashed: u64,
    pub largest_allocation: u64,
}

/// A fully structured source report with verified committed-prefix integrity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceValidationReport {
    pub file_len: u64,
    pub epoch: u32,
    pub manifest_id: u64,
    pub manifest: Manifest,
    pub entries: Vec<DirectoryEntry>,
    pub unsupported_required_capabilities: Vec<u64>,
    pub integrity: IntegrityStatus,
    pub stats: SourceValidationStats,
}

impl SourceValidationReport {
    #[must_use]
    pub fn is_fully_interpretable(&self) -> bool {
        self.unsupported_required_capabilities.is_empty()
    }
}

/// Strict validator for a stable random-access source.
///
/// The source must present the same length and bytes for the duration of one
/// call. Structural metadata is validated first, then every committed-prefix
/// byte is read and hashed in bounded blocks. Footer fields are revalidated
/// against the structural report before a verified result is returned.
#[derive(Debug, Clone, Copy)]
pub struct SourceValidator {
    limits: Limits,
}

impl SourceValidator {
    #[must_use]
    pub const fn new(limits: Limits) -> Self {
        Self { limits }
    }

    pub fn validate<S: ReadAt>(&self, source: &mut S) -> Result<SourceValidationReport, Error> {
        let inspection = MetadataInspector::new(self.limits).inspect(source)?;
        let footer_len = u64::try_from(FOOTER_LEN).expect("fixed footer length");
        let footer_offset = inspection
            .file_len
            .checked_sub(footer_len)
            .ok_or(Error::Truncated("footer"))?;

        let initial_stats = SourceValidationStats {
            read_operations: inspection.stats.read_operations,
            bytes_read: inspection.stats.bytes_read,
            bytes_hashed: 0,
            largest_allocation: inspection.stats.largest_allocation,
        };
        let mut reader = ValidationReader::new(
            source,
            inspection.file_len,
            &self.limits,
            initial_stats,
        );

        let chunk_limit = self
            .limits
            .max_stream_chunk_bytes
            .min(self.limits.max_allocation_bytes);
        if chunk_limit == 0 {
            return Err(Error::LimitExceeded("hash chunk bytes"));
        }
        let chunk_capacity = usize::try_from(chunk_limit)
            .map_err(|_| Error::LimitExceeded("hash chunk bytes"))?;
        let mut buffer = vec![0_u8; chunk_capacity];
        reader.stats.largest_allocation = reader.stats.largest_allocation.max(chunk_limit);

        let mut hasher = Sha256::new();
        let mut offset = 0_u64;
        while offset < footer_offset {
            let remaining = footer_offset
                .checked_sub(offset)
                .ok_or(Error::RangeOutOfBounds("committed prefix"))?;
            let length = remaining.min(chunk_limit);
            let length_usize = usize::try_from(length)
                .map_err(|_| Error::LimitExceeded("hash chunk bytes"))?;
            reader.read_exact(offset, &mut buffer[..length_usize], "committed prefix")?;
            hasher.update(&buffer[..length_usize]);
            reader.stats.bytes_hashed = reader
                .stats
                .bytes_hashed
                .checked_add(length)
                .ok_or(Error::LimitExceeded("bytes hashed"))?;
            offset = offset
                .checked_add(length)
                .ok_or(Error::RangeOutOfBounds("committed prefix"))?;
        }

        let footer_bytes = reader.read_array::<FOOTER_LEN>(footer_offset, "footer")?;
        let footer = parse_footer(&footer_bytes)?;
        validate_footer_against_inspection(&footer, footer_offset, &inspection.entries, inspection.manifest_id)?;

        let actual_digest = hasher.finalize();
        if actual_digest.as_slice() != footer.digest {
            return Err(Error::DigestMismatch);
        }

        Ok(SourceValidationReport {
            file_len: inspection.file_len,
            epoch: inspection.epoch,
            manifest_id: inspection.manifest_id,
            unsupported_required_capabilities: inspection.unsupported_required_capabilities,
            manifest: inspection.manifest,
            entries: inspection.entries,
            integrity: IntegrityStatus::Verified,
            stats: reader.stats,
        })
    }
}

impl Default for SourceValidator {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

struct ValidationReader<'a, S> {
    source: &'a mut S,
    file_len: u64,
    limits: &'a Limits,
    stats: SourceValidationStats,
}

impl<'a, S: ReadAt> ValidationReader<'a, S> {
    fn new(
        source: &'a mut S,
        file_len: u64,
        limits: &'a Limits,
        stats: SourceValidationStats,
    ) -> Self {
        Self {
            source,
            file_len,
            limits,
            stats,
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

    fn read_exact(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
        context: &'static str,
    ) -> Result<(), Error> {
        let length =
            u64::try_from(buffer.len()).map_err(|_| Error::LimitExceeded("total bytes read"))?;
        let end = offset
            .checked_add(length)
            .ok_or(Error::RangeOutOfBounds(context))?;
        if end > self.file_len {
            return Err(Error::RangeOutOfBounds(context));
        }
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
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct ParsedFooter {
    directory_offset: u64,
    directory_len: u64,
    manifest_id: u64,
    record_count: u64,
    digest: [u8; 32],
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

fn validate_footer_against_inspection(
    footer: &ParsedFooter,
    footer_offset: u64,
    entries: &[DirectoryEntry],
    manifest_id: u64,
) -> Result<(), Error> {
    if footer.manifest_id != manifest_id {
        return Err(Error::DirectoryMismatch("footer manifest changed during validation"));
    }
    let expected_count = u64::try_from(entries.len())
        .map_err(|_| Error::LimitExceeded("record count"))?
        .checked_add(1)
        .ok_or(Error::LimitExceeded("record count"))?;
    if footer.record_count != expected_count {
        return Err(Error::InvalidLength("footer record count"));
    }

    let directory_offset = match entries.last() {
        Some(entry) => entry
            .offset
            .checked_add(u64::try_from(RECORD_HEADER_LEN).expect("fixed record header length"))
            .and_then(|value| value.checked_add(entry.stored_len))
            .ok_or(Error::RangeOutOfBounds("directory offset"))?,
        None => u64::try_from(HEADER_LEN).expect("fixed header length"),
    };
    if footer.directory_offset != directory_offset {
        return Err(Error::DirectoryMismatch("footer directory offset"));
    }
    let directory_len = footer_offset
        .checked_sub(directory_offset)
        .ok_or(Error::RangeOutOfBounds("directory length"))?;
    if footer.directory_len != directory_len {
        return Err(Error::DirectoryMismatch("footer directory length"));
    }
    Ok(())
}
