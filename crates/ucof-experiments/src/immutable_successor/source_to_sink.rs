#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableSourceToSinkError {
    Inventory(ImmutableSourceInventoryError),
    Source(ImmutableSourceError),
    Output(ImmutableStreamingWriteError),
    VersionChanged,
}

impl fmt::Display for ImmutableSourceToSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inventory(error) => write!(formatter, "source inventory failed: {error}"),
            Self::Source(error) => write!(formatter, "source payload read failed: {error}"),
            Self::Output(error) => write!(formatter, "source rewrite output failed: {error}"),
            Self::VersionChanged => write!(formatter, "source version changed during rewrite"),
        }
    }
}

impl Error for ImmutableSourceToSinkError {}

impl From<ImmutableSourceInventoryError> for ImmutableSourceToSinkError {
    fn from(error: ImmutableSourceInventoryError) -> Self {
        Self::Inventory(error)
    }
}

impl From<ImmutableSourceError> for ImmutableSourceToSinkError {
    fn from(error: ImmutableSourceError) -> Self {
        Self::Source(error)
    }
}

impl From<ImmutableStreamingWriteError> for ImmutableSourceToSinkError {
    fn from(error: ImmutableStreamingWriteError) -> Self {
        Self::Output(error)
    }
}

impl From<ImmutableError> for ImmutableSourceToSinkError {
    fn from(error: ImmutableError) -> Self {
        Self::Output(ImmutableStreamingWriteError::Format(error))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceToSinkReport {
    pub source: ImmutableReport,
    pub output: ImmutableStreamingWriteReport,
    pub source_version: [u8; 32],
    pub inventory_stats: ImmutableSourceStats,
    pub cumulative_source_stats: ImmutableSourceStats,
    pub payload_version_checks: u64,
    pub largest_payload_read_request: usize,
}

struct SourceToSinkPreflight {
    expected_bytes: usize,
    expected_pages: usize,
    expected_root_level: u8,
    read_chunk: usize,
    payload_bytes: u64,
    payload_read_operations: u64,
}

fn preflight_source_to_sink(
    inventory: &ImmutableSourceActiveInventory,
    source_limits: ImmutableSourceLimits,
    options: ImmutableSourceStreamingWriteOptions,
) -> Result<SourceToSinkPreflight, ImmutableSourceToSinkError> {
    if inventory.objects.is_empty() || inventory.objects.len() > source_limits.format.max_objects {
        return Err(ImmutableError::Limit("object count").into());
    }
    if options.output.max_write_request_bytes == 0 || options.max_source_read_bytes == 0 {
        return Err(ImmutableError::Limit("streaming configuration").into());
    }
    let read_chunk = options
        .max_source_read_bytes
        .min(source_limits.max_read_request_bytes);
    if read_chunk == 0 {
        return Err(ImmutableSourceError::Limit("configuration").into());
    }
    if read_chunk > source_limits.format.max_allocation_bytes {
        return Err(ImmutableError::Limit("allocation").into());
    }
    allocation_check::<Locator>(inventory.objects.len(), source_limits.format)?;

    let mut object_bytes = 0_usize;
    let mut payload_bytes = 0_u64;
    let mut payload_read_operations = 0_u64;
    for object in &inventory.objects {
        if object.object_id == 0 || object.kind == 0 {
            return Err(ImmutableError::Invalid("source inventory object").into());
        }
        let logical_len = usize::try_from(object.logical_len)
            .map_err(|_| ImmutableError::Limit("object size"))?;
        let expected_record_len = OBJECT_HEADER_LEN
            .checked_add(logical_len)
            .ok_or(ImmutableError::Limit("object size"))?;
        if object.record_len != u64_from_usize(expected_record_len)? {
            return Err(ImmutableError::Invalid("source inventory record").into());
        }
        object_bytes = object_bytes
            .checked_add(expected_record_len)
            .ok_or(ImmutableError::Limit("output"))?;
        payload_bytes = payload_bytes
            .checked_add(object.logical_len)
            .ok_or(ImmutableSourceError::Limit("total bytes"))?;
        let reads = if logical_len == 0 {
            0
        } else {
            logical_len
                .checked_add(read_chunk - 1)
                .ok_or(ImmutableSourceError::Limit("read operations"))?
                / read_chunk
        };
        payload_read_operations = payload_read_operations
            .checked_add(
                u64::try_from(reads)
                    .map_err(|_| ImmutableSourceError::Limit("read operations"))?,
            )
            .ok_or(ImmutableSourceError::Limit("read operations"))?;
    }

    let cumulative_bytes = inventory
        .stats
        .bytes_read
        .checked_add(payload_bytes)
        .ok_or(ImmutableSourceError::Limit("total bytes"))?;
    if cumulative_bytes > source_limits.max_total_bytes_read {
        return Err(ImmutableSourceError::Limit("total bytes").into());
    }
    let cumulative_operations = inventory
        .stats
        .read_operations
        .checked_add(payload_read_operations)
        .ok_or(ImmutableSourceError::Limit("read operations"))?;
    if cumulative_operations > source_limits.max_read_operations {
        return Err(ImmutableSourceError::Limit("read operations").into());
    }

    let (expected_pages, expected_root_level) =
        streaming_tree_shape(inventory.objects.len(), source_limits.format)?;
    let page_bytes = expected_pages
        .checked_mul(PAGE_SIZE)
        .ok_or(ImmutableError::Limit("output"))?;
    let expected_bytes = FILE_HEADER_LEN
        .checked_add(object_bytes)
        .and_then(|value| value.checked_add(page_bytes))
        .and_then(|value| value.checked_add(SNAPSHOT_LEN))
        .and_then(|value| value.checked_add(FOOTER_LEN))
        .ok_or(ImmutableError::Limit("output"))?;
    if expected_bytes > source_limits.format.max_output_bytes {
        return Err(ImmutableError::Limit("output").into());
    }
    if expected_bytes > source_limits.format.max_file_bytes {
        return Err(ImmutableError::Limit("file size").into());
    }

    Ok(SourceToSinkPreflight {
        expected_bytes,
        expected_pages,
        expected_root_level,
        read_chunk,
        payload_bytes,
        payload_read_operations,
    })
}

fn check_rewrite_source_version<S: ImmutableVersionedReadAt>(
    source: &mut S,
    expected: [u8; 32],
    checks: &mut u64,
) -> Result<(), ImmutableSourceToSinkError> {
    let actual = source.strong_version()?;
    *checks = checks
        .checked_add(1)
        .ok_or(ImmutableSourceError::Limit("version checks"))?;
    if actual != expected {
        return Err(ImmutableSourceToSinkError::VersionChanged);
    }
    Ok(())
}

fn write_inventory_object<W: Write, S: ImmutableVersionedReadAt>(
    sink: &mut StreamingSink<'_, W>,
    source: &mut S,
    object: &ImmutableSourceActiveObject,
    expected_version: [u8; 32],
    buffer: &mut [u8],
    stats: &mut ImmutableSourceStats,
    version_checks: &mut u64,
    largest_request: &mut usize,
) -> Result<Locator, ImmutableSourceToSinkError> {
    check_rewrite_source_version(source, expected_version, version_checks)?;
    let logical_len = usize::try_from(object.logical_len)
        .map_err(|_| ImmutableError::Limit("object size"))?;
    let payload_offset = object.payload_offset()?;

    let mut header = [0_u8; OBJECT_HEADER_LEN];
    header[..8].copy_from_slice(OBJECT_MAGIC);
    put_u16(
        &mut header,
        8,
        u16::try_from(OBJECT_HEADER_LEN).map_err(|_| ImmutableError::Limit("object header"))?,
    );
    put_u16(&mut header, 10, object.kind);
    put_u64(&mut header, 16, object.object_id);
    put_u64(&mut header, 24, object.logical_len);
    put_u64(&mut header, 32, object.logical_len);

    let record_offset = u64_from_usize(sink.offset)?;
    let record_len = OBJECT_HEADER_LEN
        .checked_add(logical_len)
        .ok_or(ImmutableError::Limit("object size"))?;
    let mut object_hasher = Sha256::new();
    object_hasher.update(OBJECT_DOMAIN);
    object_hasher.update(header);
    sink.write_commit_bytes(&header)?;

    let mut completed = 0_usize;
    while completed < logical_len {
        let take = (logical_len - completed).min(buffer.len());
        if take == 0 {
            return Err(ImmutableError::Limit("source read").into());
        }
        let offset = payload_offset
            .checked_add(u64_from_usize(completed)?)
            .ok_or(ImmutableSourceError::Limit("payload offset"))?;
        source.read_exact_at(offset, &mut buffer[..take])?;
        stats.read_operations = stats
            .read_operations
            .checked_add(1)
            .ok_or(ImmutableSourceError::Limit("read operations"))?;
        stats.bytes_read = stats
            .bytes_read
            .checked_add(u64_from_usize(take)?)
            .ok_or(ImmutableSourceError::Limit("total bytes"))?;
        stats.bytes_hashed = stats
            .bytes_hashed
            .checked_add(u64_from_usize(take)?)
            .ok_or(ImmutableSourceError::Limit("hash bytes"))?;
        *largest_request = (*largest_request).max(take);
        object_hasher.update(&buffer[..take]);
        sink.write_commit_bytes(&buffer[..take])?;
        completed += take;
    }
    check_rewrite_source_version(source, expected_version, version_checks)?;

    let digest: [u8; 32] = object_hasher.finalize().into();
    if digest != object.object_digest {
        return Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "source object digest",
        ))
        .into());
    }
    Ok(Locator {
        object_id: object.object_id,
        kind: object.kind,
        record_offset,
        record_len: u64_from_usize(record_len)?,
        logical_len: object.logical_len,
        digest,
    })
}

/// Strictly inventories one stable bounded source and streams its active state into canonical
/// genesis output without materializing either the input file or complete output file.
///
/// Inventory and payload emission share the caller's cumulative source budgets. All deterministic
/// payload bytes and read operations are preflighted before the first output byte. The source version
/// is checked before output, before and after every active payload, and after the final payload. A
/// source or sink failure after output begins remains terminal; atomic visibility requires private
/// staging and a separate publication protocol.
pub fn rewrite_versioned_source_to<W: Write, S: ImmutableVersionedReadAt>(
    writer: &mut W,
    source: &mut S,
    source_limits: ImmutableSourceLimits,
    options: ImmutableSourceStreamingWriteOptions,
) -> Result<ImmutableSourceToSinkReport, ImmutableSourceToSinkError> {
    let inventory = inventory_source_at(source, source_limits)?;
    let preflight = preflight_source_to_sink(&inventory, source_limits, options)?;
    let source_report = inventory.report.clone();
    let source_version = inventory.version;
    let inventory_stats = inventory.stats;
    let objects = inventory.objects;

    let mut payload_version_checks = 0_u64;
    check_rewrite_source_version(source, source_version, &mut payload_version_checks)?;
    let mut sink = StreamingSink::new(writer, options.output.max_write_request_bytes)?;
    let mut header = [0_u8; FILE_HEADER_LEN];
    header[..8].copy_from_slice(FILE_MAGIC);
    sink.write_commit_bytes(&header)?;

    let mut buffer = vec![0_u8; preflight.read_chunk];
    let mut locators = Vec::with_capacity(objects.len());
    let mut cumulative_source_stats = inventory_stats;
    cumulative_source_stats.largest_allocation = cumulative_source_stats
        .largest_allocation
        .max(buffer.len());
    let mut largest_payload_read_request = 0_usize;
    for object in &objects {
        locators.push(write_inventory_object(
            &mut sink,
            source,
            object,
            source_version,
            &mut buffer,
            &mut cumulative_source_stats,
            &mut payload_version_checks,
            &mut largest_payload_read_request,
        )?);
    }
    check_rewrite_source_version(source, source_version, &mut payload_version_checks)?;

    if cumulative_source_stats.bytes_read
        != inventory_stats
            .bytes_read
            .checked_add(preflight.payload_bytes)
            .ok_or(ImmutableSourceError::Limit("total bytes"))?
        || cumulative_source_stats.read_operations
            != inventory_stats
                .read_operations
                .checked_add(preflight.payload_read_operations)
                .ok_or(ImmutableSourceError::Limit("read operations"))?
    {
        return Err(ImmutableError::Invalid("source budget accounting").into());
    }

    let (root, page_count) = write_streaming_tree(&mut sink, &locators, source_limits.format)?;
    if page_count != preflight.expected_pages || root.level != preflight.expected_root_level {
        return Err(ImmutableError::Invalid("streaming tree shape").into());
    }
    let mut report = write_streaming_publication(&mut sink, &root, page_count)?;
    report.object_count = locators.len();
    if sink.offset != preflight.expected_bytes {
        return Err(ImmutableError::Invalid("streaming output length").into());
    }

    Ok(ImmutableSourceToSinkReport {
        source: source_report,
        output: ImmutableStreamingWriteReport {
            report,
            bytes_written: sink.offset,
            largest_write_request: sink.largest_write_request,
            locator_entries: locators.len(),
        },
        source_version,
        inventory_stats,
        cumulative_source_stats,
        payload_version_checks,
        largest_payload_read_request,
    })
}

#[cfg(test)]
mod source_to_sink_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct VersionedMemorySource {
        data: Vec<u8>,
        version: [u8; 32],
        reads: u64,
        mutate_after_read: Option<u64>,
        largest_request: usize,
    }

    impl VersionedMemorySource {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                version: [17; 32],
                reads: 0,
                mutate_after_read: None,
                largest_request: 0,
            }
        }
    }

    impl ImmutableReadAt for VersionedMemorySource {
        fn len(&mut self) -> Result<u64, ImmutableSourceError> {
            u64::try_from(self.data.len()).map_err(|_| ImmutableSourceError::Limit("length"))
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            buffer: &mut [u8],
        ) -> Result<(), ImmutableSourceError> {
            let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
            let end = start
                .checked_add(buffer.len())
                .ok_or(ImmutableSourceError::Io("range"))?;
            buffer.copy_from_slice(
                self.data
                    .get(start..end)
                    .ok_or(ImmutableSourceError::Io("range"))?,
            );
            self.reads += 1;
            self.largest_request = self.largest_request.max(buffer.len());
            if self.mutate_after_read == Some(self.reads) {
                self.version[0] ^= 1;
            }
            Ok(())
        }
    }

    impl ImmutableVersionedReadAt for VersionedMemorySource {
        fn strong_version(&mut self) -> Result<[u8; 32], ImmutableSourceError> {
            Ok(self.version)
        }
    }

    fn object(object_id: u64, payload_len: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 23).expect("kind"),
            vec![u8::try_from(object_id % 251).expect("seed"); payload_len],
        )
    }

    #[test]
    fn versioned_source_rewrite_matches_owned_active_rewrite() {
        let format = ImmutableLimits::default();
        let inputs: Vec<_> = (1..=400_u64).map(|id| object(id, 257)).collect();
        let genesis = build_genesis(&inputs, format).expect("genesis");
        let source_bytes = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(200, 88, b"active replacement".to_vec()),
            format,
        )
        .expect("replacement");
        let expected = rewrite_all(&source_bytes, format).expect("owned active rewrite");
        let mut source = VersionedMemorySource::new(source_bytes);
        let mut actual = Vec::new();
        let report = rewrite_versioned_source_to(
            &mut actual,
            &mut source,
            ImmutableSourceLimits {
                format,
                max_read_request_bytes: 97,
                hash_block_bytes: 89,
                ..ImmutableSourceLimits::default()
            },
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 113,
                },
                max_source_read_bytes: 31,
            },
        )
        .expect("versioned source rewrite");
        assert_eq!(actual, expected.bytes);
        assert_eq!(report.source, expected.source);
        assert_eq!(report.output.report, expected.output);
        assert_eq!(report.source_version, [17; 32]);
        assert_eq!(report.largest_payload_read_request, 31);
        assert!(report.output.largest_write_request <= 113);
        assert!(source.largest_request <= 97);
        assert_eq!(report.payload_version_checks, 2 * 400 + 2);
    }

    #[test]
    fn inactive_history_is_not_reread_by_output_pass() {
        let format = ImmutableLimits::default();
        let genesis = build_genesis(&[object(1, 4_096), object(2, 17)], format).expect("genesis");
        let source_bytes = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(1, 7, b"small-active".to_vec()),
            format,
        )
        .expect("replacement");
        let mut probe = VersionedMemorySource::new(source_bytes.clone());
        let inventory = inventory_source_at(&mut probe, ImmutableSourceLimits::default())
            .expect("inventory probe");
        let mut source = VersionedMemorySource::new(source_bytes);
        let mut sink = Vec::new();
        let report = rewrite_versioned_source_to(
            &mut sink,
            &mut source,
            ImmutableSourceLimits::default(),
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions::default(),
                max_source_read_bytes: 64,
            },
        )
        .expect("source rewrite");
        assert_eq!(
            report.cumulative_source_stats.bytes_read - report.inventory_stats.bytes_read,
            12 + 17
        );
        assert_eq!(report.inventory_stats, inventory.stats);
    }

    #[test]
    fn cumulative_budget_failure_leaves_sink_untouched() {
        let format = ImmutableLimits::default();
        let source_bytes = build_genesis(&[object(1, 257)], format).expect("genesis");
        let mut probe = VersionedMemorySource::new(source_bytes.clone());
        let inventory = inventory_source_at(&mut probe, ImmutableSourceLimits::default())
            .expect("inventory probe");
        let mut source = VersionedMemorySource::new(source_bytes);
        let mut sink = Vec::new();
        assert!(rewrite_versioned_source_to(
            &mut sink,
            &mut source,
            ImmutableSourceLimits {
                max_total_bytes_read: inventory.stats.bytes_read + 256,
                ..ImmutableSourceLimits::default()
            },
            ImmutableSourceStreamingWriteOptions::default(),
        )
        .is_err());
        assert!(sink.is_empty());
    }

    #[test]
    fn version_change_during_payload_streaming_is_terminal() {
        let format = ImmutableLimits::default();
        let source_bytes = build_genesis(&[object(1, 257)], format).expect("genesis");
        let mut probe = VersionedMemorySource::new(source_bytes.clone());
        let inventory = inventory_source_at(&mut probe, ImmutableSourceLimits::default())
            .expect("inventory probe");
        let mut source = VersionedMemorySource::new(source_bytes);
        source.mutate_after_read = Some(inventory.stats.read_operations + 1);
        let mut sink = Vec::new();
        assert_eq!(
            rewrite_versioned_source_to(
                &mut sink,
                &mut source,
                ImmutableSourceLimits::default(),
                ImmutableSourceStreamingWriteOptions {
                    output: ImmutableStreamingWriteOptions {
                        max_write_request_bytes: 32,
                    },
                    max_source_read_bytes: 16,
                },
            ),
            Err(ImmutableSourceToSinkError::VersionChanged)
        );
        assert!(!sink.is_empty());
    }
}
