struct EncryptedTreeBuildEvidence {
    root: PageRef,
    page_count: usize,
    peak_locator_entries: usize,
    peak_page_ref_entries: usize,
    peak_live_tree_stage_bytes: u64,
    stage_ciphertext_sha256: [u8; 32],
}

struct EncryptedTreeEndToEndEvidence {
    base: EndToEndEvidence,
    tree_stage_ciphertext_sha256: [u8; 32],
}

fn encrypted_tree_stage_record_count(object_count: usize) -> CandidateResult<u64> {
    if object_count == 0 {
        return Err("encrypted tree object count".into());
    }
    let mut total = u64::try_from(object_count).map_err(|_| "tree nonce count".to_owned())?;
    let mut current = groups(object_count, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY)?.len();
    loop {
        total = total
            .checked_add(u64::try_from(current).map_err(|_| "tree nonce count".to_owned())?)
            .ok_or_else(|| "tree nonce count overflow".to_owned())?;
        if current == 1 {
            break;
        }
        current = groups(current, INTERNAL_FANOUT, INTERNAL_MIN_OCCUPANCY)?.len();
    }
    Ok(total)
}

fn absorb_encrypted_stage_digest(
    hasher: &mut Sha256,
    stage_ordinal: u64,
    digest: [u8; 32],
) {
    hasher.update(stage_ordinal.to_le_bytes());
    hasher.update(digest);
}

fn build_encrypted_staged_tree<W: Write>(
    sink: &mut StreamingSink<'_, W>,
    directory: &Path,
    locator_stage: EncryptedRecordStage,
    session: &mut DescriptorEncryptionSession,
    limits: ImmutableLimits,
) -> CandidateResult<EncryptedTreeBuildEvidence> {
    locator_stage.verify_all(session)?;
    let locator_stage_bytes = locator_stage.bytes()?;
    let mut stage_digest = Sha256::new();
    absorb_encrypted_stage_digest(
        &mut stage_digest,
        0,
        locator_stage.ciphertext_sha256()?,
    );
    let mut locator_reader = locator_stage.reader(session)?;
    let mut leaf_stage = EncryptedRecordStage::create(
        directory,
        "encrypted-leaf-refs",
        PAGE_REF_STAGE_BYTES,
        EncryptedTreeStageKind::PageRef,
        1,
        session,
    )?;
    let mut leaf_writer = leaf_stage.writer(session)?;
    let mut pages = 0usize;
    let mut peak_locator_entries = 0usize;

    for size in groups(locator_stage.records(), LEAF_CAPACITY, LEAF_MIN_OCCUPANCY)? {
        peak_locator_entries = peak_locator_entries.max(size);
        let mut locators = Vec::with_capacity(size);
        for _ in 0..size {
            let raw: [u8; LOCATOR_STAGE_BYTES] = locator_reader
                .read_record()?
                .try_into()
                .map_err(|_| "encrypted locator width".to_owned())?;
            locators.push(decode_locator(&raw)?);
        }
        let reference = sink
            .write_page(&encode_leaf(&locators).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        leaf_writer.write_record(&encode_page_ref(&reference))?;
        pages = pages
            .checked_add(1)
            .ok_or_else(|| "page count overflow".to_owned())?;
    }
    locator_reader.finish()?;
    leaf_writer.finish()?;
    leaf_stage.verify_all(session)?;
    let leaf_stage_bytes = leaf_stage.bytes()?;
    absorb_encrypted_stage_digest(&mut stage_digest, 1, leaf_stage.ciphertext_sha256()?);
    let mut peak_live_tree_stage_bytes = locator_stage_bytes
        .checked_add(leaf_stage_bytes)
        .ok_or_else(|| "encrypted tree stage byte overflow".to_owned())?;
    drop(locator_reader);
    drop(locator_stage);

    let mut current = leaf_stage;
    let mut current_ordinal = 1u64;
    let mut peak_page_ref_entries = 1usize;
    while current.records() > 1 {
        current.verify_all(session)?;
        let mut reader = current.reader(session)?;
        let first_raw: [u8; PAGE_REF_STAGE_BYTES] = reader
            .read_record()?
            .try_into()
            .map_err(|_| "encrypted page-ref width".to_owned())?;
        let first = decode_page_ref(&first_raw)?;
        let parent_level = first
            .level
            .checked_add(1)
            .ok_or_else(|| "page depth overflow".to_owned())?;
        if parent_level > limits.max_depth {
            return Err("page depth limit".into());
        }
        let next_ordinal = current_ordinal
            .checked_add(1)
            .ok_or_else(|| "encrypted tree stage ordinal overflow".to_owned())?;
        let mut next = EncryptedRecordStage::create(
            directory,
            "encrypted-parent-refs",
            PAGE_REF_STAGE_BYTES,
            EncryptedTreeStageKind::PageRef,
            next_ordinal,
            session,
        )?;
        let mut next_writer = next.writer(session)?;
        let mut pending_first = Some(first);
        for size in groups(current.records(), INTERNAL_FANOUT, INTERNAL_MIN_OCCUPANCY)? {
            peak_page_ref_entries = peak_page_ref_entries.max(size);
            let mut children = Vec::with_capacity(size);
            for _ in 0..size {
                let child = if let Some(first) = pending_first.take() {
                    first
                } else {
                    let raw: [u8; PAGE_REF_STAGE_BYTES] = reader
                        .read_record()?
                        .try_into()
                        .map_err(|_| "encrypted page-ref width".to_owned())?;
                    decode_page_ref(&raw)?
                };
                if child
                    .level
                    .checked_add(1)
                    .ok_or_else(|| "page-ref level overflow".to_owned())?
                    != parent_level
                {
                    return Err("page-ref level mismatch".into());
                }
                children.push(child);
            }
            let reference = sink
                .write_page(
                    &encode_internal(&children, parent_level)
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            next_writer.write_record(&encode_page_ref(&reference))?;
            pages = pages
                .checked_add(1)
                .ok_or_else(|| "page count overflow".to_owned())?;
        }
        reader.finish()?;
        next_writer.finish()?;
        next.verify_all(session)?;
        let current_bytes = current.bytes()?;
        let next_bytes = next.bytes()?;
        let live_bytes = current_bytes
            .checked_add(next_bytes)
            .ok_or_else(|| "encrypted page-ref live-byte overflow".to_owned())?;
        peak_live_tree_stage_bytes = peak_live_tree_stage_bytes.max(live_bytes);
        absorb_encrypted_stage_digest(
            &mut stage_digest,
            next_ordinal,
            next.ciphertext_sha256()?,
        );
        drop(reader);
        drop(current);
        current = next;
        current_ordinal = next_ordinal;
    }

    current.verify_all(session)?;
    let mut reader = current.reader(session)?;
    let raw: [u8; PAGE_REF_STAGE_BYTES] = reader
        .read_record()?
        .try_into()
        .map_err(|_| "encrypted root page-ref width".to_owned())?;
    let root = decode_page_ref(&raw)?;
    reader.finish()?;
    drop(reader);
    drop(current);

    Ok(EncryptedTreeBuildEvidence {
        root,
        page_count: pages,
        peak_locator_entries,
        peak_page_ref_entries,
        peak_live_tree_stage_bytes,
        stage_ciphertext_sha256: stage_digest.finalize().into(),
    })
}

fn write_prepared_encrypted_tree_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    settings: EncryptedSorterPipelineSettings,
    preflight: EncryptedSorterPreflight,
    retained_session: &DescriptorEncryptionSession,
    tree_session: &mut DescriptorEncryptionSession,
) -> CandidateResult<EncryptedTreeEndToEndEvidence>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
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
    let mut descriptor_reader = descriptor_stage.reader(retained_session)?;

    let mut sink = StreamingSink::new(writer, settings.options.output.max_write_request_bytes)
        .map_err(|error| error.to_string())?;
    let mut header = [0u8; FILE_HEADER_LEN];
    header[..8].copy_from_slice(FILE_MAGIC);
    sink.write_commit_bytes(&header)
        .map_err(|error| error.to_string())?;

    let mut locator_stage = EncryptedRecordStage::create(
        directory,
        "encrypted-locators",
        LOCATOR_STAGE_BYTES,
        EncryptedTreeStageKind::Locator,
        0,
        tree_session,
    )?;
    let mut locator_writer = locator_stage.writer(tree_session)?;
    let mut buffer = vec![0u8; largest_source_buffer];
    let mut counters = SourceStreamingCounters {
        version_checks,
        ..SourceStreamingCounters::default()
    };

    for _ in 0..object_count {
        let descriptor = descriptor_reader.read_source_descriptor()?;
        let index = usize::try_from(descriptor.source_index).map_err(|_| "source index")?;
        let source = sources.get_mut(index).ok_or("source index")?;
        if source.object_id() != descriptor.object_id
            || source.kind() != descriptor.kind
            || source.logical_len() != descriptor.logical_len
        {
            return Err(format!(
                "source {} metadata changed after preflight",
                descriptor.object_id
            ));
        }
        let logical_len =
            usize::try_from(descriptor.logical_len).map_err(|_| "object size".to_owned())?;
        let locator = write_source_streaming_object(
            &mut sink,
            source,
            descriptor.strong_version,
            logical_len,
            &mut buffer,
            &mut counters,
        )
        .map_err(|error| error.to_string())?;
        locator_writer.write_record(&encode_locator(&locator))?;
    }
    descriptor_reader.finish()?;
    locator_writer.finish()?;
    locator_stage.verify_all(tree_session)?;
    let locator_stage_bytes = locator_stage.bytes()?;
    let object_phase_live_stage_bytes = descriptor_stage_bytes
        .checked_add(locator_stage_bytes)
        .ok_or_else(|| "encrypted retained locator overlap overflow".to_owned())?;
    drop(descriptor_reader);
    drop(descriptor_stage);

    let tree = build_encrypted_staged_tree(
        &mut sink,
        directory,
        locator_stage,
        tree_session,
        settings.limits,
    )?;
    if tree.page_count != expected_pages || tree.root.level != expected_root_level {
        return Err("streaming tree shape".into());
    }
    let mut report = write_streaming_publication(&mut sink, &tree.root, tree.page_count)
        .map_err(|error| error.to_string())?;
    report.object_count = object_count;
    if sink.offset != expected_bytes {
        return Err("streaming output length".into());
    }

    Ok(EncryptedTreeEndToEndEvidence {
        base: EndToEndEvidence {
            output: ImmutableSourceStreamingWriteReport {
                output: ImmutableStreamingWriteReport {
                    report,
                    bytes_written: sink.offset,
                    largest_write_request: sink.largest_write_request,
                    locator_entries: object_count,
                },
                source_read_operations: counters.source_read_operations,
                source_bytes_read: counters.source_bytes_read,
                version_checks: counters.version_checks,
                largest_source_buffer: buffer.len(),
            },
            descriptor_stage_bytes,
            descriptor_ciphertext_sha256,
            descriptor_spill,
            peak_locator_entries: tree.peak_locator_entries,
            peak_page_ref_entries: tree.peak_page_ref_entries,
            peak_live_retained_stage_bytes: object_phase_live_stage_bytes
                .max(tree.peak_live_tree_stage_bytes),
        },
        tree_stage_ciphertext_sha256: tree.stage_ciphertext_sha256,
    })
}

fn write_genesis_sources_end_to_end_encrypted_tree_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    settings: EncryptedSorterPipelineSettings,
    spill_session: &mut DescriptorEncryptionSession,
    retained_session: &mut DescriptorEncryptionSession,
    tree_session: &mut DescriptorEncryptionSession,
) -> CandidateResult<EncryptedTreeEndToEndEvidence>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
    let tree_nonces = encrypted_tree_stage_record_count(sources.len())?;
    if tree_session.remaining() < tree_nonces {
        return Err("encrypted tree nonce lease capacity".into());
    }
    let preflight = prepare_encrypted_sorter_preflight(
        directory,
        sources,
        settings.options,
        settings.limits,
        settings.spill_limits,
        spill_session,
        retained_session,
    )?;
    write_prepared_encrypted_tree_candidate(
        writer,
        sources,
        directory,
        settings,
        preflight,
        retained_session,
        tree_session,
    )
}
