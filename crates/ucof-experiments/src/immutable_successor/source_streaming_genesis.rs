/// Random-access payload contract for source-backed canonical streaming output.
///
/// `strong_version` must identify one immutable payload view without ABA reuse. Metadata returned by
/// the other methods must be bound to that version by the implementation.
pub trait ImmutableStreamingPayloadSource {
    fn object_id(&self) -> u64;
    fn kind(&self) -> u16;
    fn logical_len(&self) -> u64;
    fn strong_version(&mut self) -> Result<[u8; 32], &'static str>;
    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), &'static str>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableSourceStreamingWriteOptions {
    pub output: ImmutableStreamingWriteOptions,
    pub max_source_read_bytes: usize,
}

impl Default for ImmutableSourceStreamingWriteOptions {
    fn default() -> Self {
        Self {
            output: ImmutableStreamingWriteOptions::default(),
            max_source_read_bytes: 64 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSourceStreamingWriteReport {
    pub output: ImmutableStreamingWriteReport,
    pub source_read_operations: u64,
    pub source_bytes_read: u64,
    pub version_checks: u64,
    pub largest_source_buffer: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmutableSourceStreamingWriteError {
    Format(ImmutableError),
    Source { object_id: u64, label: &'static str },
    VersionChanged(u64),
    OutputIo(&'static str),
}

impl fmt::Display for ImmutableSourceStreamingWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "{error}"),
            Self::Source { object_id, label } => {
                write!(formatter, "payload source {object_id} failed: {label}")
            }
            Self::VersionChanged(object_id) => {
                write!(formatter, "payload source {object_id} changed version")
            }
            Self::OutputIo(label) => write!(formatter, "streaming output failed: {label}"),
        }
    }
}

impl Error for ImmutableSourceStreamingWriteError {}

impl From<ImmutableError> for ImmutableSourceStreamingWriteError {
    fn from(error: ImmutableError) -> Self {
        Self::Format(error)
    }
}

impl From<ImmutableStreamingWriteError> for ImmutableSourceStreamingWriteError {
    fn from(error: ImmutableStreamingWriteError) -> Self {
        match error {
            ImmutableStreamingWriteError::Format(error) => Self::Format(error),
            ImmutableStreamingWriteError::Io(label) => Self::OutputIo(label),
        }
    }
}

struct SourceStreamingPreflight {
    order: Vec<usize>,
    versions: Vec<[u8; 32]>,
    lengths: Vec<usize>,
    expected_bytes: usize,
    expected_pages: usize,
    expected_root_level: u8,
    largest_source_buffer: usize,
    version_checks: u64,
}

fn preflight_source_streaming<S: ImmutableStreamingPayloadSource>(
    sources: &mut [S],
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
) -> Result<SourceStreamingPreflight, ImmutableSourceStreamingWriteError> {
    if sources.is_empty() || sources.len() > limits.max_objects {
        return Err(ImmutableError::Limit("object count").into());
    }
    if options.output.max_write_request_bytes == 0 || options.max_source_read_bytes == 0 {
        return Err(ImmutableError::Limit("streaming configuration").into());
    }
    allocation_check::<usize>(sources.len(), limits)?;
    allocation_check::<Locator>(sources.len(), limits)?;
    allocation_check::<[u8; 32]>(sources.len(), limits)?;

    let mut order: Vec<usize> = (0..sources.len()).collect();
    order.sort_unstable_by_key(|index| sources[*index].object_id());
    if let Some(pair) = order.windows(2).find(|pair| {
        sources[pair[0]].object_id() == sources[pair[1]].object_id()
    }) {
        return Err(ImmutableError::DuplicateObject(sources[pair[0]].object_id()).into());
    }

    let mut versions = vec![[0_u8; 32]; sources.len()];
    let mut lengths = vec![0_usize; sources.len()];
    let mut object_bytes = 0_usize;
    let mut largest_source_buffer = 0_usize;
    let mut version_checks = 0_u64;
    for index in &order {
        let object_id = sources[*index].object_id();
        if object_id == 0 || sources[*index].kind() == 0 {
            return Err(ImmutableError::Invalid("object input").into());
        }
        let length = usize::try_from(sources[*index].logical_len())
            .map_err(|_| ImmutableError::Limit("object size"))?;
        lengths[*index] = length;
        let record_len = OBJECT_HEADER_LEN
            .checked_add(length)
            .ok_or(ImmutableError::Limit("object size"))?;
        object_bytes = object_bytes
            .checked_add(record_len)
            .ok_or(ImmutableError::Limit("output"))?;
        largest_source_buffer = largest_source_buffer.max(length.min(options.max_source_read_bytes));
        if largest_source_buffer > limits.max_allocation_bytes {
            return Err(ImmutableError::Limit("allocation").into());
        }
        versions[*index] = sources[*index]
            .strong_version()
            .map_err(|label| ImmutableSourceStreamingWriteError::Source {
                object_id,
                label,
            })?;
        version_checks = version_checks
            .checked_add(1)
            .ok_or(ImmutableError::Limit("version checks"))?;
    }

    let (expected_pages, expected_root_level) = streaming_tree_shape(sources.len(), limits)?;
    let page_bytes = expected_pages
        .checked_mul(PAGE_SIZE)
        .ok_or(ImmutableError::Limit("output"))?;
    let expected_bytes = FILE_HEADER_LEN
        .checked_add(object_bytes)
        .and_then(|value| value.checked_add(page_bytes))
        .and_then(|value| value.checked_add(SNAPSHOT_LEN))
        .and_then(|value| value.checked_add(FOOTER_LEN))
        .ok_or(ImmutableError::Limit("output"))?;
    if expected_bytes > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output").into());
    }
    if expected_bytes > limits.max_file_bytes {
        return Err(ImmutableError::Limit("file size").into());
    }

    Ok(SourceStreamingPreflight {
        order,
        versions,
        lengths,
        expected_bytes,
        expected_pages,
        expected_root_level,
        largest_source_buffer,
        version_checks,
    })
}

fn checked_source_version<S: ImmutableStreamingPayloadSource>(
    source: &mut S,
    expected: [u8; 32],
    version_checks: &mut u64,
) -> Result<(), ImmutableSourceStreamingWriteError> {
    let object_id = source.object_id();
    let actual = source
        .strong_version()
        .map_err(|label| ImmutableSourceStreamingWriteError::Source {
            object_id,
            label,
        })?;
    *version_checks = version_checks
        .checked_add(1)
        .ok_or(ImmutableError::Limit("version checks"))?;
    if actual != expected {
        return Err(ImmutableSourceStreamingWriteError::VersionChanged(
            object_id,
        ));
    }
    Ok(())
}

fn write_source_streaming_object<W: Write, S: ImmutableStreamingPayloadSource>(
    sink: &mut StreamingSink<'_, W>,
    source: &mut S,
    expected_version: [u8; 32],
    logical_len: usize,
    buffer: &mut [u8],
    source_read_operations: &mut u64,
    source_bytes_read: &mut u64,
    version_checks: &mut u64,
) -> Result<Locator, ImmutableSourceStreamingWriteError> {
    checked_source_version(source, expected_version, version_checks)?;
    let object_id = source.object_id();
    let kind = source.kind();
    let mut header = [0_u8; OBJECT_HEADER_LEN];
    header[..8].copy_from_slice(OBJECT_MAGIC);
    put_u16(
        &mut header,
        8,
        u16::try_from(OBJECT_HEADER_LEN).map_err(|_| ImmutableError::Limit("object header"))?,
    );
    put_u16(&mut header, 10, kind);
    put_u64(&mut header, 16, object_id);
    put_u64(&mut header, 24, u64_from_usize(logical_len)?);
    put_u64(&mut header, 32, u64_from_usize(logical_len)?);

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
        let completed_u64 = u64_from_usize(completed)?;
        source
            .read_exact_at(completed_u64, &mut buffer[..take])
            .map_err(|label| ImmutableSourceStreamingWriteError::Source {
                object_id,
                label,
            })?;
        *source_read_operations = source_read_operations
            .checked_add(1)
            .ok_or(ImmutableError::Limit("read operations"))?;
        *source_bytes_read = source_bytes_read
            .checked_add(u64_from_usize(take)?)
            .ok_or(ImmutableError::Limit("read bytes"))?;
        object_hasher.update(&buffer[..take]);
        sink.write_commit_bytes(&buffer[..take])?;
        completed += take;
    }
    checked_source_version(source, expected_version, version_checks)?;

    Ok(Locator {
        object_id,
        kind,
        record_offset,
        record_len: u64_from_usize(record_len)?,
        logical_len: u64_from_usize(logical_len)?,
        digest: object_hasher.finalize().into(),
    })
}

/// Streams canonical genesis output from independently versioned random-access payload sources.
///
/// All metadata and initial versions are preflighted before the first sink write. Each payload's
/// strong version is checked immediately before and after its bounded read sequence. A source or
/// version failure after output begins is terminal and returns no publication report; callers must
/// use private staging when partial sink bytes must remain invisible.
pub fn write_genesis_sources_to<W: Write, S: ImmutableStreamingPayloadSource>(
    writer: &mut W,
    sources: &mut [S],
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
) -> Result<ImmutableSourceStreamingWriteReport, ImmutableSourceStreamingWriteError> {
    let preflight = preflight_source_streaming(sources, options, limits)?;
    let mut sink = StreamingSink::new(writer, options.output.max_write_request_bytes)?;
    let mut header = [0_u8; FILE_HEADER_LEN];
    header[..8].copy_from_slice(FILE_MAGIC);
    sink.write_commit_bytes(&header)?;

    let mut buffer = vec![0_u8; preflight.largest_source_buffer];
    let mut locators = Vec::with_capacity(preflight.order.len());
    let mut source_read_operations = 0_u64;
    let mut source_bytes_read = 0_u64;
    let mut version_checks = preflight.version_checks;
    for index in preflight.order {
        locators.push(write_source_streaming_object(
            &mut sink,
            &mut sources[index],
            preflight.versions[index],
            preflight.lengths[index],
            &mut buffer,
            &mut source_read_operations,
            &mut source_bytes_read,
            &mut version_checks,
        )?);
    }

    let (root, page_count) = write_streaming_tree(&mut sink, &locators, limits)?;
    if page_count != preflight.expected_pages || root.level != preflight.expected_root_level {
        return Err(ImmutableError::Invalid("streaming tree shape").into());
    }
    let mut report = write_streaming_publication(&mut sink, &root, page_count)?;
    report.object_count = locators.len();
    if sink.offset != preflight.expected_bytes {
        return Err(ImmutableError::Invalid("streaming output length").into());
    }

    Ok(ImmutableSourceStreamingWriteReport {
        output: ImmutableStreamingWriteReport {
            report,
            bytes_written: sink.offset,
            largest_write_request: sink.largest_write_request,
            locator_entries: locators.len(),
        },
        source_read_operations,
        source_bytes_read,
        version_checks,
        largest_source_buffer: buffer.len(),
    })
}

#[cfg(test)]
mod source_streaming_genesis_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct MemoryPayloadSource {
        object_id: u64,
        kind: u16,
        bytes: Vec<u8>,
        version: [u8; 32],
        mutate_after_read: bool,
        fail_reads: bool,
        largest_request: usize,
    }

    impl MemoryPayloadSource {
        fn new(object_id: u64, bytes: Vec<u8>) -> Self {
            Self {
                object_id,
                kind: u16::try_from(1 + object_id % 17).expect("kind"),
                bytes,
                version: [u8::try_from(object_id % 251).expect("version seed"); 32],
                mutate_after_read: false,
                fail_reads: false,
                largest_request: 0,
            }
        }
    }

    impl ImmutableStreamingPayloadSource for MemoryPayloadSource {
        fn object_id(&self) -> u64 {
            self.object_id
        }

        fn kind(&self) -> u16 {
            self.kind
        }

        fn logical_len(&self) -> u64 {
            u64::try_from(self.bytes.len()).expect("test payload length")
        }

        fn strong_version(&mut self) -> Result<[u8; 32], &'static str> {
            Ok(self.version)
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            buffer: &mut [u8],
        ) -> Result<(), &'static str> {
            if self.fail_reads {
                return Err("injected read failure");
            }
            let start = usize::try_from(offset).map_err(|_| "offset")?;
            let end = start.checked_add(buffer.len()).ok_or("range")?;
            buffer.copy_from_slice(self.bytes.get(start..end).ok_or("range")?);
            self.largest_request = self.largest_request.max(buffer.len());
            if self.mutate_after_read {
                self.version[0] ^= 1;
                self.mutate_after_read = false;
            }
            Ok(())
        }
    }

    fn owned_inputs(sources: &[MemoryPayloadSource]) -> Vec<ImmutableObjectInput> {
        sources
            .iter()
            .map(|source| {
                ImmutableObjectInput::new(source.object_id, source.kind, source.bytes.clone())
            })
            .collect()
    }

    #[test]
    fn source_backed_output_matches_owned_canonical_output() {
        let limits = ImmutableLimits::default();
        let mut sources: Vec<_> = (1..=400_u64)
            .rev()
            .map(|object_id| {
                MemoryPayloadSource::new(
                    object_id,
                    vec![u8::try_from(object_id % 251).expect("seed"); 257],
                )
            })
            .collect();
        let expected = build_genesis(&owned_inputs(&sources), limits).expect("owned genesis");
        let mut actual = Vec::new();
        let report = write_genesis_sources_to(
            &mut actual,
            &mut sources,
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 113,
                },
                max_source_read_bytes: 31,
            },
            limits,
        )
        .expect("source-backed genesis");
        assert_eq!(actual, expected);
        assert_eq!(
            report.output.report,
            validate_canonical_occupancy(&actual, limits).expect("canonical output")
        );
        assert_eq!(report.source_bytes_read, 400 * 257);
        assert_eq!(report.version_checks, 1_200);
        assert_eq!(report.largest_source_buffer, 31);
        assert!(sources.iter().all(|source| source.largest_request <= 31));
        assert!(report.output.largest_write_request <= 113);
    }

    #[test]
    fn version_change_is_terminal_and_returns_no_report() {
        let mut sources = vec![MemoryPayloadSource::new(1, vec![7; 10])];
        sources[0].mutate_after_read = true;
        let mut sink = Vec::new();
        assert_eq!(
            write_genesis_sources_to(
                &mut sink,
                &mut sources,
                ImmutableSourceStreamingWriteOptions {
                    output: ImmutableStreamingWriteOptions {
                        max_write_request_bytes: 32,
                    },
                    max_source_read_bytes: 16,
                },
                ImmutableLimits::default(),
            ),
            Err(ImmutableSourceStreamingWriteError::VersionChanged(1))
        );
        assert!(!sink.is_empty());
        assert!(sink.len() < FOOTER_LEN);
    }

    #[test]
    fn source_failure_is_terminal_and_returns_no_report() {
        let mut sources = vec![MemoryPayloadSource::new(1, vec![7; 100])];
        sources[0].fail_reads = true;
        let mut sink = Vec::new();
        assert_eq!(
            write_genesis_sources_to(
                &mut sink,
                &mut sources,
                ImmutableSourceStreamingWriteOptions::default(),
                ImmutableLimits::default(),
            ),
            Err(ImmutableSourceStreamingWriteError::Source {
                object_id: 1,
                label: "injected read failure"
            })
        );
        assert!(!sink.is_empty());
    }

    #[test]
    fn metadata_failure_leaves_sink_untouched() {
        let mut sources = vec![
            MemoryPayloadSource::new(2, Vec::new()),
            MemoryPayloadSource::new(2, Vec::new()),
        ];
        let mut sink = Vec::new();
        assert_eq!(
            write_genesis_sources_to(
                &mut sink,
                &mut sources,
                ImmutableSourceStreamingWriteOptions::default(),
                ImmutableLimits::default(),
            ),
            Err(ImmutableSourceStreamingWriteError::Format(
                ImmutableError::DuplicateObject(2)
            ))
        );
        assert!(sink.is_empty());
    }
}
