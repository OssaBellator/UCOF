use super::{LOCATOR_STAGE_BYTES, PAGE_REF_STAGE_BYTES};

const TREE_STAGE_AAD_DOMAIN: &[u8] = b"UCOF-EXP-0172-TREE-STAGE\0";
pub(super) const ENCRYPTED_LOCATOR_STAGE_BYTES: usize =
    ENCRYPTED_DESCRIPTOR_NONCE_BYTES + LOCATOR_STAGE_BYTES + ENCRYPTED_DESCRIPTOR_TAG_BYTES;
pub(super) const ENCRYPTED_PAGE_REF_STAGE_BYTES: usize =
    ENCRYPTED_DESCRIPTOR_NONCE_BYTES + PAGE_REF_STAGE_BYTES + ENCRYPTED_DESCRIPTOR_TAG_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EncryptedTreeStageKind {
    Locator = 1,
    PageRef = 2,
}

pub(super) struct EncryptedRecordStage {
    stage: FixedStage,
    plaintext_bytes: usize,
    kind: EncryptedTreeStageKind,
    stage_ordinal: u64,
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    journal_generation: u64,
    first_counter: Option<u64>,
}

impl EncryptedRecordStage {
    pub(super) fn create(
        directory: &Path,
        label: &'static str,
        plaintext_bytes: usize,
        kind: EncryptedTreeStageKind,
        stage_ordinal: u64,
        session: &DescriptorEncryptionSession,
    ) -> CandidateResult<Self> {
        let frame_bytes = encrypted_tree_frame_bytes(plaintext_bytes)?;
        Ok(Self {
            stage: FixedStage::create(directory, label, frame_bytes)?,
            plaintext_bytes,
            kind,
            stage_ordinal,
            nonce_prefix: session.nonce_prefix,
            operation_id: session.operation_id,
            journal_generation: session.journal_generation,
            first_counter: None,
        })
    }

    pub(super) fn records(&self) -> usize {
        self.stage.records
    }

    pub(super) fn bytes(&self) -> CandidateResult<u64> {
        self.stage.validate_bytes()
    }

    pub(super) fn writer<'a>(
        &'a mut self,
        session: &'a mut DescriptorEncryptionSession,
    ) -> CandidateResult<EncryptedRecordStageWriter<'a>> {
        self.check_session(session)?;
        let writer = self.stage.writer()?;
        Ok(EncryptedRecordStageWriter {
            stage: self,
            writer,
            session,
        })
    }

    pub(super) fn reader(
        &self,
        session: &DescriptorEncryptionSession,
    ) -> CandidateResult<EncryptedRecordStageReader> {
        self.check_session(session)?;
        let first_counter = self
            .first_counter
            .ok_or_else(|| "encrypted tree stage nonce start".to_owned())?;
        Ok(EncryptedRecordStageReader {
            reader: self.stage.reader()?,
            key: descriptor_key(&session.key)?,
            plaintext_bytes: self.plaintext_bytes,
            frame_bytes: self.stage.record_bytes,
            kind: self.kind,
            stage_ordinal: self.stage_ordinal,
            nonce_prefix: self.nonce_prefix,
            operation_id: self.operation_id,
            journal_generation: self.journal_generation,
            first_counter,
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
            reader.read_record()?;
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
            .ok_or_else(|| "closed encrypted tree stage".to_owned())?;
        let length = file.metadata().map_err(|error| error.to_string())?.len();
        if offset >= length {
            return Err("encrypted tree stage test offset".into());
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

    fn check_session(&self, session: &DescriptorEncryptionSession) -> CandidateResult<()> {
        if session.nonce_prefix != self.nonce_prefix
            || session.operation_id != self.operation_id
            || session.journal_generation != self.journal_generation
        {
            return Err("encrypted tree stage session mismatch".into());
        }
        Ok(())
    }
}

pub(super) struct EncryptedRecordStageWriter<'a> {
    stage: &'a mut EncryptedRecordStage,
    writer: std::io::BufWriter<File>,
    session: &'a mut DescriptorEncryptionSession,
}

impl EncryptedRecordStageWriter<'_> {
    pub(super) fn write_record(&mut self, plaintext: &[u8]) -> CandidateResult<()> {
        if plaintext.len() != self.stage.plaintext_bytes {
            return Err("encrypted tree stage plaintext width".into());
        }
        let sequence =
            u64::try_from(self.stage.stage.records).map_err(|_| "tree stage sequence")?;
        let counter = self.session.allocate_counter()?;
        if self.stage.first_counter.is_none() {
            self.stage.first_counter = Some(counter);
        }
        let nonce = descriptor_nonce(self.session.nonce_prefix, counter);
        let aad = encrypted_tree_aad(
            self.stage.kind,
            self.stage.stage_ordinal,
            self.session.operation_id,
            self.session.journal_generation,
            sequence,
            counter,
            self.stage.plaintext_bytes,
        );
        let key = descriptor_key(&self.session.key)?;
        let mut protected = plaintext.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad.as_slice()),
            &mut protected,
        )
        .map_err(|_| "encrypted tree stage seal".to_owned())?;
        self.writer
            .write_all(&nonce)
            .and_then(|_| self.writer.write_all(&protected))
            .map_err(|error| error.to_string())?;
        self.stage.stage.note_record()?;
        Ok(())
    }

    pub(super) fn finish(mut self) -> CandidateResult<()> {
        self.writer.flush().map_err(|error| error.to_string())?;
        drop(self.writer);
        self.stage.stage.validate_bytes()?;
        Ok(())
    }
}

pub(super) struct EncryptedRecordStageReader {
    reader: BufReader<File>,
    key: LessSafeKey,
    plaintext_bytes: usize,
    frame_bytes: usize,
    kind: EncryptedTreeStageKind,
    stage_ordinal: u64,
    nonce_prefix: [u8; 4],
    operation_id: [u8; 16],
    journal_generation: u64,
    first_counter: u64,
    records: usize,
    next_sequence: usize,
}

impl EncryptedRecordStageReader {
    pub(super) fn read_record(&mut self) -> CandidateResult<Vec<u8>> {
        if self.next_sequence >= self.records {
            return Err("encrypted tree stage exhausted".into());
        }
        let sequence = u64::try_from(self.next_sequence).map_err(|_| "tree stage sequence")?;
        let counter = self
            .first_counter
            .checked_add(sequence)
            .ok_or_else(|| "tree stage nonce overflow".to_owned())?;
        let expected_nonce = descriptor_nonce(self.nonce_prefix, counter);
        let mut frame = vec![0u8; self.frame_bytes];
        self.reader
            .read_exact(&mut frame)
            .map_err(|error| error.to_string())?;
        if frame[..ENCRYPTED_DESCRIPTOR_NONCE_BYTES] != expected_nonce {
            return Err("encrypted tree stage nonce sequence".into());
        }
        let aad = encrypted_tree_aad(
            self.kind,
            self.stage_ordinal,
            self.operation_id,
            self.journal_generation,
            sequence,
            counter,
            self.plaintext_bytes,
        );
        let mut protected = frame[ENCRYPTED_DESCRIPTOR_NONCE_BYTES..].to_vec();
        let plaintext = self
            .key
            .open_in_place(
                Nonce::assume_unique_for_key(expected_nonce),
                Aad::from(aad.as_slice()),
                &mut protected,
            )
            .map_err(|_| "encrypted tree stage authentication".to_owned())?;
        if plaintext.len() != self.plaintext_bytes {
            return Err("encrypted tree stage decrypted width".into());
        }
        self.next_sequence += 1;
        Ok(plaintext.to_vec())
    }

    pub(super) fn finish(&mut self) -> CandidateResult<()> {
        if self.next_sequence != self.records {
            return Err("encrypted tree stage incomplete".into());
        }
        read_exact_end(&mut self.reader, "encrypted tree stage")
    }
}

fn encrypted_tree_frame_bytes(plaintext_bytes: usize) -> CandidateResult<usize> {
    ENCRYPTED_DESCRIPTOR_NONCE_BYTES
        .checked_add(plaintext_bytes)
        .and_then(|value| value.checked_add(ENCRYPTED_DESCRIPTOR_TAG_BYTES))
        .ok_or_else(|| "encrypted tree stage frame width".to_owned())
}

fn encrypted_tree_aad(
    kind: EncryptedTreeStageKind,
    stage_ordinal: u64,
    operation_id: [u8; 16],
    journal_generation: u64,
    sequence: u64,
    counter: u64,
    plaintext_bytes: usize,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(TREE_STAGE_AAD_DOMAIN.len() + 1 + 16 + 8 * 5);
    aad.extend_from_slice(TREE_STAGE_AAD_DOMAIN);
    aad.push(kind as u8);
    aad.extend_from_slice(&stage_ordinal.to_le_bytes());
    aad.extend_from_slice(&operation_id);
    aad.extend_from_slice(&journal_generation.to_le_bytes());
    aad.extend_from_slice(&sequence.to_le_bytes());
    aad.extend_from_slice(&counter.to_le_bytes());
    aad.extend_from_slice(
        &u64::try_from(plaintext_bytes)
            .expect("tree stage record width fits u64")
            .to_le_bytes(),
    );
    aad
}
