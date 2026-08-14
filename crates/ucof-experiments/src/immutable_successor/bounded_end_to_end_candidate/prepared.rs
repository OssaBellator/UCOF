fn write_prepared_bounded_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    preflight: BoundedPreflight,
) -> CandidateResult<EndToEndEvidence>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
    let descriptor_stage_bytes = preflight.descriptor_stage.validate_bytes()?;
    let descriptor_spill = preflight.descriptor_spill.clone();
    let expected_bytes = preflight.expected_bytes;
    let expected_pages = preflight.expected_pages;
    let expected_root_level = preflight.expected_root_level;
    let object_count = preflight.object_count;
    let descriptor_stage = preflight.descriptor_stage;

    let mut sink = StreamingSink::new(writer, options.output.max_write_request_bytes)
        .map_err(|error| error.to_string())?;
    let mut header = [0u8; FILE_HEADER_LEN];
    header[..8].copy_from_slice(FILE_MAGIC);
    sink.write_commit_bytes(&header)
        .map_err(|error| error.to_string())?;

    let mut locator_stage = FixedStage::create(directory, "locators", LOCATOR_STAGE_BYTES)?;
    let mut locator_writer = locator_stage.writer()?;
    let mut descriptor_reader = descriptor_stage.reader()?;
    let mut raw_descriptor = [0u8; DESCRIPTOR_STAGE_BYTES];
    let mut buffer = vec![0u8; preflight.largest_source_buffer];
    let mut counters = SourceStreamingCounters {
        version_checks: preflight.version_checks,
        ..SourceStreamingCounters::default()
    };

    for _ in 0..object_count {
        descriptor_reader
            .read_exact(&mut raw_descriptor)
            .map_err(|error| error.to_string())?;
        let descriptor = SourceDescriptor::decode(&raw_descriptor)?;
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
        locator_writer
            .write_all(&encode_locator(&locator))
            .map_err(|error| error.to_string())?;
        locator_stage.note_record()?;
    }
    read_exact_end(&mut descriptor_reader, "descriptor stage")?;
    locator_writer.flush().map_err(|error| error.to_string())?;
    drop(locator_writer);
    let locator_stage_bytes = locator_stage.validate_bytes()?;
    let object_phase_live_stage_bytes = descriptor_stage_bytes
        .checked_add(locator_stage_bytes)
        .ok_or_else(|| "retained stage byte overflow".to_owned())?;
    drop(descriptor_reader);
    drop(descriptor_stage);

    let tree = build_staged_tree(&mut sink, directory, locator_stage, limits)?;
    if tree.page_count != expected_pages || tree.root.level != expected_root_level {
        return Err("streaming tree shape".into());
    }
    let mut report = write_streaming_publication(&mut sink, &tree.root, tree.page_count)
        .map_err(|error| error.to_string())?;
    report.object_count = object_count;
    if sink.offset != expected_bytes {
        return Err("streaming output length".into());
    }

    Ok(EndToEndEvidence {
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
        descriptor_spill,
        peak_locator_entries: tree.peak_locator_entries,
        peak_page_ref_entries: tree.peak_page_ref_entries,
        peak_live_retained_stage_bytes: object_phase_live_stage_bytes
            .max(tree.peak_live_tree_stage_bytes),
    })
}
