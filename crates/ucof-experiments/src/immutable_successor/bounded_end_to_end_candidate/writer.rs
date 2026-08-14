#[derive(Clone, Debug)]
struct SourceDescriptor {
    object_id: u64,
    source_index: u64,
    kind: u16,
    logical_len: u64,
    strong_version: [u8; 32],
}

impl SourceDescriptor {
    fn encode(&self) -> [u8; DESCRIPTOR_STAGE_BYTES] {
        let mut bytes = [0u8; DESCRIPTOR_STAGE_BYTES];
        bytes[..8].copy_from_slice(&self.object_id.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.source_index.to_le_bytes());
        bytes[16..18].copy_from_slice(&self.kind.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.logical_len.to_le_bytes());
        bytes[32..64].copy_from_slice(&self.strong_version);
        bytes
    }

    fn decode(bytes: &[u8; DESCRIPTOR_STAGE_BYTES]) -> CandidateResult<Self> {
        if bytes[18..24].iter().any(|byte| *byte != 0) {
            return Err("descriptor reserved bytes".into());
        }
        let object_id = u64::from_le_bytes(bytes[..8].try_into().expect("descriptor field"));
        let source_index =
            u64::from_le_bytes(bytes[8..16].try_into().expect("descriptor field"));
        let kind = u16::from_le_bytes(bytes[16..18].try_into().expect("descriptor field"));
        if object_id == 0 || kind == 0 {
            return Err("descriptor identity".into());
        }
        Ok(Self {
            object_id,
            source_index,
            kind,
            logical_len: u64::from_le_bytes(
                bytes[24..32].try_into().expect("descriptor logical length"),
            ),
            strong_version: bytes[32..64].try_into().expect("descriptor version"),
        })
    }
}

struct DescriptorRecords<'a, S> {
    sources: std::iter::Enumerate<std::slice::IterMut<'a, S>>,
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    input_error: &'a mut Option<String>,
    object_bytes: &'a mut usize,
    largest_source_buffer: &'a mut usize,
    version_checks: &'a mut u64,
    failed: bool,
}

impl<S: ImmutableStreamingPayloadSource> Iterator for DescriptorRecords<'_, S> {
    type Item = BoundedSpillRecord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        let (index, source) = self.sources.next()?;
        let result = (|| -> CandidateResult<BoundedSpillRecord> {
            let object_id = source.object_id();
            let kind = source.kind();
            if object_id == 0 || kind == 0 {
                return Err("invalid object input".into());
            }
            let logical_len = source.logical_len();
            let length = usize::try_from(logical_len).map_err(|_| "object size".to_owned())?;
            let record_len = OBJECT_HEADER_LEN
                .checked_add(length)
                .ok_or_else(|| "object size".to_owned())?;
            *self.object_bytes = self
                .object_bytes
                .checked_add(record_len)
                .ok_or_else(|| "output size".to_owned())?;
            *self.largest_source_buffer = (*self.largest_source_buffer)
                .max(length.min(self.options.max_source_read_bytes));
            if *self.largest_source_buffer > self.limits.max_allocation_bytes {
                return Err("source buffer allocation limit".into());
            }
            let strong_version = source
                .strong_version()
                .map_err(|label| format!("source {object_id} version: {label}"))?;
            *self.version_checks = self
                .version_checks
                .checked_add(1)
                .ok_or_else(|| "version check overflow".to_owned())?;
            let descriptor = SourceDescriptor {
                object_id,
                source_index: u64::try_from(index).map_err(|_| "source index".to_owned())?,
                kind,
                logical_len,
                strong_version,
            };
            Ok(BoundedSpillRecord::new(
                object_id,
                descriptor.encode().to_vec(),
            ))
        })();
        match result {
            Ok(record) => Some(record),
            Err(error) => {
                *self.input_error = Some(error);
                self.failed = true;
                Some(BoundedSpillRecord::new(0, Vec::new()))
            }
        }
    }
}

#[derive(Debug)]
struct BoundedPreflight {
    descriptor_stage: FixedStage,
    descriptor_spill: BoundedSpillSortReport,
    expected_bytes: usize,
    expected_pages: usize,
    expected_root_level: u8,
    largest_source_buffer: usize,
    version_checks: u64,
    object_count: usize,
}

fn prepare_bounded_preflight<S: ImmutableStreamingPayloadSource>(
    directory: &Path,
    sources: &mut [S],
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    spill_limits: BoundedSpillSortLimits,
) -> CandidateResult<BoundedPreflight> {
    if sources.is_empty() || sources.len() > limits.max_objects {
        return Err("object count limit".into());
    }
    if options.output.max_write_request_bytes == 0 || options.max_source_read_bytes == 0 {
        return Err("streaming configuration".into());
    }
    if spill_limits.record_bytes != DESCRIPTOR_STAGE_BYTES {
        return Err("descriptor spill record size".into());
    }

    allocation_check::<Locator>(LEAF_CAPACITY, limits).map_err(|error| error.to_string())?;
    allocation_check::<PageRef>(INTERNAL_FANOUT, limits).map_err(|error| error.to_string())?;

    let mut descriptor_stage =
        FixedStage::create(directory, "source-descriptors", DESCRIPTOR_STAGE_BYTES)?;
    let mut writer = descriptor_stage.writer()?;
    let mut input_error = None;
    let mut object_bytes = 0usize;
    let mut largest_source_buffer = 0usize;
    let mut version_checks = 0u64;
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
    let sorted = bounded_spill_sort_to(directory, records, &mut writer, spill_limits);
    if let Some(error) = input_error {
        return Err(error);
    }
    let descriptor_spill = sorted.map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())?;
    drop(writer);
    descriptor_stage.set_records_u64(descriptor_spill.output_records)?;
    let stage_bytes = descriptor_stage.validate_bytes()?;
    let expected_stage_bytes = descriptor_spill
        .output_records
        .checked_mul(u64::try_from(DESCRIPTOR_STAGE_BYTES).expect("descriptor width fits u64"))
        .ok_or_else(|| "descriptor stage byte overflow".to_owned())?;
    if descriptor_spill.output_records
        != u64::try_from(sources.len()).map_err(|_| "object count".to_owned())?
        || descriptor_spill.output_payload_bytes != expected_stage_bytes
        || stage_bytes != expected_stage_bytes
    {
        return Err("descriptor stage size".into());
    }

    let (expected_pages, expected_root_level) =
        streaming_tree_shape(sources.len(), limits).map_err(|error| error.to_string())?;
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

    Ok(BoundedPreflight {
        descriptor_stage,
        descriptor_spill,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
        object_count: sources.len(),
    })
}

#[derive(Debug)]
struct EndToEndEvidence {
    output: ImmutableSourceStreamingWriteReport,
    descriptor_stage_bytes: u64,
    descriptor_spill: BoundedSpillSortReport,
    peak_locator_entries: usize,
    peak_page_ref_entries: usize,
    peak_live_retained_stage_bytes: u64,
}

fn write_genesis_sources_end_to_end_bounded_candidate<W, S>(
    writer: &mut W,
    sources: &mut [S],
    directory: &Path,
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
    spill_limits: BoundedSpillSortLimits,
) -> CandidateResult<EndToEndEvidence>
where
    W: Write,
    S: ImmutableStreamingPayloadSource,
{
    let preflight = prepare_bounded_preflight(directory, sources, options, limits, spill_limits)?;
    let descriptor_stage_bytes = preflight.descriptor_stage.validate_bytes()?;
    let descriptor_spill = preflight.descriptor_spill.clone();
    let expected_bytes = preflight.expected_bytes;
    let expected_pages = preflight.expected_pages;
    let expected_root_level = preflight.expected_root_level;
    let object_count = preflight.object_count;
    let mut descriptor_stage = preflight.descriptor_stage;

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
