use std::cell::RefCell;

#[derive(Clone, Copy)]
struct EncryptedSorterPipelineSettings {
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    spill_limits: BoundedSpillSortLimits,
}

struct EncryptedSorterPreflight {
    descriptor_stage: EncryptedDescriptorStage,
    descriptor_spill: BoundedSpillSortReport,
    expected_bytes: usize,
    expected_pages: usize,
    expected_root_level: u8,
    largest_source_buffer: usize,
    version_checks: u64,
    object_count: usize,
}

fn prepare_encrypted_sorter_preflight<S: ImmutableStreamingPayloadSource>(
    directory: &Path,
    sources: &mut [S],
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    spill_limits: BoundedSpillSortLimits,
    spill_session: &mut DescriptorEncryptionSession,
    retained_session: &mut DescriptorEncryptionSession,
) -> CandidateResult<EncryptedSorterPreflight> {
    if sources.is_empty() || sources.len() > limits.max_objects {
        return Err("object count limit".into());
    }
    if options.output.max_write_request_bytes == 0 || options.max_source_read_bytes == 0 {
        return Err("streaming configuration".into());
    }
    if spill_limits.record_bytes != DESCRIPTOR_STAGE_BYTES {
        return Err("descriptor spill record size".into());
    }

    let object_count = sources.len();
    let required_nonces = u64::try_from(object_count).map_err(|_| "descriptor nonce count")?;
    if spill_session.remaining() < required_nonces {
        return Err("encrypted sorter nonce lease capacity".into());
    }
    if retained_session.remaining() < required_nonces {
        return Err("retained descriptor nonce lease capacity".into());
    }

    allocation_check::<Locator>(LEAF_CAPACITY, limits).map_err(|error| error.to_string())?;
    allocation_check::<PageRef>(INTERNAL_FANOUT, limits).map_err(|error| error.to_string())?;

    let mut input_error = None;
    let encryption_error = RefCell::new(None::<String>);
    let mut object_bytes = 0usize;
    let mut largest_source_buffer = 0usize;
    let mut version_checks = 0u64;
    let spill_context = DescriptorCryptoContext::from_session(spill_session);
    let records = DescriptorRecords {
        sources: sources.iter_mut().enumerate(),
        options,
        limits,
        input_error: &mut input_error,
        object_bytes: &mut object_bytes,
        largest_source_buffer: &mut largest_source_buffer,
        version_checks: &mut version_checks,
        failed: false,
    };
    let encrypted_records = records.map(|record| {
        match encrypt_descriptor_for_sorter(record, spill_session) {
            Ok(record) => record,
            Err(error) => {
                let mut slot = encryption_error.borrow_mut();
                if slot.is_none() {
                    *slot = Some(error);
                }
                BoundedSpillRecord::new(0, vec![0u8; ENCRYPTED_SORTER_PAYLOAD_BYTES])
            }
        }
    });
    let sorted = sort_encrypted_descriptors_to_retained_stage(
        directory,
        encrypted_records,
        spill_limits,
        object_count,
        spill_context,
        retained_session,
    );
    if let Some(error) = input_error {
        return Err(error);
    }
    if let Some(error) = encryption_error.into_inner() {
        return Err(error);
    }
    let (descriptor_stage, descriptor_spill) = sorted?;

    let source_count = u64::try_from(object_count).map_err(|_| "object count".to_owned())?;
    let expected_sorter_payload_bytes = source_count
        .checked_mul(
            u64::try_from(ENCRYPTED_SORTER_PAYLOAD_BYTES).expect("encrypted sorter width fits u64"),
        )
        .ok_or_else(|| "encrypted sorter output size".to_owned())?;
    let expected_retained_bytes = source_count
        .checked_mul(
            u64::try_from(ENCRYPTED_DESCRIPTOR_STAGE_BYTES)
                .expect("encrypted descriptor width fits u64"),
        )
        .ok_or_else(|| "encrypted descriptor stage size".to_owned())?;
    let stage_bytes = descriptor_stage.bytes()?;
    if descriptor_spill.output_records != source_count
        || descriptor_spill.output_payload_bytes != expected_sorter_payload_bytes
        || descriptor_stage.records() != object_count
        || stage_bytes != expected_retained_bytes
    {
        return Err("encrypted descriptor sorter stage size".into());
    }

    let (expected_pages, expected_root_level) =
        streaming_tree_shape(object_count, limits).map_err(|error| error.to_string())?;
    let page_bytes = expected_pages
        .checked_mul(PAGE_SIZE)
        .ok_or_else(|| "page output size".to_owned())?;
    let expected_bytes = FILE_HEADER_LEN
        .checked_add(object_bytes)
        .and_then(|value| value.checked_add(page_bytes))
        .and_then(|value| value.checked_add(SNAPSHOT_LEN))
        .and_then(|value| value.checked_add(FOOTER_LEN))
        .ok_or_else(|| "output size".to_owned())?;
    if expected_bytes > limits.max_output_bytes {
        return Err("output limit".into());
    }
    if expected_bytes > limits.max_file_bytes {
        return Err("file size limit".into());
    }

    Ok(EncryptedSorterPreflight {
        descriptor_stage,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count,
    })
}

fn write_genesis_sources_end_to_end_encrypted_sorter_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    settings: EncryptedSorterPipelineSettings,
    spill_session: &mut DescriptorEncryptionSession,
    retained_session: &mut DescriptorEncryptionSession,
) -> CandidateResult<EndToEndEvidence>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
    let preflight = prepare_encrypted_sorter_preflight(
        directory,
        sources,
        settings.options,
        settings.limits,
        settings.spill_limits,
        spill_session,
        retained_session,
    )?;
    let EncryptedSorterPreflight {
        descriptor_stage,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count,
    } = preflight;

    descriptor_stage.verify_all(retained_session)?;
    let descriptor_stage_bytes = descriptor_stage.bytes()?;
    let descriptor_ciphertext_sha256 = Some(descriptor_stage.ciphertext_sha256()?);
    let descriptor_reader = descriptor_stage.reader(retained_session)?;
    let emission = PreparedEmission {
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
    let result = write_prepared_from_descriptor_reader(
        writer,
        sources,
        directory,
        settings.options,
        settings.limits,
        emission,
        descriptor_reader,
    );
    drop(descriptor_stage);
    result
}
