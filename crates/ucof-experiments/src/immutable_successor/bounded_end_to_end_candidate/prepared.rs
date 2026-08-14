trait PreparedDescriptorReader {
    fn read_source_descriptor(&mut self) -> CandidateResult<SourceDescriptor>;
    fn finish(&mut self) -> CandidateResult<()>;
}

struct PlainPreparedDescriptorReader {
    reader: BufReader<File>,
}

impl PreparedDescriptorReader for PlainPreparedDescriptorReader {
    fn read_source_descriptor(&mut self) -> CandidateResult<SourceDescriptor> {
        let mut raw = [0u8; DESCRIPTOR_STAGE_BYTES];
        self.reader
            .read_exact(&mut raw)
            .map_err(|error| error.to_string())?;
        SourceDescriptor::decode(&raw)
    }

    fn finish(&mut self) -> CandidateResult<()> {
        read_exact_end(&mut self.reader, "descriptor stage")
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl PreparedDescriptorReader for EncryptedDescriptorReader {
    fn read_source_descriptor(&mut self) -> CandidateResult<SourceDescriptor> {
        SourceDescriptor::decode(&self.read_descriptor()?)
    }

    fn finish(&mut self) -> CandidateResult<()> {
        EncryptedDescriptorReader::finish(self)
    }
}

struct PreparedEmission {
    descriptor_stage_bytes: u64,
    descriptor_ciphertext_sha256: Option<[u8; 32]>,
    descriptor_spill: BoundedSpillSortReport,
    expected_bytes: usize,
    expected_pages: usize,
    expected_root_level: u8,
    largest_source_buffer: usize,
    version_checks: u64,
    object_count: usize,
}

fn write_prepared_from_descriptor_reader<W, S, R>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    emission: PreparedEmission,
    mut descriptor_reader: R,
) -> CandidateResult<EndToEndEvidence>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
    R: PreparedDescriptorReader,
{
    let mut sink = StreamingSink::new(writer, options.output.max_write_request_bytes)
        .map_err(|error| error.to_string())?;
    let mut header = [0u8; FILE_HEADER_LEN];
    header[..8].copy_from_slice(FILE_MAGIC);
    sink.write_commit_bytes(&header)
        .map_err(|error| error.to_string())?;

    let mut locator_stage = FixedStage::create(directory, "locators", LOCATOR_STAGE_BYTES)?;
    let mut locator_writer = locator_stage.writer()?;
    let mut buffer = vec![0u8; emission.largest_source_buffer];
    let mut counters = SourceStreamingCounters {
        version_checks: emission.version_checks,
        ..SourceStreamingCounters::default()
    };

    for _ in 0..emission.object_count {
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
        locator_writer
            .write_all(&encode_locator(&locator))
            .map_err(|error| error.to_string())?;
        locator_stage.note_record()?;
    }
    descriptor_reader.finish()?;
    locator_writer.flush().map_err(|error| error.to_string())?;
    drop(locator_writer);
    let locator_stage_bytes = locator_stage.validate_bytes()?;
    let object_phase_live_stage_bytes = emission
        .descriptor_stage_bytes
        .checked_add(locator_stage_bytes)
        .ok_or_else(|| "retained stage byte overflow".to_owned())?;
    drop(descriptor_reader);

    let tree = build_staged_tree(&mut sink, directory, locator_stage, limits)?;
    if tree.page_count != emission.expected_pages || tree.root.level != emission.expected_root_level {
        return Err("streaming tree shape".into());
    }
    let mut report = write_streaming_publication(&mut sink, &tree.root, tree.page_count)
        .map_err(|error| error.to_string())?;
    report.object_count = emission.object_count;
    if sink.offset != emission.expected_bytes {
        return Err("streaming output length".into());
    }

    Ok(EndToEndEvidence {
        output: ImmutableSourceStreamingWriteReport {
            output: ImmutableStreamingWriteReport {
                report,
                bytes_written: sink.offset,
                largest_write_request: sink.largest_write_request,
                locator_entries: emission.object_count,
            },
            source_read_operations: counters.source_read_operations,
            source_bytes_read: counters.source_bytes_read,
            version_checks: counters.version_checks,
            largest_source_buffer: buffer.len(),
        },
        descriptor_stage_bytes: emission.descriptor_stage_bytes,
        descriptor_ciphertext_sha256: emission.descriptor_ciphertext_sha256,
        descriptor_spill: emission.descriptor_spill,
        peak_locator_entries: tree.peak_locator_entries,
        peak_page_ref_entries: tree.peak_page_ref_entries,
        peak_live_retained_stage_bytes: object_phase_live_stage_bytes
            .max(tree.peak_live_tree_stage_bytes),
    })
}

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
    let BoundedPreflight {
        descriptor_stage,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count,
    } = preflight;
    let descriptor_stage_bytes = descriptor_stage.validate_bytes()?;
    let descriptor_reader = PlainPreparedDescriptorReader {
        reader: descriptor_stage.reader()?,
    };
    let emission = PreparedEmission {
        descriptor_stage_bytes,
        descriptor_ciphertext_sha256: None,
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
        options,
        limits,
        emission,
        descriptor_reader,
    );
    drop(descriptor_stage);
    result
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[derive(Clone, Copy)]
struct EncryptedPreparedSettings {
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn write_prepared_encrypted_bounded_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    preflight: BoundedPreflight,
    session: &mut DescriptorEncryptionSession,
) -> CandidateResult<EndToEndEvidence>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
    write_prepared_encrypted_bounded_candidate_with_stage_hook(
        writer,
        sources,
        directory,
        EncryptedPreparedSettings { options, limits },
        preflight,
        session,
        |_| Ok(()),
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn write_prepared_encrypted_bounded_candidate_with_stage_hook<W, S, F>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    settings: EncryptedPreparedSettings,
    preflight: BoundedPreflight,
    session: &mut DescriptorEncryptionSession,
    stage_hook: F,
) -> CandidateResult<EndToEndEvidence>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
    F: FnOnce(&mut EncryptedDescriptorStage) -> CandidateResult<()>,
{
    let BoundedPreflight {
        descriptor_stage,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count,
    } = preflight;
    let mut encrypted_stage = transcode_descriptor_stage(directory, descriptor_stage, session)?;
    if encrypted_stage.records() != object_count {
        return Err("encrypted descriptor object count".into());
    }
    stage_hook(&mut encrypted_stage)?;
    encrypted_stage.verify_all(session)?;
    let descriptor_stage_bytes = encrypted_stage.bytes()?;
    let descriptor_ciphertext_sha256 = Some(encrypted_stage.ciphertext_sha256()?);
    let descriptor_reader = encrypted_stage.reader(session)?;
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
    drop(encrypted_stage);
    result
}
