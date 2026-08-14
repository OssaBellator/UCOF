use crate::bounded_spill_sort::{
    bounded_spill_sort_to, BoundedSpillRecord, BoundedSpillSortLimits, BoundedSpillSortReport,
};
use std::io::{BufWriter, Error as IoError, ErrorKind as IoErrorKind, Result as IoResult};

const ENCRYPTED_SORTER_KEY_BYTES: usize = 8;
const ENCRYPTED_SORTER_PAYLOAD_BYTES: usize = ENCRYPTED_SORTER_KEY_BYTES
    + ENCRYPTED_DESCRIPTOR_NONCE_BYTES
    + DESCRIPTOR_STAGE_BYTES
    + ENCRYPTED_DESCRIPTOR_TAG_BYTES;
pub(super) const ENCRYPTED_SORTER_FRAME_BYTES: usize = 8 + ENCRYPTED_SORTER_PAYLOAD_BYTES;
const SORTER_AAD_DOMAIN: &[u8] = b"UCOF-EXP-0171-SPILL\0";

#[derive(Clone, Copy)]
struct DescriptorCryptoContext {
    key: [u8; 32],
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    journal_generation: u64,
}

impl DescriptorCryptoContext {
    fn from_session(session: &DescriptorEncryptionSession) -> Self {
        Self {
            key: session.key,
            nonce_prefix: session.nonce_prefix,
            operation_id: session.operation_id,
            journal_generation: session.journal_generation,
        }
    }
}

pub(super) fn encrypted_sorter_limits(
    mut limits: BoundedSpillSortLimits,
) -> CandidateResult<BoundedSpillSortLimits> {
    if limits.record_bytes != DESCRIPTOR_STAGE_BYTES {
        return Err("encrypted sorter plaintext descriptor width".into());
    }
    limits.record_bytes = ENCRYPTED_SORTER_PAYLOAD_BYTES;
    Ok(limits)
}

pub(super) fn encrypt_descriptor_for_sorter(
    record: BoundedSpillRecord,
    session: &mut DescriptorEncryptionSession,
) -> CandidateResult<BoundedSpillRecord> {
    if record.payload.len() != DESCRIPTOR_STAGE_BYTES {
        return Err("encrypted sorter descriptor width".into());
    }
    let mut descriptor = [0u8; DESCRIPTOR_STAGE_BYTES];
    descriptor.copy_from_slice(&record.payload);
    let decoded = SourceDescriptor::decode(&descriptor)?;
    if decoded.object_id != record.key {
        return Err("encrypted sorter key mismatch".into());
    }

    let counter = session.allocate_counter()?;
    let nonce = descriptor_nonce(session.nonce_prefix, counter);
    let aad = sorter_aad(
        session.operation_id,
        session.journal_generation,
        record.key,
        counter,
    );
    let key = descriptor_key(&session.key)?;
    let mut protected = descriptor.to_vec();
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad.as_slice()),
        &mut protected,
    )
    .map_err(|_| "encrypted sorter seal".to_owned())?;
    if protected.len() != DESCRIPTOR_STAGE_BYTES + ENCRYPTED_DESCRIPTOR_TAG_BYTES {
        return Err("encrypted sorter protected width".into());
    }

    let mut payload = Vec::with_capacity(ENCRYPTED_SORTER_PAYLOAD_BYTES);
    payload.extend_from_slice(&record.key.to_le_bytes());
    payload.extend_from_slice(&nonce);
    payload.extend_from_slice(&protected);
    Ok(BoundedSpillRecord::new(record.key, payload))
}

fn decrypt_sorter_payload(
    payload: &[u8],
    context: DescriptorCryptoContext,
) -> CandidateResult<(u64, [u8; DESCRIPTOR_STAGE_BYTES])> {
    if payload.len() != ENCRYPTED_SORTER_PAYLOAD_BYTES {
        return Err("encrypted sorter payload width".into());
    }
    let key_value = u64::from_le_bytes(
        payload[..ENCRYPTED_SORTER_KEY_BYTES]
            .try_into()
            .expect("encrypted sorter key"),
    );
    let nonce: [u8; ENCRYPTED_DESCRIPTOR_NONCE_BYTES] = payload
        [ENCRYPTED_SORTER_KEY_BYTES..ENCRYPTED_SORTER_KEY_BYTES + ENCRYPTED_DESCRIPTOR_NONCE_BYTES]
        .try_into()
        .expect("encrypted sorter nonce");
    if nonce[..4] != context.nonce_prefix {
        return Err("encrypted sorter nonce prefix".into());
    }
    let counter = u64::from_be_bytes(nonce[4..].try_into().expect("encrypted sorter counter"));
    let aad = sorter_aad(
        context.operation_id,
        context.journal_generation,
        key_value,
        counter,
    );
    let key = descriptor_key(&context.key)?;
    let mut protected = payload[ENCRYPTED_SORTER_KEY_BYTES + ENCRYPTED_DESCRIPTOR_NONCE_BYTES..]
        .to_vec();
    let plaintext = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad.as_slice()),
            &mut protected,
        )
        .map_err(|_| "encrypted sorter authentication".to_owned())?;
    if plaintext.len() != DESCRIPTOR_STAGE_BYTES {
        return Err("encrypted sorter plaintext width".into());
    }
    let mut descriptor = [0u8; DESCRIPTOR_STAGE_BYTES];
    descriptor.copy_from_slice(plaintext);
    let decoded = SourceDescriptor::decode(&descriptor)?;
    if decoded.object_id != key_value {
        return Err("encrypted sorter authenticated key mismatch".into());
    }
    Ok((key_value, descriptor))
}

struct EncryptedSorterOutput<'a> {
    stage: FixedStage,
    writer: BufWriter<File>,
    spill_context: DescriptorCryptoContext,
    retained_session: &'a mut DescriptorEncryptionSession,
    pending: [u8; ENCRYPTED_SORTER_PAYLOAD_BYTES],
    pending_len: usize,
    previous_key: Option<u64>,
    first_retained_counter: Option<u64>,
}

impl<'a> EncryptedSorterOutput<'a> {
    fn create(
        directory: &Path,
        spill_context: DescriptorCryptoContext,
        retained_session: &'a mut DescriptorEncryptionSession,
    ) -> CandidateResult<Self> {
        let stage = FixedStage::create(
            directory,
            "encrypted-source-descriptors",
            ENCRYPTED_DESCRIPTOR_STAGE_BYTES,
        )?;
        let writer = stage.writer()?;
        Ok(Self {
            stage,
            writer,
            spill_context,
            retained_session,
            pending: [0u8; ENCRYPTED_SORTER_PAYLOAD_BYTES],
            pending_len: 0,
            previous_key: None,
            first_retained_counter: None,
        })
    }

    fn accept_complete_payload(&mut self) -> CandidateResult<()> {
        let (key_value, descriptor) = decrypt_sorter_payload(&self.pending, self.spill_context)?;
        if self.previous_key.is_some_and(|previous| key_value <= previous) {
            return Err("encrypted sorter final key ordering".into());
        }
        self.previous_key = Some(key_value);

        let sequence = u64::try_from(self.stage.records)
            .map_err(|_| "retained descriptor sequence".to_owned())?;
        let counter = self.retained_session.allocate_counter()?;
        if self.first_retained_counter.is_none() {
            self.first_retained_counter = Some(counter);
        }
        let nonce = descriptor_nonce(self.retained_session.nonce_prefix, counter);
        let aad = descriptor_aad(
            self.retained_session.operation_id,
            self.retained_session.journal_generation,
            sequence,
            counter,
        );
        let key = descriptor_key(&self.retained_session.key)?;
        let mut protected = descriptor.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad.as_slice()),
            &mut protected,
        )
        .map_err(|_| "retained descriptor encryption".to_owned())?;
        self.writer
            .write_all(&nonce)
            .and_then(|_| self.writer.write_all(&protected))
            .map_err(|error| error.to_string())?;
        self.stage.note_record()?;
        Ok(())
    }

    fn finish(mut self, expected_records: usize) -> CandidateResult<EncryptedDescriptorStage> {
        if self.pending_len != 0 {
            return Err("encrypted sorter output truncated".into());
        }
        if self.stage.records != expected_records {
            return Err("encrypted sorter retained record count".into());
        }
        self.writer.flush().map_err(|error| error.to_string())?;
        drop(self.writer);
        self.stage.validate_bytes()?;
        Ok(EncryptedDescriptorStage {
            stage: self.stage,
            nonce_prefix: self.retained_session.nonce_prefix,
            operation_id: self.retained_session.operation_id,
            journal_generation: self.retained_session.journal_generation,
            first_counter: self
                .first_retained_counter
                .ok_or_else(|| "retained descriptor nonce start".to_owned())?,
        })
    }
}

impl Write for EncryptedSorterOutput<'_> {
    fn write(&mut self, input: &[u8]) -> IoResult<usize> {
        if input.is_empty() {
            return Ok(0);
        }
        let remaining = ENCRYPTED_SORTER_PAYLOAD_BYTES - self.pending_len;
        let take = remaining.min(input.len());
        self.pending[self.pending_len..self.pending_len + take].copy_from_slice(&input[..take]);
        self.pending_len += take;
        if self.pending_len == ENCRYPTED_SORTER_PAYLOAD_BYTES {
            self.accept_complete_payload().map_err(candidate_io_error)?;
            self.pending_len = 0;
        }
        Ok(take)
    }

    fn flush(&mut self) -> IoResult<()> {
        self.writer.flush()
    }
}

pub(super) fn sort_encrypted_descriptors_to_retained_stage<I>(
    directory: &Path,
    records: I,
    plaintext_limits: BoundedSpillSortLimits,
    expected_records: usize,
    spill_session: &DescriptorEncryptionSession,
    retained_session: &mut DescriptorEncryptionSession,
) -> CandidateResult<(EncryptedDescriptorStage, BoundedSpillSortReport)>
where
    I: IntoIterator<Item = BoundedSpillRecord>,
{
    let limits = encrypted_sorter_limits(plaintext_limits)?;
    let spill_context = DescriptorCryptoContext::from_session(spill_session);
    let mut output = EncryptedSorterOutput::create(directory, spill_context, retained_session)?;
    let report = bounded_spill_sort_to(directory, records, &mut output, limits)
        .map_err(|error| error.to_string())?;
    let stage = output.finish(expected_records)?;
    Ok((stage, report))
}

fn sorter_aad(
    operation_id: [u8; 16],
    journal_generation: u64,
    key_value: u64,
    counter: u64,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(SORTER_AAD_DOMAIN.len() + 16 + 8 * 4);
    aad.extend_from_slice(SORTER_AAD_DOMAIN);
    aad.extend_from_slice(&operation_id);
    aad.extend_from_slice(&journal_generation.to_le_bytes());
    aad.extend_from_slice(&key_value.to_le_bytes());
    aad.extend_from_slice(&counter.to_le_bytes());
    aad.extend_from_slice(
        &u64::try_from(DESCRIPTOR_STAGE_BYTES)
            .expect("descriptor width fits u64")
            .to_le_bytes(),
    );
    aad
}

fn candidate_io_error(error: String) -> IoError {
    IoError::new(IoErrorKind::InvalidData, error)
}
