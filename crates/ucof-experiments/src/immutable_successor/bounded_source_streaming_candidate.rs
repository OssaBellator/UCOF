#[cfg(test)]
mod bounded_source_streaming_candidate_tests {
    use super::*;
    use crate::bounded_spill_sort::{
        bounded_spill_sort_to, BoundedSpillRecord, BoundedSpillSortError, BoundedSpillSortLimits,
        BoundedSpillSortReport,
    };
    use std::fs::{self, File, OpenOptions};
    use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const DESCRIPTOR_BYTES: usize = 64;
    static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, Debug)]
    struct SourceDescriptor {
        object_id: u64,
        source_index: u64,
        kind: u16,
        logical_len: u64,
        strong_version: [u8; 32],
    }

    impl SourceDescriptor {
        fn encode(&self) -> [u8; DESCRIPTOR_BYTES] {
            let mut bytes = [0u8; DESCRIPTOR_BYTES];
            bytes[..8].copy_from_slice(&self.object_id.to_le_bytes());
            bytes[8..16].copy_from_slice(&self.source_index.to_le_bytes());
            bytes[16..18].copy_from_slice(&self.kind.to_le_bytes());
            bytes[24..32].copy_from_slice(&self.logical_len.to_le_bytes());
            bytes[32..64].copy_from_slice(&self.strong_version);
            bytes
        }

        fn decode(bytes: &[u8; DESCRIPTOR_BYTES]) -> Result<Self, CandidateError> {
            if bytes[18..24].iter().any(|byte| *byte != 0) {
                return Err(CandidateError::Stage("descriptor reserved bytes"));
            }
            let object_id = u64::from_le_bytes(bytes[..8].try_into().expect("descriptor field"));
            let source_index =
                u64::from_le_bytes(bytes[8..16].try_into().expect("descriptor field"));
            let kind = u16::from_le_bytes(bytes[16..18].try_into().expect("descriptor field"));
            let logical_len =
                u64::from_le_bytes(bytes[24..32].try_into().expect("descriptor field"));
            let strong_version = bytes[32..64].try_into().expect("descriptor version");
            if object_id == 0 || kind == 0 {
                return Err(CandidateError::Stage("descriptor identity"));
            }
            Ok(Self {
                object_id,
                source_index,
                kind,
                logical_len,
                strong_version,
            })
        }
    }

    #[derive(Debug)]
    enum CandidateError {
        Writer(ImmutableSourceStreamingWriteError),
        Spill(BoundedSpillSortError),
        StageIo(std::io::ErrorKind),
        Stage(&'static str),
    }

    impl From<ImmutableSourceStreamingWriteError> for CandidateError {
        fn from(error: ImmutableSourceStreamingWriteError) -> Self {
            Self::Writer(error)
        }
    }

    impl From<ImmutableError> for CandidateError {
        fn from(error: ImmutableError) -> Self {
            Self::Writer(ImmutableSourceStreamingWriteError::Format(error))
        }
    }

    #[derive(Debug)]
    struct CandidateReport {
        output: ImmutableSourceStreamingWriteReport,
        descriptor_stage_bytes: u64,
        descriptor_spill: BoundedSpillSortReport,
    }

    struct PreparedDescriptors {
        path: PathBuf,
        file: Option<File>,
        records: u64,
        bytes: u64,
        spill: BoundedSpillSortReport,
    }

    impl PreparedDescriptors {
        fn visit<F>(&self, mut visit: F) -> Result<(), CandidateError>
        where
            F: FnMut(SourceDescriptor) -> Result<(), CandidateError>,
        {
            let mut file = self
                .file
                .as_ref()
                .ok_or(CandidateError::Stage("closed descriptor stage"))?
                .try_clone()
                .map_err(|error| CandidateError::StageIo(error.kind()))?;
            file.seek(SeekFrom::Start(0))
                .map_err(|error| CandidateError::StageIo(error.kind()))?;
            let mut reader = BufReader::new(file);
            let mut bytes = [0u8; DESCRIPTOR_BYTES];
            for _ in 0..self.records {
                reader
                    .read_exact(&mut bytes)
                    .map_err(|error| CandidateError::StageIo(error.kind()))?;
                visit(SourceDescriptor::decode(&bytes)?)?;
            }
            let mut trailing = [0u8; 1];
            match reader.read(&mut trailing) {
                Ok(0) => Ok(()),
                Ok(_) => Err(CandidateError::Stage("descriptor trailing bytes")),
                Err(error) => Err(CandidateError::StageIo(error.kind())),
            }
        }
    }

    impl Drop for PreparedDescriptors {
        fn drop(&mut self) {
            drop(self.file.take());
            let _ = fs::remove_file(&self.path);
        }
    }

    struct DescriptorRecords<'a, S> {
        sources: std::iter::Enumerate<std::slice::IterMut<'a, S>>,
        options: ImmutableSourceStreamingWriteOptions,
        limits: ImmutableLimits,
        input_error: &'a mut Option<ImmutableSourceStreamingWriteError>,
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
            let result = (|| {
                let object_id = source.object_id();
                let kind = source.kind();
                if object_id == 0 || kind == 0 {
                    return Err(ImmutableError::Invalid("object input").into());
                }
                let logical_len = source.logical_len();
                let length = usize::try_from(logical_len)
                    .map_err(|_| ImmutableError::Limit("object size"))?;
                let record_len = OBJECT_HEADER_LEN
                    .checked_add(length)
                    .ok_or(ImmutableError::Limit("object size"))?;
                *self.object_bytes = self
                    .object_bytes
                    .checked_add(record_len)
                    .ok_or(ImmutableError::Limit("output"))?;
                *self.largest_source_buffer = (*self.largest_source_buffer)
                    .max(length.min(self.options.max_source_read_bytes));
                if *self.largest_source_buffer > self.limits.max_allocation_bytes {
                    return Err(ImmutableError::Limit("allocation").into());
                }
                let strong_version = source.strong_version().map_err(|label| {
                    ImmutableSourceStreamingWriteError::Source { object_id, label }
                })?;
                *self.version_checks = self
                    .version_checks
                    .checked_add(1)
                    .ok_or(ImmutableError::Limit("version checks"))?;
                let descriptor = SourceDescriptor {
                    object_id,
                    source_index: u64::try_from(index)
                        .map_err(|_| ImmutableError::Limit("source index"))?,
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

    struct CandidatePreflight {
        stage: PreparedDescriptors,
        expected_bytes: usize,
        expected_pages: usize,
        expected_root_level: u8,
        largest_source_buffer: usize,
        version_checks: u64,
    }

    fn create_stage(directory: &Path) -> Result<(PathBuf, File), CandidateError> {
        let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            ".ucof-candidate-source-descriptors-{}-{sequence}.bin",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let file = options
            .open(&path)
            .map_err(|error| CandidateError::StageIo(error.kind()))?;
        Ok((path, file))
    }

    fn prepare_source_descriptors<S: ImmutableStreamingPayloadSource>(
        directory: &Path,
        sources: &mut [S],
        options: ImmutableSourceStreamingWriteOptions,
        limits: ImmutableLimits,
        spill_limits: BoundedSpillSortLimits,
    ) -> Result<CandidatePreflight, CandidateError> {
        if sources.is_empty() || sources.len() > limits.max_objects {
            return Err(ImmutableError::Limit("object count").into());
        }
        if options.output.max_write_request_bytes == 0 || options.max_source_read_bytes == 0 {
            return Err(ImmutableError::Limit("streaming configuration").into());
        }
        if spill_limits.record_bytes != DESCRIPTOR_BYTES {
            return Err(CandidateError::Stage("descriptor spill record size"));
        }
        allocation_check::<Locator>(sources.len(), limits)?;

        let (path, file) = create_stage(directory)?;
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
        let mut writer = BufWriter::new(file);
        let sorted = bounded_spill_sort_to(directory, records, &mut writer, spill_limits);
        if let Some(error) = input_error {
            drop(writer);
            let _ = fs::remove_file(&path);
            return Err(CandidateError::Writer(error));
        }
        let spill = match sorted {
            Ok(report) => report,
            Err(error) => {
                drop(writer);
                let _ = fs::remove_file(&path);
                return Err(CandidateError::Spill(error));
            }
        };
        writer
            .flush()
            .map_err(|error| CandidateError::StageIo(error.kind()))?;
        let retained = writer
            .get_ref()
            .try_clone()
            .map_err(|error| CandidateError::StageIo(error.kind()))?;
        drop(writer);

        let descriptor_bytes =
            u64::try_from(DESCRIPTOR_BYTES).expect("descriptor byte width fits u64");
        let stage_bytes = spill
            .output_records
            .checked_mul(descriptor_bytes)
            .ok_or(CandidateError::Stage("descriptor stage size"))?;
        let on_disk = retained
            .metadata()
            .map_err(|error| CandidateError::StageIo(error.kind()))?
            .len();
        if spill.output_records
            != u64::try_from(sources.len()).map_err(|_| ImmutableError::Limit("object count"))?
            || spill.output_payload_bytes != stage_bytes
            || on_disk != stage_bytes
        {
            drop(retained);
            let _ = fs::remove_file(&path);
            return Err(CandidateError::Stage("descriptor stage size"));
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

        Ok(CandidatePreflight {
            stage: PreparedDescriptors {
                path,
                file: Some(retained),
                records: spill.output_records,
                bytes: stage_bytes,
                spill,
            },
            expected_bytes,
            expected_pages,
            expected_root_level,
            largest_source_buffer,
            version_checks,
        })
    }

    fn write_genesis_sources_bounded_candidate<W, S>(
        writer: &mut W,
        sources: &mut [S],
        directory: &Path,
        options: ImmutableSourceStreamingWriteOptions,
        limits: ImmutableLimits,
        spill_limits: BoundedSpillSortLimits,
    ) -> Result<CandidateReport, CandidateError>
    where
        W: Write,
        S: ImmutableStreamingPayloadSource,
    {
        let preflight = prepare_source_descriptors(
            directory,
            sources,
            options,
            limits,
            spill_limits,
        )?;
        let descriptor_stage_bytes = preflight.stage.bytes;
        let descriptor_spill = preflight.stage.spill.clone();
        let mut sink = StreamingSink::new(writer, options.output.max_write_request_bytes)?;
        let mut header = [0u8; FILE_HEADER_LEN];
        header[..8].copy_from_slice(FILE_MAGIC);
        sink.write_commit_bytes(&header)?;

        let mut buffer = vec![0u8; preflight.largest_source_buffer];
        let source_count = sources.len();
        let mut locators = Vec::with_capacity(source_count);
        let mut counters = SourceStreamingCounters {
            version_checks: preflight.version_checks,
            ..SourceStreamingCounters::default()
        };
        preflight.stage.visit(|descriptor| {
            let index = usize::try_from(descriptor.source_index)
                .map_err(|_| CandidateError::Stage("source index"))?;
            let source = sources
                .get_mut(index)
                .ok_or(CandidateError::Stage("source index"))?;
            if source.object_id() != descriptor.object_id
                || source.kind() != descriptor.kind
                || source.logical_len() != descriptor.logical_len
            {
                return Err(CandidateError::Writer(
                    ImmutableSourceStreamingWriteError::Source {
                        object_id: descriptor.object_id,
                        label: "source metadata changed",
                    },
                ));
            }
            let logical_len = usize::try_from(descriptor.logical_len)
                .map_err(|_| ImmutableError::Limit("object size"))?;
            locators.push(write_source_streaming_object(
                &mut sink,
                source,
                descriptor.strong_version,
                logical_len,
                &mut buffer,
                &mut counters,
            )?);
            Ok(())
        })?;

        let (root, page_count) = write_streaming_tree(&mut sink, &locators, limits)?;
        if page_count != preflight.expected_pages || root.level != preflight.expected_root_level {
            return Err(ImmutableError::Invalid("streaming tree shape").into());
        }
        let mut report = write_streaming_publication(&mut sink, &root, page_count)?;
        report.object_count = locators.len();
        if sink.offset != preflight.expected_bytes {
            return Err(ImmutableError::Invalid("streaming output length").into());
        }
        Ok(CandidateReport {
            output: ImmutableSourceStreamingWriteReport {
                output: ImmutableStreamingWriteReport {
                    report,
                    bytes_written: sink.offset,
                    largest_write_request: sink.largest_write_request,
                    locator_entries: locators.len(),
                },
                source_read_operations: counters.source_read_operations,
                source_bytes_read: counters.source_bytes_read,
                version_checks: counters.version_checks,
                largest_source_buffer: buffer.len(),
            },
            descriptor_stage_bytes,
            descriptor_spill,
        })
    }

    #[derive(Clone, Debug)]
    struct CandidateSource {
        object_id: u64,
        kind: u16,
        bytes: Vec<u8>,
        version: [u8; 32],
        fail_version: bool,
    }

    impl CandidateSource {
        fn new(object_id: u64, bytes: Vec<u8>) -> Self {
            Self {
                object_id,
                kind: u16::try_from(1 + object_id % 17).expect("kind"),
                bytes,
                version: [u8::try_from(object_id % 251).expect("version seed"); 32],
                fail_version: false,
            }
        }
    }

    impl ImmutableStreamingPayloadSource for CandidateSource {
        fn object_id(&self) -> u64 {
            self.object_id
        }

        fn kind(&self) -> u16 {
            self.kind
        }

        fn logical_len(&self) -> u64 {
            u64::try_from(self.bytes.len()).expect("payload length")
        }

        fn strong_version(&mut self) -> Result<[u8; 32], &'static str> {
            if self.fail_version {
                Err("metadata version failure")
            } else {
                Ok(self.version)
            }
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            buffer: &mut [u8],
        ) -> Result<(), &'static str> {
            let start = usize::try_from(offset).map_err(|_| "offset")?;
            let end = start.checked_add(buffer.len()).ok_or("range")?;
            buffer.copy_from_slice(self.bytes.get(start..end).ok_or("range")?);
            Ok(())
        }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ucof-bounded-source-candidate-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create candidate directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn spill_limits(run_records: usize, max_open_inputs: usize) -> BoundedSpillSortLimits {
        BoundedSpillSortLimits {
            record_bytes: DESCRIPTOR_BYTES,
            run_records,
            max_records: 10_000,
            max_initial_runs: 10_000,
            max_open_inputs,
            max_merge_passes: 32,
            max_live_spill_bytes: 64 * 1024 * 1024,
            max_spill_bytes_written: 256 * 1024 * 1024,
            max_merge_bytes_read: 256 * 1024 * 1024,
            max_merge_bytes_written: 256 * 1024 * 1024,
        }
    }

    #[test]
    fn bounded_source_candidate_matches_current_writer_across_spill_geometry() {
        let limits = ImmutableLimits::default();
        let options = ImmutableSourceStreamingWriteOptions {
            output: ImmutableStreamingWriteOptions {
                max_write_request_bytes: 113,
            },
            max_source_read_bytes: 31,
        };
        let original: Vec<_> = (1..=401u64)
            .rev()
            .map(|object_id| {
                CandidateSource::new(
                    object_id,
                    vec![u8::try_from(object_id % 251).expect("seed"); 257],
                )
            })
            .collect();

        let mut baseline_sources = original.clone();
        let mut baseline = Vec::new();
        let baseline_report = write_genesis_sources_to(
            &mut baseline,
            &mut baseline_sources,
            options,
            limits,
        )
        .expect("current source writer");

        let first_directory = TestDirectory::new("first");
        let mut first_sources = original.clone();
        let mut first = Vec::new();
        let first_report = write_genesis_sources_bounded_candidate(
            &mut first,
            &mut first_sources,
            &first_directory.0,
            options,
            limits,
            spill_limits(17, 3),
        )
        .expect("first bounded source writer");

        let second_directory = TestDirectory::new("second");
        let mut second_sources = original;
        let mut second = Vec::new();
        let second_report = write_genesis_sources_bounded_candidate(
            &mut second,
            &mut second_sources,
            &second_directory.0,
            options,
            limits,
            spill_limits(53, 7),
        )
        .expect("second bounded source writer");

        assert_eq!(first, baseline);
        assert_eq!(second, baseline);
        assert_eq!(first_report.output, baseline_report);
        assert_eq!(second_report.output, baseline_report);
        let descriptor_bytes = u64::try_from(DESCRIPTOR_BYTES).expect("descriptor bytes");
        assert_eq!(first_report.descriptor_stage_bytes, 401 * descriptor_bytes);
        assert_eq!(second_report.descriptor_stage_bytes, 401 * descriptor_bytes);
        assert_ne!(
            first_report.descriptor_spill.initial_runs,
            second_report.descriptor_spill.initial_runs
        );
        assert!(fs::read_dir(&first_directory.0).unwrap().next().is_none());
        assert!(fs::read_dir(&second_directory.0).unwrap().next().is_none());
    }

    #[test]
    fn duplicate_descriptor_preflight_leaves_output_untouched() {
        let directory = TestDirectory::new("duplicate");
        let mut sources = [
            CandidateSource::new(2, vec![1; 10]),
            CandidateSource::new(2, vec![2; 10]),
        ];
        let mut output = Vec::new();
        let error = write_genesis_sources_bounded_candidate(
            &mut output,
            &mut sources,
            &directory.0,
            ImmutableSourceStreamingWriteOptions::default(),
            ImmutableLimits::default(),
            spill_limits(1, 2),
        )
        .expect_err("duplicate descriptor");
        assert!(matches!(
            error,
            CandidateError::Spill(BoundedSpillSortError::DuplicateKey(2))
        ));
        assert!(output.is_empty());
        assert!(fs::read_dir(&directory.0).unwrap().next().is_none());
    }

    #[test]
    fn metadata_failure_after_completed_run_leaves_output_untouched() {
        let directory = TestDirectory::new("metadata-failure");
        let mut sources = [
            CandidateSource::new(2, vec![2; 10]),
            CandidateSource::new(1, vec![1; 10]),
            CandidateSource::new(3, vec![3; 10]),
        ];
        sources[2].fail_version = true;
        let mut output = Vec::new();
        let error = write_genesis_sources_bounded_candidate(
            &mut output,
            &mut sources,
            &directory.0,
            ImmutableSourceStreamingWriteOptions::default(),
            ImmutableLimits::default(),
            spill_limits(2, 2),
        )
        .expect_err("metadata failure");
        assert!(matches!(
            error,
            CandidateError::Writer(ImmutableSourceStreamingWriteError::Source {
                object_id: 3,
                label: "metadata version failure"
            })
        ));
        assert!(output.is_empty());
        assert!(fs::read_dir(&directory.0).unwrap().next().is_none());
    }
}
