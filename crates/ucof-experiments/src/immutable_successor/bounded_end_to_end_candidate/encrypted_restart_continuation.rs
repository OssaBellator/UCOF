#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestartRecoveredPreflight {
    object_count: usize,
    expected_bytes: usize,
    expected_pages: usize,
    expected_root_level: u8,
    largest_source_buffer: usize,
    version_checks: u64,
}

#[derive(Debug)]
struct EncryptedRestartContinuationEvidence {
    output: super::EndToEndEvidence,
    crashed_generation: u64,
    fresh_generation: u64,
    crashed_lease_first: u64,
    crashed_lease_last: u64,
    fresh_lease_first: u64,
    fresh_lease_last: u64,
    persisted_spill_sha256: [u8; 32],
}

fn reconstructed_restart_spill_report(
    object_count: usize,
    stage_length: u64,
    stage_sha256: [u8; 32],
) -> super::CandidateResult<super::BoundedSpillSortReport> {
    let records = u64::try_from(object_count).map_err(|_| "restart spill record count".to_owned())?;
    Ok(super::BoundedSpillSortReport {
        input_records: records,
        initial_runs: 0,
        merge_passes: 0,
        peak_open_files: 0,
        peak_buffer_encoded_bytes: 0,
        initial_spill_bytes: 0,
        total_spill_bytes_written: 0,
        peak_live_spill_bytes: 0,
        merge_bytes_read: 0,
        merge_bytes_written: 0,
        final_run_bytes_read: 0,
        output_records: records,
        output_payload_bytes: stage_length,
        output_sha256: stage_sha256,
    })
}

fn open_verified_restart_stage(
    journal: &LinuxDurableNonceJournal,
    stage_directory: &File,
    generation: u64,
    disposition: &EncryptedStageRestartDisposition,
) -> super::CandidateResult<(LinuxEncryptedStageManifest, LinuxNonceJournalRecord, File)> {
    let role = EncryptedRestartStageRole::SortedDescriptorSpill;
    let manifest = load_encrypted_stage_manifest(journal, generation, role)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "restart manifest disappeared".to_owned())?;
    let nonce_record = load_nonce_generation_record(journal, generation)
        .map_err(|error| error.to_string())?;
    let name = match disposition {
        EncryptedStageRestartDisposition::VerifiedExactNeedsFreshLease { .. } => {
            encrypted_stage_file_name(generation, role)
        }
        EncryptedStageRestartDisposition::VerifiedRenamedNeedsFreshLease { actual_name, .. } => {
            actual_name.clone()
        }
        _ => return Err("restart stage is not verified continuation input".into()),
    };
    let file = linux_nonce_open_relative_readonly(stage_directory, &name)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "verified restart stage disappeared".to_owned())?;
    Ok((manifest, nonce_record, file))
}

fn transcode_restart_spill_with_fresh_session(
    work_directory: &Path,
    persisted_stage: &File,
    manifest: LinuxEncryptedStageManifest,
    nonce_record: LinuxNonceJournalRecord,
    aes_key: &[u8; 32],
    options: super::ImmutableSourceStreamingWriteOptions,
    limits: super::ImmutableLimits,
    fresh_session: &mut DescriptorEncryptionSession,
) -> super::CandidateResult<(EncryptedDescriptorStage, RestartRecoveredPreflight)> {
    if manifest.role != EncryptedRestartStageRole::SortedDescriptorSpill
        || manifest.key_id != linux_nonce_key_id(aes_key)
        || nonce_record.key_id != manifest.key_id
        || nonce_record.nonce_prefix != manifest.nonce_prefix
        || nonce_record.operation_id != manifest.operation_id
        || nonce_record.generation != manifest.generation
    {
        return Err("restart spill context mismatch".into());
    }
    if fresh_session.journal_generation <= manifest.generation {
        return Err("fresh descriptor generation".into());
    }
    if linux_nonce_key_id(&fresh_session.key) != manifest.key_id
        || fresh_session.nonce_prefix != manifest.nonce_prefix
    {
        return Err("fresh descriptor key context".into());
    }
    if options.output.max_write_request_bytes == 0 || options.max_source_read_bytes == 0 {
        return Err("streaming configuration".into());
    }

    let stage_length = persisted_stage
        .metadata()
        .map_err(|error| error.to_string())?
        .len();
    if stage_length != manifest.stage_length
        || stage_length == 0
        || stage_length
            % u64::try_from(ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES)
                .expect("encrypted spill width fits u64")
            != 0
    {
        return Err("restart spill stage length".into());
    }
    let object_count_u64 = stage_length
        / u64::try_from(ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES)
            .expect("encrypted spill width fits u64");
    let object_count =
        usize::try_from(object_count_u64).map_err(|_| "restart object count".to_owned())?;
    if object_count == 0 || object_count > limits.max_objects {
        return Err("object count limit".into());
    }
    if fresh_session.remaining() < object_count_u64 {
        return Err("fresh descriptor nonce lease capacity".into());
    }

    super::allocation_check::<super::Locator>(super::LEAF_CAPACITY, limits)
        .map_err(|error| error.to_string())?;
    super::allocation_check::<super::PageRef>(super::INTERNAL_FANOUT, limits)
        .map_err(|error| error.to_string())?;

    let old_lease_size = nonce_record
        .lease_last
        .checked_sub(nonce_record.lease_first)
        .and_then(|delta| delta.checked_add(1))
        .ok_or_else(|| "restart old lease range".to_owned())?;
    if old_lease_size
        != object_count_u64
            .checked_mul(2)
            .ok_or_else(|| "restart old lease size".to_owned())?
    {
        return Err("restart old lease size".into());
    }
    let old_spill_counter_end = nonce_record
        .lease_first
        .checked_add(object_count_u64)
        .ok_or_else(|| "restart old spill counter range".to_owned())?;

    let old_key = descriptor_key(aes_key)?;
    let fresh_key = descriptor_key(&fresh_session.key)?;
    let mut reader = persisted_stage.try_clone().map_err(|error| error.to_string())?;
    std::io::Seek::seek(&mut reader, std::io::SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut retained = super::FixedStage::create(
        work_directory,
        "restart-encrypted-source-descriptors",
        ENCRYPTED_DESCRIPTOR_STAGE_BYTES,
    )?;
    let mut retained_writer = retained.writer()?;
    let mut seen_old_counters = vec![false; object_count];
    let mut previous_object_id = None;
    let mut first_fresh_counter = None;
    let mut previous_fresh_counter: Option<u64> = None;
    let mut object_bytes = 0usize;
    let mut largest_source_buffer = 0usize;

    for sequence_index in 0..object_count {
        let mut frame = [0u8; ENCRYPTED_DESCRIPTOR_SPILL_PAYLOAD_BYTES];
        reader
            .read_exact(&mut frame)
            .map_err(|error| error.to_string())?;
        let object_id = u64::from_le_bytes(
            frame[..ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES]
                .try_into()
                .expect("restart spill object id"),
        );
        if object_id == 0 || previous_object_id.is_some_and(|previous| object_id <= previous) {
            return Err("restart spill ordering".into());
        }
        let nonce: [u8; ENCRYPTED_DESCRIPTOR_NONCE_BYTES] = frame
            [ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES
                ..ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES + ENCRYPTED_DESCRIPTOR_NONCE_BYTES]
            .try_into()
            .expect("restart spill nonce");
        let old_counter = spill_counter_from_nonce(&nonce, manifest.nonce_prefix)?;
        if old_counter < nonce_record.lease_first || old_counter >= old_spill_counter_end {
            return Err("restart spill nonce range".into());
        }
        let old_index = usize::try_from(old_counter - nonce_record.lease_first)
            .map_err(|_| "restart spill nonce index".to_owned())?;
        if seen_old_counters[old_index] {
            return Err("restart spill duplicate nonce".into());
        }
        seen_old_counters[old_index] = true;
        let old_aad = descriptor_spill_aad(
            manifest.operation_id,
            manifest.generation,
            object_id,
            old_counter,
        );
        let mut protected = frame[ENCRYPTED_DESCRIPTOR_SPILL_KEY_BYTES
            + ENCRYPTED_DESCRIPTOR_NONCE_BYTES..]
            .to_vec();
        let plaintext = old_key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(old_aad.as_slice()),
                &mut protected,
            )
            .map_err(|_| "restart spill authentication".to_owned())?;
        if plaintext.len() != super::DESCRIPTOR_STAGE_BYTES {
            return Err("restart spill plaintext width".into());
        }
        let mut descriptor_bytes = [0u8; super::DESCRIPTOR_STAGE_BYTES];
        descriptor_bytes.copy_from_slice(plaintext);
        let descriptor = super::SourceDescriptor::decode(&descriptor_bytes)?;
        if descriptor.object_id != object_id {
            return Err("restart spill object id mismatch".into());
        }
        let logical_len =
            usize::try_from(descriptor.logical_len).map_err(|_| "object size".to_owned())?;
        let record_len = super::OBJECT_HEADER_LEN
            .checked_add(logical_len)
            .ok_or_else(|| "object size".to_owned())?;
        object_bytes = object_bytes
            .checked_add(record_len)
            .ok_or_else(|| "output size".to_owned())?;
        largest_source_buffer = largest_source_buffer
            .max(logical_len.min(options.max_source_read_bytes));
        if largest_source_buffer > limits.max_allocation_bytes {
            return Err("source buffer allocation limit".into());
        }
        previous_object_id = Some(object_id);

        let fresh_counter = fresh_session.allocate_counter()?;
        if let Some(previous) = previous_fresh_counter {
            if previous.checked_add(1) != Some(fresh_counter) {
                return Err("fresh descriptor nonce lease discontinuity".into());
            }
        } else {
            first_fresh_counter = Some(fresh_counter);
        }
        previous_fresh_counter = Some(fresh_counter);
        let sequence =
            u64::try_from(sequence_index).map_err(|_| "restart descriptor sequence".to_owned())?;
        let fresh_nonce = descriptor_nonce(fresh_session.nonce_prefix, fresh_counter);
        let fresh_aad = descriptor_aad(
            fresh_session.operation_id,
            fresh_session.journal_generation,
            sequence,
            fresh_counter,
        );
        let mut fresh_protected = descriptor_bytes.to_vec();
        fresh_key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(fresh_nonce),
                Aad::from(fresh_aad.as_slice()),
                &mut fresh_protected,
            )
            .map_err(|_| "restart retained descriptor encryption".to_owned())?;
        retained_writer
            .write_all(&fresh_nonce)
            .and_then(|_| retained_writer.write_all(&fresh_protected))
            .map_err(|error| error.to_string())?;
        retained.note_record()?;
    }
    let mut trailing = [0u8; 1];
    if reader
        .read(&mut trailing)
        .map_err(|error| error.to_string())?
        != 0
    {
        return Err("restart spill trailing bytes".into());
    }
    if seen_old_counters.iter().any(|seen| !seen) {
        return Err("restart spill nonce coverage".into());
    }
    retained_writer.flush().map_err(|error| error.to_string())?;
    drop(retained_writer);
    retained.validate_bytes()?;

    let (expected_pages, expected_root_level) =
        super::streaming_tree_shape(object_count, limits).map_err(|error| error.to_string())?;
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

    Ok((
        EncryptedDescriptorStage {
            stage: retained,
            nonce_prefix: fresh_session.nonce_prefix,
            operation_id: fresh_session.operation_id,
            journal_generation: fresh_session.journal_generation,
            first_counter: first_fresh_counter
                .ok_or_else(|| "fresh descriptor nonce start".to_owned())?,
        },
        RestartRecoveredPreflight {
            object_count,
            expected_bytes,
            expected_pages,
            expected_root_level,
            largest_source_buffer,
            version_checks: object_count_u64,
        },
    ))
}

fn continue_verified_encrypted_spill_with_fresh_lease<W, S>(
    journal: &LinuxDurableNonceJournal,
    stage_directory_path: &Path,
    work_directory: &Path,
    writer: &mut W,
    sources: &mut [S],
    aes_key: [u8; 32],
    crashed_generation: u64,
    trusted_floor: Option<TrustedNonceFloor>,
    restart_limits: LinuxEncryptedStageRestartLimits,
    options: super::ImmutableSourceStreamingWriteOptions,
    limits: super::ImmutableLimits,
    fresh_operation_id: [u8; 16],
) -> super::CandidateResult<EncryptedRestartContinuationEvidence>
where
    W: Write,
    S: super::ImmutableStreamingPayloadSource,
{
    let (disposition, _) = classify_encrypted_spill_restart(
        journal,
        stage_directory_path,
        &aes_key,
        crashed_generation,
        trusted_floor,
        restart_limits,
    )
    .map_err(|error| error.to_string())?;
    let object_count = match &disposition {
        EncryptedStageRestartDisposition::VerifiedExactNeedsFreshLease { object_count }
        | EncryptedStageRestartDisposition::VerifiedRenamedNeedsFreshLease {
            object_count, ..
        } => *object_count,
        EncryptedStageRestartDisposition::NoDurableManifestRestartWork => {
            return Err("restart stage has no durable manifest".into())
        }
        EncryptedStageRestartDisposition::StageAbsentRestartWork => {
            return Err("restart stage is absent".into())
        }
        EncryptedStageRestartDisposition::RetainIndeterminate => {
            return Err("restart stage is indeterminate".into())
        }
    };
    if sources.len() != object_count {
        return Err("restart source count".into());
    }

    let stage_directory = linux_nonce_open_private_directory(stage_directory_path)
        .map_err(|error| error.to_string())?;
    let (manifest, crashed_nonce_record, persisted_stage) = open_verified_restart_stage(
        journal,
        &stage_directory,
        crashed_generation,
        &disposition,
    )?;
    verify_manifest_bound_stage_identity(
        &persisted_stage,
        manifest,
        restart_limits.max_identity_bytes,
    )
    .map_err(|error| error.to_string())?;

    let mut authority = journal
        .recover_authority(trusted_floor)
        .map_err(|error| error.to_string())?;
    if authority.durable.generation != crashed_generation {
        return Err("restart journal generation advanced".into());
    }
    let fresh_lease_size =
        u64::try_from(object_count).map_err(|_| "fresh restart lease size".to_owned())?;
    let mut fresh_session = journal
        .commit_descriptor_session(
            &mut authority,
            aes_key,
            fresh_operation_id,
            fresh_lease_size,
            JournalCommitCut::Complete,
        )
        .map_err(|error| error.to_string())?;
    let fresh_lease_first = fresh_session.lease.first;
    let fresh_lease_last = fresh_session.lease.last;

    let (retained_stage, recovered) = transcode_restart_spill_with_fresh_session(
        work_directory,
        &persisted_stage,
        manifest,
        crashed_nonce_record,
        &aes_key,
        options,
        limits,
        &mut fresh_session,
    )?;
    if fresh_session.remaining() != 0 {
        return Err("fresh restart lease not exhausted".into());
    }
    if retained_stage.records() != object_count {
        return Err("restart retained descriptor count".into());
    }
    retained_stage.verify_all(&fresh_session)?;
    let descriptor_stage_bytes = retained_stage.bytes()?;
    let descriptor_ciphertext_sha256 = Some(retained_stage.ciphertext_sha256()?);
    let descriptor_reader = retained_stage.reader(&fresh_session)?;
    let descriptor_spill = reconstructed_restart_spill_report(
        recovered.object_count,
        manifest.stage_length,
        manifest.stage_sha256,
    )?;
    let emission = super::PreparedEmission {
        descriptor_stage_bytes,
        descriptor_ciphertext_sha256,
        descriptor_spill,
        expected_bytes: recovered.expected_bytes,
        expected_pages: recovered.expected_pages,
        expected_root_level: recovered.expected_root_level,
        largest_source_buffer: recovered.largest_source_buffer,
        version_checks: recovered.version_checks,
        object_count: recovered.object_count,
    };
    let output = super::write_prepared_from_descriptor_reader(
        writer,
        sources,
        work_directory,
        options,
        limits,
        emission,
        descriptor_reader,
    )?;
    drop(retained_stage);

    Ok(EncryptedRestartContinuationEvidence {
        output,
        crashed_generation,
        fresh_generation: fresh_session.journal_generation,
        crashed_lease_first: crashed_nonce_record.lease_first,
        crashed_lease_last: crashed_nonce_record.lease_last,
        fresh_lease_first,
        fresh_lease_last,
        persisted_spill_sha256: manifest.stage_sha256,
    })
}
