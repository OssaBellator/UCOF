use crate::format::{
    checked_range, read_u16_le, read_u32_le, read_u64_le, take, FILE_MAGIC, FOOTER_LEN,
    FOOTER_MAGIC, HEADER_LEN, RECORD_HEADER_LEN, RECORD_MAGIC,
};
use crate::{
    decode_canonical, CborValue, DirectoryEntry, Error, Limits, Manifest, RecordKind,
    EXPERIMENTAL_EPOCH,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordInfo {
    pub kind: RecordKind,
    pub object_id: u64,
    pub offset: u64,
    pub stored_len: u64,
    pub logical_len: u64,
    payload_range: Range<usize>,
}

#[derive(Debug)]
pub struct ValidatedFile<'a> {
    bytes: &'a [u8],
    pub epoch: u32,
    pub manifest_id: u64,
    pub manifest: Manifest,
    pub records: Vec<RecordInfo>,
    by_id: BTreeMap<u64, usize>,
}

impl<'a> ValidatedFile<'a> {
    pub fn parse(bytes: &'a [u8], limits: &Limits) -> Result<Self, Error> {
        let file_len =
            u64::try_from(bytes.len()).map_err(|_| Error::LimitExceeded("file bytes"))?;
        if file_len > limits.max_file_bytes {
            return Err(Error::LimitExceeded("file bytes"));
        }
        if bytes.len() < HEADER_LEN + FOOTER_LEN {
            return Err(Error::Truncated("file header or footer"));
        }

        validate_header(bytes)?;
        let footer_offset = bytes.len() - FOOTER_LEN;
        let footer = parse_footer(bytes, footer_offset)?;
        let directory_range = checked_range(
            footer.directory_offset,
            footer.directory_len,
            footer_offset,
            "directory",
        )?;
        if directory_range.end != footer_offset {
            return Err(Error::InvalidRecordOrder("directory must end at footer"));
        }

        let records = scan_records(bytes, footer_offset, limits)?;
        if u64::try_from(records.len()).map_err(|_| Error::InvalidLength("record count"))?
            != footer.record_count
        {
            return Err(Error::InvalidLength("footer record count"));
        }
        let directory_index = records
            .len()
            .checked_sub(1)
            .ok_or(Error::InvalidRecordOrder("missing directory record"))?;
        let directory_record = &records[directory_index];
        if directory_record.kind != RecordKind::Directory {
            return Err(Error::InvalidRecordOrder("last record is not directory"));
        }
        if directory_record.object_id != 0 {
            return Err(Error::InvalidRecordOrder(
                "directory identifier is not zero",
            ));
        }
        let actual_directory_start = usize::try_from(directory_record.offset)
            .map_err(|_| Error::RangeOutOfBounds("directory offset"))?;
        if actual_directory_start != directory_range.start
            || directory_range.len()
                != usize::try_from(footer.directory_len)
                    .map_err(|_| Error::RangeOutOfBounds("directory length"))?
        {
            return Err(Error::DirectoryMismatch(
                "footer location does not match framing",
            ));
        }

        let digest = Sha256::digest(&bytes[..footer_offset]);
        if digest.as_slice() != footer.digest {
            return Err(Error::DigestMismatch);
        }

        let directory_payload = &bytes[directory_record.payload_range.clone()];
        let directory_value = decode_canonical(directory_payload, limits)?;
        let directory_entries = parse_directory(&directory_value)?;
        compare_directory(&records[..directory_index], &directory_entries)?;

        let manifest_index = records
            .iter()
            .position(|record| record.object_id == footer.manifest_id)
            .ok_or(Error::MissingManifest(footer.manifest_id))?;
        let manifest_record = &records[manifest_index];
        if manifest_record.kind != RecordKind::Manifest {
            return Err(Error::MissingManifest(footer.manifest_id));
        }
        let manifest_value =
            decode_canonical(&bytes[manifest_record.payload_range.clone()], limits)?;
        let manifest = parse_manifest(&manifest_value)?;

        let available: BTreeSet<u64> = records[..directory_index]
            .iter()
            .map(|record| record.object_id)
            .collect();
        for root in &manifest.roots {
            if !available.contains(root) {
                return Err(Error::InvalidMetadataSchema("manifest root does not exist"));
            }
        }
        if let Some(capability) = manifest.required_capabilities.first() {
            return Err(Error::UnsupportedRequiredCapability(*capability));
        }

        let mut by_id = BTreeMap::new();
        for (index, record) in records.iter().enumerate() {
            if record.object_id != 0 {
                by_id.insert(record.object_id, index);
            }
        }

        Ok(Self {
            bytes,
            epoch: EXPERIMENTAL_EPOCH,
            manifest_id: footer.manifest_id,
            manifest,
            records,
            by_id,
        })
    }

    #[must_use]
    pub fn object(&self, object_id: u64) -> Option<&'a [u8]> {
        let index = *self.by_id.get(&object_id)?;
        let record = &self.records[index];
        Some(&self.bytes[record.payload_range.clone()])
    }

    #[must_use]
    pub fn record(&self, object_id: u64) -> Option<&RecordInfo> {
        let index = *self.by_id.get(&object_id)?;
        self.records.get(index)
    }

    #[must_use]
    pub fn inspect(&self) -> String {
        let mut output = String::new();
        output.push_str("UCOF-EXP-0001\n");
        output.push_str(&format!("bytes: {}\n", self.bytes.len()));
        output.push_str(&format!("records: {}\n", self.records.len()));
        output.push_str(&format!("active manifest: {}\n", self.manifest_id));
        output.push_str(&format!("roots: {:?}\n", self.manifest.roots));
        output.push_str("inventory:\n");
        for record in &self.records {
            output.push_str(&format!(
                "  id={} kind={:?} offset={} stored={} logical={}\n",
                record.object_id, record.kind, record.offset, record.stored_len, record.logical_len
            ));
        }
        output
    }
}

#[derive(Debug)]
struct Footer {
    directory_offset: u64,
    directory_len: u64,
    manifest_id: u64,
    record_count: u64,
    digest: [u8; 32],
}

fn validate_header(bytes: &[u8]) -> Result<(), Error> {
    if take(bytes, 0, FILE_MAGIC.len(), "file magic")? != FILE_MAGIC {
        return Err(Error::InvalidMagic("file"));
    }
    let epoch = read_u32_le(bytes, 8, "epoch")?;
    if epoch != EXPERIMENTAL_EPOCH {
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
    if take(bytes, 20, 12, "file reserved bytes")?
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(Error::InvalidReserved("file header"));
    }
    Ok(())
}

fn parse_footer(bytes: &[u8], offset: usize) -> Result<Footer, Error> {
    if take(bytes, offset, FOOTER_MAGIC.len(), "footer magic")? != FOOTER_MAGIC {
        return Err(Error::InvalidMagic("footer"));
    }
    if read_u32_le(bytes, offset + 8, "footer length")?
        != u32::try_from(FOOTER_LEN).expect("fixed footer length")
    {
        return Err(Error::InvalidLength("footer"));
    }
    let flags = read_u32_le(bytes, offset + 12, "footer flags")?;
    if flags != 0 {
        return Err(Error::UnsupportedFlags("footer", u64::from(flags)));
    }
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(take(bytes, offset + 48, 32, "footer digest")?);
    Ok(Footer {
        directory_offset: read_u64_le(bytes, offset + 16, "directory offset")?,
        directory_len: read_u64_le(bytes, offset + 24, "directory length")?,
        manifest_id: read_u64_le(bytes, offset + 32, "manifest identifier")?,
        record_count: read_u64_le(bytes, offset + 40, "record count")?,
        digest,
    })
}

fn scan_records(
    bytes: &[u8],
    footer_offset: usize,
    limits: &Limits,
) -> Result<Vec<RecordInfo>, Error> {
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    let mut offset = HEADER_LEN;
    while offset < footer_offset {
        if u64::try_from(records.len()).map_err(|_| Error::LimitExceeded("record count"))?
            >= limits.max_records
        {
            return Err(Error::LimitExceeded("record count"));
        }
        if footer_offset - offset < RECORD_HEADER_LEN {
            return Err(Error::Truncated("record header"));
        }
        if take(bytes, offset, RECORD_MAGIC.len(), "record magic")? != RECORD_MAGIC {
            return Err(Error::InvalidMagic("record"));
        }
        let raw_kind = read_u16_le(bytes, offset + 4, "record kind")?;
        let kind = RecordKind::try_from(raw_kind)?;
        let flags = read_u16_le(bytes, offset + 6, "record flags")?;
        if flags != 0 {
            return Err(Error::UnsupportedFlags("record", u64::from(flags)));
        }
        if read_u32_le(bytes, offset + 8, "record header length")?
            != u32::try_from(RECORD_HEADER_LEN).expect("fixed record header length")
        {
            return Err(Error::InvalidLength("record header"));
        }
        let stored_len = read_u64_le(bytes, offset + 12, "stored length")?;
        let logical_len = read_u64_le(bytes, offset + 20, "logical length")?;
        if stored_len != logical_len {
            return Err(Error::InvalidLength("transformed logical length"));
        }
        if stored_len > limits.max_payload_bytes {
            return Err(Error::LimitExceeded("record payload bytes"));
        }
        let object_id = read_u64_le(bytes, offset + 28, "object identifier")?;
        if read_u32_le(bytes, offset + 36, "record reserved")? != 0 {
            return Err(Error::InvalidReserved("record header"));
        }
        if kind == RecordKind::Directory {
            if object_id != 0 {
                return Err(Error::InvalidRecordOrder(
                    "directory identifier must be zero",
                ));
            }
        } else if object_id == 0 {
            return Err(Error::InvalidRecordOrder(
                "non-directory identifier is zero",
            ));
        } else if !ids.insert(object_id) {
            return Err(Error::DuplicateObjectId(object_id));
        }

        let payload_offset = offset
            .checked_add(RECORD_HEADER_LEN)
            .ok_or(Error::RangeOutOfBounds("record payload"))?;
        let payload_range = checked_range(
            u64::try_from(payload_offset).map_err(|_| Error::RangeOutOfBounds("record payload"))?,
            stored_len,
            footer_offset,
            "record payload",
        )?;
        let next = payload_range.end;
        records.push(RecordInfo {
            kind,
            object_id,
            offset: u64::try_from(offset).map_err(|_| Error::RangeOutOfBounds("record offset"))?,
            stored_len,
            logical_len,
            payload_range,
        });
        offset = next;
    }
    if offset != footer_offset {
        return Err(Error::InvalidRecordOrder("records do not end at footer"));
    }
    Ok(records)
}

fn parse_directory(value: &CborValue) -> Result<Vec<DirectoryEntry>, Error> {
    let map = exact_map(value, &["entries"], "directory")?;
    let entries_value = map
        .get("entries")
        .ok_or(Error::InvalidMetadataSchema("directory entries"))?;
    let CborValue::Array(entries) = entries_value else {
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

fn compare_directory(records: &[RecordInfo], entries: &[DirectoryEntry]) -> Result<(), Error> {
    if records.len() != entries.len() {
        return Err(Error::DirectoryMismatch("entry count"));
    }
    for (record, entry) in records.iter().zip(entries) {
        if record.object_id != entry.id
            || record.kind != entry.kind
            || record.offset != entry.offset
            || record.stored_len != entry.stored_len
            || record.logical_len != entry.logical_len
        {
            return Err(Error::DirectoryMismatch("entry does not match framing"));
        }
    }
    Ok(())
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
    match value.copied() {
        Some(CborValue::Unsigned(value)) => Ok(*value),
        _ => Err(Error::InvalidMetadataSchema(context)),
    }
}

fn unsigned_array(value: Option<&&CborValue>, context: &'static str) -> Result<Vec<u64>, Error> {
    let Some(CborValue::Array(values)) = value.copied() else {
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
