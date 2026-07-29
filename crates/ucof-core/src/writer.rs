use crate::format::{
    push_u16_le, push_u32_le, push_u64_le, FILE_MAGIC, FOOTER_LEN, FOOTER_MAGIC, HEADER_LEN,
    RECORD_HEADER_LEN, RECORD_MAGIC,
};
use crate::model::text;
use crate::{
    encode_canonical, CborValue, DirectoryEntry, Error, Manifest, RecordKind, EXPERIMENTAL_EPOCH,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Debug)]
pub struct Writer {
    bytes: Vec<u8>,
    entries: Vec<DirectoryEntry>,
    object_ids: BTreeSet<u64>,
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    #[must_use]
    pub fn new() -> Self {
        let mut bytes = Vec::with_capacity(HEADER_LEN);
        bytes.extend_from_slice(&FILE_MAGIC);
        push_u32_le(&mut bytes, EXPERIMENTAL_EPOCH);
        push_u32_le(&mut bytes, 0);
        push_u32_le(
            &mut bytes,
            u32::try_from(HEADER_LEN).expect("fixed header length"),
        );
        bytes.extend_from_slice(&[0_u8; 12]);
        Self {
            bytes,
            entries: Vec::new(),
            object_ids: BTreeSet::new(),
        }
    }

    pub fn add_opaque(&mut self, object_id: u64, payload: &[u8]) -> Result<(), Error> {
        self.add_record(RecordKind::Opaque, object_id, payload)
    }

    pub fn add_manifest(&mut self, object_id: u64, manifest: &Manifest) -> Result<(), Error> {
        manifest.validate_shape()?;
        let payload = encode_canonical(&manifest.to_cbor())?;
        self.add_record(RecordKind::Manifest, object_id, &payload)
    }

    fn add_record(
        &mut self,
        kind: RecordKind,
        object_id: u64,
        payload: &[u8],
    ) -> Result<(), Error> {
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
        if !self.object_ids.insert(object_id) {
            return Err(Error::DuplicateObjectId(object_id));
        }
        let offset = u64::try_from(self.bytes.len())
            .map_err(|_| Error::RangeOutOfBounds("writer offset"))?;
        let stored_len =
            u64::try_from(payload.len()).map_err(|_| Error::InvalidLength("record payload"))?;
        append_record(&mut self.bytes, kind, object_id, payload)?;
        self.entries.push(DirectoryEntry {
            id: object_id,
            kind,
            offset,
            stored_len,
            logical_len: stored_len,
        });
        Ok(())
    }

    pub fn finish(mut self, manifest_id: u64) -> Result<Vec<u8>, Error> {
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
        let directory_offset = u64::try_from(self.bytes.len())
            .map_err(|_| Error::RangeOutOfBounds("directory offset"))?;
        append_record(
            &mut self.bytes,
            RecordKind::Directory,
            0,
            &directory_payload,
        )?;
        let directory_total_len = u64::try_from(RECORD_HEADER_LEN)
            .and_then(|header| {
                u64::try_from(directory_payload.len()).map(|payload| header + payload)
            })
            .map_err(|_| Error::InvalidLength("directory record"))?;
        let record_count = u64::try_from(self.entries.len())
            .map_err(|_| Error::InvalidLength("record count"))?
            .checked_add(1)
            .ok_or(Error::InvalidLength("record count"))?;

        let digest = Sha256::digest(&self.bytes);
        self.bytes.extend_from_slice(&FOOTER_MAGIC);
        push_u32_le(
            &mut self.bytes,
            u32::try_from(FOOTER_LEN).expect("fixed footer length"),
        );
        push_u32_le(&mut self.bytes, 0);
        push_u64_le(&mut self.bytes, directory_offset);
        push_u64_le(&mut self.bytes, directory_total_len);
        push_u64_le(&mut self.bytes, manifest_id);
        push_u64_le(&mut self.bytes, record_count);
        self.bytes.extend_from_slice(&digest);
        Ok(self.bytes)
    }
}

fn append_record(
    target: &mut Vec<u8>,
    kind: RecordKind,
    object_id: u64,
    payload: &[u8],
) -> Result<(), Error> {
    let payload_len = u64::try_from(payload.len()).map_err(|_| Error::InvalidLength("payload"))?;
    target.extend_from_slice(&RECORD_MAGIC);
    push_u16_le(target, u16::from(kind));
    push_u16_le(target, 0);
    push_u32_le(
        target,
        u32::try_from(RECORD_HEADER_LEN).expect("fixed record header length"),
    );
    push_u64_le(target, payload_len);
    push_u64_le(target, payload_len);
    push_u64_le(target, object_id);
    push_u32_le(target, 0);
    target.extend_from_slice(payload);
    Ok(())
}
