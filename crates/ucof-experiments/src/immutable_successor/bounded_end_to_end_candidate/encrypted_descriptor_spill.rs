const ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES: usize = 8;
pub(super) const ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES: usize =
    ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES + ENCRYPTED_DESCRIPTOR_STAGE_BYTES;
const DESCRIPTOR_SPILL_AAD_DOMAIN: &[u8] = b"UCOF-EXP-0171-DESCRIPTOR-SPILL\0";

struct EncryptedDescriptorSpillRecords<'a, S> {
    records: super::DescriptorRecords<'a, S>,
    session: &'a mut DescriptorEncryptionSession,
    key: LessSafeKey,
    encryption_error: &'a mut Option<String>,
    failed: bool,
}

impl<S: super::ImmutableStreamingPayloadSource> Iterator for EncryptedDescriptorSpillRecords<'_, S> {
    type Item = super::BoundedSpillRecord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        let record = self.records.next()?;
        if record.payload.len() != super::DESCRIPTOR_STAGE_BYTES {
            return Some(record);
        }
        let encrypted = encrypt_descriptor_spill_record(&self.key, self.session, record);
        match encrypted {
            Ok(record) => Some(record),
            Err(error) => {
                *self.encryption_error = Some(error);
                self.failed = true;
                Some(super::BoundedSpillRecord::new(0, Vec::new()))
            }
        }
    }
}

struct EncryptedSpillPreflight {
    descriptor_stage: super::FixedStage,
    descriptor_spill: super::BoundedSpillSortReport,
    expected_bytes: usize,
    expected_pages: usize,
    expected_root_level: u8,
    largest_source_buffer: usize,
    version_checks: u64,
    object_count: usize,
    sorted_spill_ciphertext_sha256: [u8; 32],
}

#[derive(Debug)]
pub(super) struct EncryptedSpillEndToEndEvidence {
    pub(super) output: super::EndToEndEvidence,
    pub(super) sorted_spill_stage_bytes: u64,
    pub(super) sorted_spill_ciphertext_sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EncryptedSpillPrivateStoragePlan {
    pub(super) encrypted_spill_descriptor_bytes: u64,
    pub(super) retained_encrypted_descriptor_bytes: u64,
    pub(super) locator_bytes: u64,
    pub(super) leaf_ref_bytes: u64,
    pub(super) sorter_plus_encrypted_spill_bytes: u64,
    pub(super) encrypted_spill_plus_retained_bytes: u64,
    pub(super) retained_plus_locator_bytes: u64,
    pub(super) locator_plus_leaf_ref_bytes: u64,
    pub(super) max_adjacent_page_ref_bytes: u64,
    pub(super) required_bytes: u64,
}

pub(super) fn encrypted_spill_private_storage_plan(
    object_count: usize,
    spill_limits: super::BoundedSpillSortLimits,
) -> super::CandidateResult<EncryptedSpillPrivateStoragePlan> {
    if object_count == 0 {
        return Err("encrypted spill private storage object count".into());
    }
    if spill_limits.record_bytes != super::DESCRIPTOR_STAGE_BYTES {
        return Err("descriptor spill record size".into());
    }

    let encrypted_spill_descriptor_bytes = super::checked_stage_bytes(
        object_count,
        ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES,
    )?;
    let retained_encrypted_descriptor_bytes =
        super::checked_stage_bytes(object_count, ENCRYPTED_DESCRIPTOR_STAGE_BYTES)?;
    let locator_bytes = super::checked_stage_bytes(object_count, super::LOCATOR_STAGE_BYTES)?;
    let leaf_ref_records =
        super::groups(object_count, super::LEAF_CAPACITY, super::LEAF_MIN_OCCUPANCY)?.len();
    let leaf_ref_bytes = super::checked_stage_bytes(leaf_ref_records, super::PAGE_REF_STAGE_BYTES)?;

    let sorter_plus_encrypted_spill_bytes = spill_limits
        .max_live_spill_bytes
        .checked_add(encrypted_spill_descriptor_bytes)
        .ok_or_else(|| "encrypted spill sorter overlap overflow".to_owned())?;
    let encrypted_spill_plus_retained_bytes = encrypted_spill_descriptor_bytes
        .checked_add(retained_encrypted_descriptor_bytes)
        .ok_or_else(|| "encrypted spill transcode overlap overflow".to_owned())?;
    let retained_plus_locator_bytes = retained_encrypted_descriptor_bytes
        .checked_add(locator_bytes)
        .ok_or_else(|| "encrypted descriptor locator overlap overflow".to_owned())?;
    let locator_plus_leaf_ref_bytes = locator_bytes
        .checked_add(leaf_ref_bytes)
        .ok_or_else(|| "encrypted spill locator leaf overlap overflow".to_owned())?;

    let mut max_adjacent_page_ref_bytes = leaf_ref_bytes;
    let mut current_records = leaf_ref_records;
    while current_records > 1 {
        let next_records = super::groups(
            current_records,
            super::INTERNAL_FANOUT,
            super::INTERNAL_MIN_OCCUPANCY,
        )?
        .len();
        let current_bytes = super::checked_stage_bytes(current_records, super::PAGE_REF_STAGE_BYTES)?;
        let next_bytes = super::checked_stage_bytes(next_records, super::PAGE_REF_STAGE_BYTES)?;
        max_adjacent_page_ref_bytes = max_adjacent_page_ref_bytes.max(
            current_bytes
                .checked_add(next_bytes)
                .ok_or_else(|| "encrypted spill page-ref overlap overflow".to_owned())?,
        );
        current_records = next_records;
    }

    let required_bytes = sorter_plus_encrypted_spill_bytes
        .max(encrypted_spill_plus_retained_bytes)
        .max(retained_plus_locator_bytes)
        .max(locator_plus_leaf_ref_bytes)
        .max(max_adjacent_page_ref_bytes);

    Ok(EncryptedSpillPrivateStoragePlan {
        encrypted_spill_descriptor_bytes,
        retained_encrypted_descriptor_bytes,
        locator_bytes,
        leaf_ref_bytes,
        sorter_plus_encrypted_spill_bytes,
        encrypted_spill_plus_retained_bytes,
        retained_plus_locator_bytes,
        locator_plus_leaf_ref_bytes,
        max_adjacent_page_ref_bytes,
        required_bytes,
    })
}

fn encrypted_descriptor_spill_limits(
    mut limits: super::BoundedSpillSortLimits,
) -> super::CandidateResult<super::BoundedSpillSortLimits> {
    if limits.record_bytes != super::DESCRIPTOR_STAGE_BYTES {
        return Err("descriptor spill record size".into());
    }
    limits.record_bytes = ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES;
    Ok(limits)
}

fn required_encrypted_spill_nonce_uses(object_count: usize) -> super::CandidateResult<u64> {
    u64::try_from(object_count)
        .map_err(|_| "encrypted descriptor nonce count".to_owned())?
        .checked_mul(2)
        .ok_or_else(|| "encrypted descriptor nonce count overflow".to_owned())
}

fn encrypt_descriptor_spill_record(
    key: &LessSafeKey,
    session: &mut DescriptorEncryptionSession,
    record: super::BoundedSpillRecord,
) -> super::CandidateResult<super::BoundedSpillRecord> {
    if record.key == 0 || record.payload.len() != super::DESCRIPTOR_STAGE_BYTES {
        return Err("encrypted descriptor spill input".into());
    }
    let counter = session.allocate_counter()?;
    let nonce = descriptor_nonce(session.nonce_prefix, counter);
    let aad = descriptor_spill_aad(
        session.operation_id,
        session.journal_generation,
        record.key,
        counter,
    );
    let mut protected = record.payload;
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad.as_slice()),
        &mut protected,
    )
    .map_err(|_| "encrypted descriptor spill encryption".to_owned())?;
    if protected.len() != super::DESCRIPTOR_STAGE_BYTES + ENCRYPTED_DESCRIPTOR_TAG_BYTES {
        return Err("encrypted descriptor spill protected width".into());
    }
    let mut payload = Vec::with_capacity(ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES);
    payload.extend_from_slice(&record.key.to_le_bytes());
    payload.extend_from_slice(&nonce);
    payload.extend_from_slice(&protected);
    if payload.len() != ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES {
        return Err("encrypted descriptor spill payload width".into());
    }
    Ok(super::BoundedSpillRecord::new(record.key, payload))
}

fn descriptor_spill_aad(
    operation_id: [u8; 16],
    journal_generation: u64,
    object_id: u64,
    counter: u64,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(DESCRIPTOR_SPILL_AAD_DOMAIN.len() + 16 + 8 * 4);
    aad.extend_from_slice(DESCRIPTOR_SPILL_AAD_DOMAIN);
    aad.extend_from_slice(&operation_id);
    aad.extend_from_slice(&journal_generation.to_le_bytes());
    aad.extend_from_slice(&object_id.to_le_bytes());
    aad.extend_from_slice(&counter.to_le_bytes());
    aad.extend_from_slice(
        &u64::try_from(super::DESCRIPTOR_STAGE_BYTES)
            .expect("descriptor width fits u64")
            .to_le_bytes(),
    );
    aad
}

fn spill_counter_from_nonce(
    nonce: &[u8; ENCRYPTED_DESCRIPTOR_NONCE_BYTES],
    expected_prefix: [u8; 4],
) -> super::CandidateResult<u64> {
    if nonce[..4] != expected_prefix {
        return Err("encrypted descriptor spill nonce prefix".into());
    }
    Ok(u64::from_be_bytes(
        nonce[4..]
            .try_into()
            .expect("encrypted descriptor spill nonce counter"),
    ))
}

fn hash_fixed_stage(stage: &super::FixedStage) -> super::CandidateResult<[u8; 32]> {
    stage.validate_bytes()?;
    let mut reader = stage.reader()?;
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

fn transcode_sorted_encrypted_spill_stage(
    directory: &Path,
    spill_stage: super::FixedStage,
    session: &mut DescriptorEncryptionSession,
) -> super::CandidateResult<EncryptedDescriptorStage> {
    if spill_stage.records == 0 {
        return Err("empty encrypted descriptor spill stage".into());
    }
    if spill_stage.record_bytes != ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES {
        return Err("encrypted descriptor spill stage width".into());
    }
    spill_stage.validate_bytes()?;
    let required_nonces = u64::try_from(spill_stage.records)
        .map_err(|_| "retained descriptor nonce count".to_owned())?;
    if session.remaining() < required_nonces {
        return Err("descriptor nonce lease capacity".into());
    }

    let mut retained = super::FixedStage::create(
        directory,
        "encrypted-source-descriptors",
        ENCRYPTED_DESCRIPTOR_STAGE_BYTES,
    )?;
    let mut retained_writer = retained.writer()?;
    let mut spill_reader = spill_stage.reader()?;
    let key = descriptor_key(&session.key)?;
    let mut previous_object_id = None;
    let mut first_counter = None;
    let mut previous_retained_counter: Option<u64> = None;

    for sequence_index in 0..spill_stage.records {
        let mut frame = [0u8; ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES];
        spill_reader
            .read_exact(&mut frame)
            .map_err(|error| error.to_string())?;
        let object_id = u64::from_le_bytes(
            frame[..ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES]
                .try_into()
                .expect("encrypted descriptor spill key"),
        );
        if object_id == 0 {
            return Err("encrypted descriptor spill key".into());
        }
        if previous_object_id.is_some_and(|previous| object_id <= previous) {
            return Err("encrypted descriptor spill ordering".into());
        }

        let nonce: [u8; ENCRYPTED_DESCRIPTOR_NONCE_BYTES] = frame
            [ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES
                ..ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES + ENCRYPTED_DESCRIPTOR_NONCE_BYTES]
            .try_into()
            .expect("encrypted descriptor spill nonce");
        let counter = spill_counter_from_nonce(&nonce, session.nonce_prefix)?;
        let aad = descriptor_spill_aad(
            session.operation_id,
            session.journal_generation,
            object_id,
            counter,
        );
        let mut protected = frame[ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES
            + ENCRYPTED_DESCRIPTOR_NONCE_BYTES..]
            .to_vec();
        let plaintext = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad.as_slice()),
                &mut protected,
            )
            .map_err(|_| "encrypted descriptor spill authentication".to_owned())?;
        if plaintext.len() != super::DESCRIPTOR_STAGE_BYTES {
            return Err("encrypted descriptor spill plaintext width".into());
        }
        let mut descriptor_bytes = [0u8; super::DESCRIPTOR_STAGE_BYTES];
        descriptor_bytes.copy_from_slice(plaintext);
        let descriptor = super::SourceDescriptor::decode(&descriptor_bytes)?;
        if descriptor.object_id != object_id {
            return Err("encrypted descriptor spill key mismatch".into());
        }
        previous_object_id = Some(object_id);

        let retained_counter = session.allocate_counter()?;
        if let Some(previous) = previous_retained_counter {
            if previous.checked_add(1) != Some(retained_counter) {
                return Err("descriptor nonce lease discontinuity".into());
            }
        } else {
            first_counter = Some(retained_counter);
        }
        previous_retained_counter = Some(retained_counter);
        let sequence =
            u64::try_from(sequence_index).map_err(|_| "descriptor sequence".to_owned())?;
        let retained_nonce = descriptor_nonce(session.nonce_prefix, retained_counter);
        let retained_aad = descriptor_aad(
            session.operation_id,
            session.journal_generation,
            sequence,
            retained_counter,
        );
        let mut retained_protected = descriptor_bytes.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(retained_nonce),
            Aad::from(retained_aad.as_slice()),
            &mut retained_protected,
        )
        .map_err(|_| "descriptor encryption".to_owned())?;
        retained_writer
            .write_all(&retained_nonce)
            .and_then(|_| retained_writer.write_all(&retained_protected))
            .map_err(|error| error.to_string())?;
        retained.note_record()?;
    }
    super::read_exact_end(&mut spill_reader, "encrypted descriptor spill stage")?;
    retained_writer.flush().map_err(|error| error.to_string())?;
    drop(retained_writer);
    retained.validate_bytes()?;

    Ok(EncryptedDescriptorStage {
        stage: retained,
        nonce_prefix: session.nonce_prefix,
        operation_id: session.operation_id,
        journal_generation: session.journal_generation,
        first_counter: first_counter.ok_or_else(|| "descriptor nonce start".to_owned())?,
    })
}

fn prepare_encrypted_spill_preflight<S: super::ImmutableStreamingPayloadSource>(
    directory: &Path,
    sources: &mut [S],
    options: super::ImmutableSourceStreamingWriteOptions,
    limits: super::ImmutableLimits,
    spill_limits: super::BoundedSpillSortLimits,
    session: &mut DescriptorEncryptionSession,
) -> super::CandidateResult<EncryptedSpillPreflight> {
    if sources.is_empty() || sources.len() > limits.max_objects {
        return Err("object count limit".into());
    }
    if options.output.max_write_request_bytes == 0 || options.max_source_read_bytes == 0 {
        return Err("streaming configuration".into());
    }
    if spill_limits.record_bytes != super::DESCRIPTOR_STAGE_BYTES {
        return Err("descriptor spill record size".into());
    }
    let required_nonce_uses = required_encrypted_spill_nonce_uses(sources.len())?;
    if session.remaining() < required_nonce_uses {
        return Err("descriptor nonce lease capacity".into());
    }

    super::allocation_check::<super::Locator>(super::LEAF_CAPACITY, limits)
        .map_err(|error| error.to_string())?;
    super::allocation_check::<super::PageRef>(super::INTERNAL_FANOUT, limits)
        .map_err(|error| error.to_string())?;

    let encrypted_limits = encrypted_descriptor_spill_limits(spill_limits)?;
    let mut descriptor_stage = super::FixedStage::create(
        directory,
        "sorted-encrypted-source-descriptors",
        ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES,
    )?;
    let mut writer = descriptor_stage.writer()?;
    let mut input_error = None;
    let mut encryption_error = None;
    let mut object_bytes = 0usize;
    let mut largest_source_buffer = 0usize;
    let mut version_checks = 0u64;
    let records = super::DescriptorRecords {
        sources: sources.iter_mut().enumerate(),
        options,
        limits,
        input_error: &mut input_error,
        object_bytes: &mut object_bytes,
        largest_source_buffer: &mut largest_source_buffer,
        version_checks: &mut version_checks,
        failed: false,
    };
    let key = descriptor_key(&session.key)?;
    let encrypted_records = EncryptedDescriptorSpillRecords {
        records,
        session,
        key,
        encryption_error: &mut encryption_error,
        failed: false,
    };
    let sorted =
        super::bounded_spill_sort_to(directory, encrypted_records, &mut writer, encrypted_limits);
    if let Some(error) = input_error {
        return Err(error);
    }
    if let Some(error) = encryption_error {
        return Err(error);
    }
    let descriptor_spill = sorted.map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())?;
    drop(writer);
    descriptor_stage.set_records_u64(descriptor_spill.output_records)?;
    let stage_bytes = descriptor_stage.validate_bytes()?;
    let expected_stage_bytes = descriptor_spill
        .output_records
        .checked_mul(
            u64::try_from(ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES)
                .expect("encrypted spill width fits u64"),
        )
        .ok_or_else(|| "encrypted descriptor spill stage byte overflow".to_owned())?;
    if descriptor_spill.output_records
        != u64::try_from(sources.len()).map_err(|_| "object count".to_owned())?
        || descriptor_spill.output_payload_bytes != expected_stage_bytes
        || stage_bytes != expected_stage_bytes
    {
        return Err("encrypted descriptor spill stage size".into());
    }
    let sorted_spill_ciphertext_sha256 = hash_fixed_stage(&descriptor_stage)?;

    let (expected_pages, expected_root_level) =
        super::streaming_tree_shape(sources.len(), limits).map_err(|error| error.to_string())?;
    let page_bytes = expected_pages
        .checked_mul(super::PAGE_SIZE)
        .ok_or_else(|| "page output size".to_owned())?;
    let expected_bytes = super::FILE_HEADER_LEN
        .checked_add(object_bytes)
        .and_then(|value| value.checked_add(page_bytes))
        .and_then(|value| value.checked_add(super::SNAPSHOT_LEN))
        .and_then(|value| value.checked_add(super::FOOTER_LEN))
        .ok_or_else(|| "output size".to_owned())?;
    if expected_bytes > limits.max_output_bytes {
        return Err("output limit".into());
    }
    if expected_bytes > limits.max_file_bytes {
        return Err("file size limit".into());
    }

    Ok(EncryptedSpillPreflight {
        descriptor_stage,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count: sources.len(),
        sorted_spill_ciphertext_sha256,
    })
}

#[derive(Clone, Copy)]
struct EncryptedSpillPreparedSettings {
    options: super::ImmutableSourceStreamingWriteOptions,
    limits: super::ImmutableLimits,
}

fn write_prepared_encrypted_spill_candidate_with_stage_hook<W, S, F>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    settings: EncryptedSpillPreparedSettings,
    preflight: EncryptedSpillPreflight,
    session: &mut DescriptorEncryptionSession,
    stage_hook: F,
) -> super::CandidateResult<EncryptedSpillEndToEndEvidence>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
    F: FnOnce(&mut super::FixedStage) -> super::CandidateResult<()>,
{
    let EncryptedSpillPreflight {
        mut descriptor_stage,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count,
        sorted_spill_ciphertext_sha256,
    } = preflight;
    let sorted_spill_stage_bytes = descriptor_stage.validate_bytes()?;
    stage_hook(&mut descriptor_stage)?;
    descriptor_stage.validate_bytes()?;
    let encrypted_stage =
        transcode_sorted_encrypted_spill_stage(directory, descriptor_stage, session)?;
    if encrypted_stage.records() != object_count {
        return Err("encrypted descriptor object count".into());
    }
    encrypted_stage.verify_all(session)?;
    let descriptor_stage_bytes = encrypted_stage.bytes()?;
    let descriptor_ciphertext_sha256 = Some(encrypted_stage.ciphertext_sha256()?);
    let descriptor_reader = encrypted_stage.reader(session)?;
    let emission = super::PreparedEmission {
        descriptor_stage_bytes,
        descriptor_ciphertext_sha256,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count,
    };
    let output = super::write_prepared_from_descriptor_reader(
        writer,
        sources,
        directory,
        settings.options,
        settings.limits,
        emission,
        descriptor_reader,
    )?;
    drop(encrypted_stage);
    Ok(EncryptedSpillEndToEndEvidence {
        output,
        sorted_spill_stage_bytes,
        sorted_spill_ciphertext_sha256,
    })
}

pub(super) fn write_genesis_sources_end_to_end_encrypted_spill_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    options: super::ImmutableSourceStreamingWriteOptions,
    limits: super::ImmutableLimits,
    spill_limits: super::BoundedSpillSortLimits,
    session: &mut DescriptorEncryptionSession,
) -> super::CandidateResult<EncryptedSpillEndToEndEvidence>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
{
    let preflight = prepare_encrypted_spill_preflight(
        directory,
        sources,
        options,
        limits,
        spill_limits,
        session,
    )?;
    write_prepared_encrypted_spill_candidate_with_stage_hook(
        writer,
        sources,
        directory,
        EncryptedSpillPreparedSettings { options, limits },
        preflight,
        session,
        |_| Ok(()),
    )
}

pub(super) fn write_genesis_sources_with_encrypted_spill_private_quota_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    options: super::ImmutableSourceStreamingWriteOptions,
    limits: super::ImmutableLimits,
    spill_limits: super::BoundedSpillSortLimits,
    max_private_storage_bytes: u64,
    session: &mut DescriptorEncryptionSession,
) -> super::CandidateResult<(EncryptedSpillPrivateStoragePlan, EncryptedSpillEndToEndEvidence)>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
{
    let plan = encrypted_spill_private_storage_plan(sources.len(), spill_limits)?;
    if plan.required_bytes > max_private_storage_bytes {
        return Err("private storage limit".into());
    }
    let evidence = write_genesis_sources_end_to_end_encrypted_spill_candidate(
        writer,
        sources,
        directory,
        options,
        limits,
        spill_limits,
        session,
    )?;
    Ok((plan, evidence))
}
