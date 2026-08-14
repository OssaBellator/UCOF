use super::{
    read_exact_end, CandidateResult, FixedStage, SourceDescriptor, DESCRIPTOR_STAGE_BYTES,
};
use aws_lc_rs::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

const ENCRYPTED_DESCRIPTOR_NONCE_BYTES: usize = 12;
const ENCRYPTED_DESCRIPTOR_TAG_BYTES: usize = 16;
pub(super) const ENCRYPTED_DESCRIPTOR_STAGE_BYTES: usize = ENCRYPTED_DESCRIPTOR_NONCE_BYTES
    + DESCRIPTOR_STAGE_BYTES
    + ENCRYPTED_DESCRIPTOR_TAG_BYTES;
const DESCRIPTOR_AAD_DOMAIN: &[u8] = b"UCOF-EXP-0170-DESCRIPTOR\0";

pub(super) struct DescriptorNonceAuthority {
    durable: DurableNonceState,
}

impl DescriptorNonceAuthority {
    pub(super) fn initial() -> Self {
        Self {
            durable: DurableNonceState::initial(),
        }
    }

    pub(super) fn activate_session(
        &mut self,
        key: [u8; 32],
        nonce_prefix: [u8; 4],
        operation_id: [u8; 16],
        lease_size: u64,
        max_lease_size: u64,
        durably_committed: bool,
    ) -> CandidateResult<DescriptorEncryptionSession> {
        let pending = reserve_nonce_lease(self.durable, lease_size, max_lease_size)
            .map_err(|error| format!("descriptor nonce reservation: {error:?}"))?;
        let (durable, lease) = activate_nonce_lease(self.durable, pending, durably_committed)
            .map_err(|error| format!("descriptor nonce activation: {error:?}"))?;
        self.durable = durable;
        Ok(DescriptorEncryptionSession {
            key,
            nonce_prefix,
            operation_id,
            journal_generation: durable.generation,
            lease,
        })
    }

    pub(super) fn next_unreserved(&self) -> Option<u64> {
        self.durable.next_unreserved
    }
}

pub(super) struct DescriptorEncryptionSession {
    key: [u8; 32],
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    journal_generation: u64,
    lease: ActiveNonceLease,
}

impl DescriptorEncryptionSession {
    pub(super) fn remaining(&self) -> u64 {
        self.lease.remaining()
    }

    fn allocate_counter(&mut self) -> CandidateResult<u64> {
        self.lease
            .allocate()
            .map_err(|error| format!("descriptor nonce allocation: {error:?}"))
    }
}

pub(super) struct EncryptedDescriptorStage {
    stage: FixedStage,
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    journal_generation: u64,
    first_counter: u64,
}

impl EncryptedDescriptorStage {
    pub(super) fn records(&self) -> usize {
        self.stage.records
    }

    pub(super) fn bytes(&self) -> CandidateResult<u64> {
        self.stage.validate_bytes()
    }

    pub(super) fn reader(
        &self,
        session: &DescriptorEncryptionSession,
    ) -> CandidateResult<EncryptedDescriptorReader> {
        if session.nonce_prefix != self.nonce_prefix
            || session.operation_id != self.operation_id
            || session.journal_generation != self.journal_generation
        {
            return Err("descriptor encryption session mismatch".into());
        }
        Ok(EncryptedDescriptorReader {
            reader: self.stage.reader()?,
            key: descriptor_key(&session.key)?,
            nonce_prefix: self.nonce_prefix,
            operation_id: self.operation_id,
            journal_generation: self.journal_generation,
            first_counter: self.first_counter,
            records: self.stage.records,
            next_sequence: 0,
        })
    }

    pub(super) fn verify_all(
        &self,
        session: &DescriptorEncryptionSession,
    ) -> CandidateResult<()> {
        let mut reader = self.reader(session)?;
        for _ in 0..self.records() {
            let raw = reader.read_descriptor()?;
            SourceDescriptor::decode(&raw)?;
        }
        reader.finish()
    }

    pub(super) fn ciphertext_sha256(&self) -> CandidateResult<[u8; 32]> {
        let mut reader = self.stage.reader()?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = reader.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(hasher.finalize().into())
    }

    #[cfg(test)]
    pub(super) fn flip_byte_for_test(&mut self, offset: u64) -> CandidateResult<()> {
        use std::io::{Seek, SeekFrom};

        let file = self
            .stage
            .file
            .as_mut()
            .ok_or_else(|| "closed encrypted descriptor stage".to_owned())?;
        let length = file.metadata().map_err(|error| error.to_string())?.len();
        if offset >= length {
            return Err("encrypted descriptor test offset".into());
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        let mut byte = [0u8; 1];
        file.read_exact(&mut byte)
            .map_err(|error| error.to_string())?;
        byte[0] ^= 0x80;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        file.write_all(&byte).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
        Ok(())
    }
}

pub(super) struct EncryptedDescriptorReader {
    reader: BufReader<File>,
    key: LessSafeKey,
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    journal_generation: u64,
    first_counter: u64,
    records: usize,
    next_sequence: usize,
}

impl EncryptedDescriptorReader {
    pub(super) fn read_descriptor(&mut self) -> CandidateResult<[u8; DESCRIPTOR_STAGE_BYTES]> {
        if self.next_sequence >= self.records {
            return Err("encrypted descriptor stage exhausted".into());
        }
        let sequence = u64::try_from(self.next_sequence)
            .map_err(|_| "encrypted descriptor sequence".to_owned())?;
        let counter = self
            .first_counter
            .checked_add(sequence)
            .ok_or_else(|| "encrypted descriptor nonce overflow".to_owned())?;
        let expected_nonce = descriptor_nonce(self.nonce_prefix, counter);
        let mut frame = [0u8; ENCRYPTED_DESCRIPTOR_STAGE_BYTES];
        self.reader
            .read_exact(&mut frame)
            .map_err(|error| error.to_string())?;
        if frame[..ENCRYPTED_DESCRIPTOR_NONCE_BYTES] != expected_nonce {
            return Err("encrypted descriptor nonce sequence".into());
        }
        let mut protected = frame[ENCRYPTED_DESCRIPTOR_NONCE_BYTES..].to_vec();
        let aad = descriptor_aad(
            self.operation_id,
            self.journal_generation,
            sequence,
            counter,
        );
        let plaintext = self
            .key
            .open_in_place(
                Nonce::assume_unique_for_key(expected_nonce),
                Aad::from(aad.as_slice()),
                &mut protected,
            )
            .map_err(|_| "encrypted descriptor authentication".to_owned())?;
        if plaintext.len() != DESCRIPTOR_STAGE_BYTES {
            return Err("encrypted descriptor plaintext width".into());
        }
        let mut descriptor = [0u8; DESCRIPTOR_STAGE_BYTES];
        descriptor.copy_from_slice(plaintext);
        self.next_sequence += 1;
        Ok(descriptor)
    }

    pub(super) fn finish(&mut self) -> CandidateResult<()> {
        if self.next_sequence != self.records {
            return Err("encrypted descriptor stage incomplete".into());
        }
        read_exact_end(&mut self.reader, "encrypted descriptor stage")
    }
}

pub(super) fn transcode_descriptor_stage(
    directory: &Path,
    plaintext: FixedStage,
    session: &mut DescriptorEncryptionSession,
) -> CandidateResult<EncryptedDescriptorStage> {
    if plaintext.records == 0 {
        return Err("empty descriptor stage".into());
    }
    if plaintext.record_bytes != DESCRIPTOR_STAGE_BYTES {
        return Err("plaintext descriptor stage width".into());
    }
    plaintext.validate_bytes()?;
    let required_nonces =
        u64::try_from(plaintext.records).map_err(|_| "descriptor nonce count".to_owned())?;
    if session.remaining() < required_nonces {
        return Err("descriptor nonce lease capacity".into());
    }

    let mut encrypted = FixedStage::create(
        directory,
        "encrypted-source-descriptors",
        ENCRYPTED_DESCRIPTOR_STAGE_BYTES,
    )?;
    let mut encrypted_writer = encrypted.writer()?;
    let mut plaintext_reader = plaintext.reader()?;
    let key = descriptor_key(&session.key)?;
    let mut first_counter: Option<u64> = None;
    let mut previous_counter: Option<u64> = None;

    for sequence_index in 0..plaintext.records {
        let mut descriptor = [0u8; DESCRIPTOR_STAGE_BYTES];
        plaintext_reader
            .read_exact(&mut descriptor)
            .map_err(|error| error.to_string())?;
        SourceDescriptor::decode(&descriptor)?;

        let counter = session.allocate_counter()?;
        if let Some(previous) = previous_counter {
            if previous.checked_add(1) != Some(counter) {
                return Err("descriptor nonce lease discontinuity".into());
            }
        } else {
            first_counter = Some(counter);
        }
        previous_counter = Some(counter);
        let sequence =
            u64::try_from(sequence_index).map_err(|_| "descriptor sequence".to_owned())?;
        let nonce = descriptor_nonce(session.nonce_prefix, counter);
        let aad = descriptor_aad(
            session.operation_id,
            session.journal_generation,
            sequence,
            counter,
        );
        let mut protected = descriptor.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad.as_slice()),
            &mut protected,
        )
        .map_err(|_| "descriptor encryption".to_owned())?;
        if protected.len() != DESCRIPTOR_STAGE_BYTES + ENCRYPTED_DESCRIPTOR_TAG_BYTES {
            return Err("encrypted descriptor protected width".into());
        }
        encrypted_writer
            .write_all(&nonce)
            .and_then(|_| encrypted_writer.write_all(&protected))
            .map_err(|error| error.to_string())?;
        encrypted.note_record()?;
    }
    read_exact_end(&mut plaintext_reader, "plaintext descriptor stage")?;
    encrypted_writer.flush().map_err(|error| error.to_string())?;
    drop(encrypted_writer);
    encrypted.validate_bytes()?;

    Ok(EncryptedDescriptorStage {
        stage: encrypted,
        nonce_prefix: session.nonce_prefix,
        operation_id: session.operation_id,
        journal_generation: session.journal_generation,
        first_counter: first_counter.ok_or_else(|| "descriptor nonce start".to_owned())?,
    })
}

fn descriptor_key(key_bytes: &[u8; 32]) -> CandidateResult<LessSafeKey> {
    let unbound =
        UnboundKey::new(&AES_256_GCM, key_bytes).map_err(|_| "descriptor AES-256-GCM key")?;
    Ok(LessSafeKey::new(unbound))
}

fn descriptor_nonce(prefix: [u8; 4], counter: u64) -> [u8; ENCRYPTED_DESCRIPTOR_NONCE_BYTES] {
    let mut nonce = [0u8; ENCRYPTED_DESCRIPTOR_NONCE_BYTES];
    nonce[..4].copy_from_slice(&prefix);
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    nonce
}

fn descriptor_aad(
    operation_id: [u8; 16],
    journal_generation: u64,
    sequence: u64,
    counter: u64,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(DESCRIPTOR_AAD_DOMAIN.len() + 16 + 8 * 4);
    aad.extend_from_slice(DESCRIPTOR_AAD_DOMAIN);
    aad.extend_from_slice(&operation_id);
    aad.extend_from_slice(&journal_generation.to_le_bytes());
    aad.extend_from_slice(&sequence.to_le_bytes());
    aad.extend_from_slice(&counter.to_le_bytes());
    aad.extend_from_slice(
        &u64::try_from(DESCRIPTOR_STAGE_BYTES)
            .expect("descriptor width fits u64")
            .to_le_bytes(),
    );
    aad
}
