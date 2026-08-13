#[cfg(test)]
mod bounded_locator_stage_candidate_tests {
    use super::*;
    mod group_iter {
        include!("../canonical_group_iter_candidate.rs");
    }
    use group_iter::{CanonicalGroupIterError, CanonicalGroupSizesIter};
    use std::fs::{self, File, OpenOptions};
    use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const STAGED_LOCATOR_BYTES: usize = 72;
    static NEXT_LOCATOR_STAGE: AtomicU64 = AtomicU64::new(1);

    #[derive(Debug)]
    enum CandidateError {
        Writer(ImmutableSourceStreamingWriteError),
        StageIo(std::io::ErrorKind),
        Stage(&'static str),
    }

    impl From<ImmutableSourceStreamingWriteError> for CandidateError {
        fn from(error: ImmutableSourceStreamingWriteError) -> Self {
            Self::Writer(error)
        }
    }

    impl From<ImmutableStreamingWriteError> for CandidateError {
        fn from(error: ImmutableStreamingWriteError) -> Self {
            Self::Writer(ImmutableSourceStreamingWriteError::from(error))
        }
    }

    impl From<ImmutableError> for CandidateError {
        fn from(error: ImmutableError) -> Self {
            Self::Writer(ImmutableSourceStreamingWriteError::Format(error))
        }
    }

    fn group_sizes(
        total: usize,
        capacity: usize,
        minimum: usize,
    ) -> Result<CanonicalGroupSizesIter, CandidateError> {
        CanonicalGroupSizesIter::new(total, capacity, minimum).map_err(|error| match error {
            CanonicalGroupIterError::Invalid => CandidateError::Stage("canonical occupancy"),
            CanonicalGroupIterError::Overflow => {
                CandidateError::Writer(ImmutableSourceStreamingWriteError::Format(
                    ImmutableError::Limit("page count"),
                ))
            }
        })
    }

    fn encode_locator(locator: &Locator) -> [u8; STAGED_LOCATOR_BYTES] {
        let mut bytes = [0u8; STAGED_LOCATOR_BYTES];
        bytes[..8].copy_from_slice(&locator.object_id.to_le_bytes());
        bytes[8..10].copy_from_slice(&locator.kind.to_le_bytes());
        bytes[16..24].copy_from_slice(&locator.record_offset.to_le_bytes());
        bytes[24..32].copy_from_slice(&locator.record_len.to_le_bytes());
        bytes[32..40].copy_from_slice(&locator.logical_len.to_le_bytes());
        bytes[40..72].copy_from_slice(&locator.digest);
        bytes
    }

    fn decode_locator(bytes: &[u8; STAGED_LOCATOR_BYTES]) -> Result<Locator, CandidateError> {
        if bytes[10..16].iter().any(|byte| *byte != 0) {
            return Err(CandidateError::Stage("locator reserved bytes"));
        }
        let object_id = u64::from_le_bytes(bytes[..8].try_into().expect("locator field"));
        let kind = u16::from_le_bytes(bytes[8..10].try_into().expect("locator field"));
        if object_id == 0 || kind == 0 {
            return Err(CandidateError::Stage("locator identity"));
        }
        Ok(Locator {
            object_id,
            kind,
            record_offset: u64::from_le_bytes(bytes[16..24].try_into().expect("locator field")),
            record_len: u64::from_le_bytes(bytes[24..32].try_into().expect("locator field")),
            logical_len: u64::from_le_bytes(bytes[32..40].try_into().expect("locator field")),
            digest: bytes[40..72].try_into().expect("locator digest"),
        })
    }

    struct LocatorStage {
        path: PathBuf,
        file: File,
        records: usize,
    }

    impl LocatorStage {
        fn create(directory: &Path) -> Result<Self, CandidateError> {
            let sequence = NEXT_LOCATOR_STAGE.fetch_add(1, Ordering::Relaxed);
            let path = directory.join(format!(
                ".ucof-locator-stage-{}-{sequence}.bin",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let file = options
                .open(&path)
                .map_err(|error| CandidateError::StageIo(error.kind()))?;
            Ok(Self {
                path,
                file,
                records: 0,
            })
        }

        fn writer(&self) -> Result<BufWriter<File>, CandidateError> {
            self.file
                .try_clone()
                .map(BufWriter::new)
                .map_err(|error| CandidateError::StageIo(error.kind()))
        }

        fn reader(&self) -> Result<BufReader<File>, CandidateError> {
            let mut file = self
                .file
                .try_clone()
                .map_err(|error| CandidateError::StageIo(error.kind()))?;
            file.seek(SeekFrom::Start(0))
                .map_err(|error| CandidateError::StageIo(error.kind()))?;
            Ok(BufReader::new(file))
        }
    }

    impl Drop for LocatorStage {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[derive(Clone, Debug)]
    struct LocatorStageReport {
        output: ImmutableSourceStreamingWriteReport,
        locator_stage_bytes: u64,
        peak_locator_buffer_entries: usize,
        peak_page_frontier_entries: usize,
    }

    fn write_tree_from_locator_stage<W: Write>(
        sink: &mut StreamingSink<'_, W>,
        stage: &LocatorStage,
        limits: ImmutableLimits,
    ) -> Result<(PageRef, usize, usize, usize), CandidateError> {
        let mut reader = stage.reader()?;
        let mut pages = 0usize;
        let mut peak_locator_buffer_entries = 0usize;
        let mut level = Vec::new();
        for size in group_sizes(stage.records, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY)? {
            if pages >= limits.max_pages {
                return Err(ImmutableError::Limit("page count").into());
            }
            peak_locator_buffer_entries = peak_locator_buffer_entries.max(size);
            let mut entries = Vec::with_capacity(size);
            let mut raw = [0u8; STAGED_LOCATOR_BYTES];
            for _ in 0..size {
                reader
                    .read_exact(&mut raw)
                    .map_err(|error| CandidateError::StageIo(error.kind()))?;
                entries.push(decode_locator(&raw)?);
            }
            level.push(sink.write_page(&encode_leaf(&entries)?)?);
            pages += 1;
        }
        let mut trailing = [0u8; 1];
        match reader.read(&mut trailing) {
            Ok(0) => {}
            Ok(_) => return Err(CandidateError::Stage("locator trailing bytes")),
            Err(error) => return Err(CandidateError::StageIo(error.kind())),
        }

        let mut peak_page_frontier_entries = level.len();
        while level.len() > 1 {
            let parent_level = level[0]
                .level
                .checked_add(1)
                .ok_or(ImmutableError::Limit("page depth"))?;
            if parent_level > limits.max_depth {
                return Err(ImmutableError::Limit("page depth").into());
            }
            let mut next = Vec::with_capacity(level.len().div_ceil(INTERNAL_FANOUT));
            let mut start = 0usize;
            for size in group_sizes(level.len(), INTERNAL_FANOUT, INTERNAL_MIN_OCCUPANCY)? {
                if pages >= limits.max_pages {
                    return Err(ImmutableError::Limit("page count").into());
                }
                let end = start
                    .checked_add(size)
                    .ok_or(ImmutableError::Limit("page count"))?;
                next.push(sink.write_page(&encode_internal(
                    &level[start..end],
                    parent_level,
                )?)?);
                pages += 1;
                start = end;
            }
            level = next;
            peak_page_frontier_entries = peak_page_frontier_entries.max(level.len());
        }
        Ok((
            level.pop().ok_or(CandidateError::Stage("empty tree"))?,
            pages,
            peak_locator_buffer_entries,
            peak_page_frontier_entries,
        ))
    }

    fn write_genesis_sources_locator_staged<W, S>(
        writer: &mut W,
        sources: &mut [S],
        directory: &Path,
        options: ImmutableSourceStreamingWriteOptions,
        limits: ImmutableLimits,
    ) -> Result<LocatorStageReport, CandidateError>
    where
        W: Write,
        S: ImmutableStreamingPayloadSource,
    {
        let preflight = preflight_source_streaming(sources, options, limits)?;
        let mut sink = StreamingSink::new(writer, options.output.max_write_request_bytes)?;
        let mut header = [0u8; FILE_HEADER_LEN];
        header[..8].copy_from_slice(FILE_MAGIC);
        sink.write_commit_bytes(&header)?;

        let mut stage = LocatorStage::create(directory)?;
        let mut stage_writer = stage.writer()?;
        let mut buffer = vec![0u8; preflight.largest_source_buffer];
        let mut counters = SourceStreamingCounters {
            version_checks: preflight.version_checks,
            ..SourceStreamingCounters::default()
        };
        for index in preflight.order {
            let locator = write_source_streaming_object(
                &mut sink,
                &mut sources[index],
                preflight.versions[index],
                preflight.lengths[index],
                &mut buffer,
                &mut counters,
            )?;
            stage_writer
                .write_all(&encode_locator(&locator))
                .map_err(|error| CandidateError::StageIo(error.kind()))?;
            stage.records = stage
                .records
                .checked_add(1)
                .ok_or(ImmutableError::Limit("object count"))?;
        }
        stage_writer
            .flush()
            .map_err(|error| CandidateError::StageIo(error.kind()))?;
        drop(stage_writer);
        let locator_stage_bytes = u64::try_from(stage.records)
            .map_err(|_| ImmutableError::Limit("object count"))?
            .checked_mul(u64::try_from(STAGED_LOCATOR_BYTES).expect("locator width"))
            .ok_or(ImmutableError::Limit("allocation"))?;
        if stage
            .file
            .metadata()
            .map_err(|error| CandidateError::StageIo(error.kind()))?
            .len()
            != locator_stage_bytes
        {
            return Err(CandidateError::Stage("locator stage length"));
        }

        let (root, page_count, peak_locator_buffer_entries, peak_page_frontier_entries) =
            write_tree_from_locator_stage(&mut sink, &stage, limits)?;
        if page_count != preflight.expected_pages || root.level != preflight.expected_root_level {
            return Err(ImmutableError::Invalid("streaming tree shape").into());
        }
        let mut report = write_streaming_publication(&mut sink, &root, page_count)?;
        report.object_count = stage.records;
        if sink.offset != preflight.expected_bytes {
            return Err(ImmutableError::Invalid("streaming output length").into());
        }
        Ok(LocatorStageReport {
            output: ImmutableSourceStreamingWriteReport {
                output: ImmutableStreamingWriteReport {
                    report,
                    bytes_written: sink.offset,
                    largest_write_request: sink.largest_write_request,
                    locator_entries: stage.records,
                },
                source_read_operations: counters.source_read_operations,
                source_bytes_read: counters.source_bytes_read,
                version_checks: counters.version_checks,
                largest_source_buffer: buffer.len(),
            },
            locator_stage_bytes,
            peak_locator_buffer_entries,
            peak_page_frontier_entries,
        })
    }

    #[derive(Clone, Debug)]
    struct MemorySource {
        object_id: u64,
        kind: u16,
        bytes: Vec<u8>,
        version: [u8; 32],
    }

    impl MemorySource {
        fn new(object_id: u64) -> Self {
            Self {
                object_id,
                kind: u16::try_from(1 + object_id % 17).expect("kind"),
                bytes: vec![u8::try_from(object_id % 251).expect("seed"); 257],
                version: [u8::try_from(object_id % 251).expect("version"); 32],
            }
        }
    }

    impl ImmutableStreamingPayloadSource for MemorySource {
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
            Ok(self.version)
        }

        fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), &'static str> {
            let start = usize::try_from(offset).map_err(|_| "offset")?;
            let end = start.checked_add(buffer.len()).ok_or("range")?;
            buffer.copy_from_slice(self.bytes.get(start..end).ok_or("range")?);
            Ok(())
        }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_LOCATOR_STAGE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ucof-locator-stage-test-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn staged_locators_preserve_canonical_bytes_with_leaf_bounded_memory() {
        let limits = ImmutableLimits::default();
        let options = ImmutableSourceStreamingWriteOptions {
            output: ImmutableStreamingWriteOptions {
                max_write_request_bytes: 113,
            },
            max_source_read_bytes: 31,
        };
        let original: Vec<_> = (1..=2_003u64).rev().map(MemorySource::new).collect();

        let mut baseline_sources = original.clone();
        let mut baseline = Vec::new();
        let baseline_report = write_genesis_sources_to(
            &mut baseline,
            &mut baseline_sources,
            options,
            limits,
        )
        .expect("baseline source writer");

        let directory = TestDirectory::new();
        let mut staged_sources = original;
        let mut staged = Vec::new();
        let staged_report = write_genesis_sources_locator_staged(
            &mut staged,
            &mut staged_sources,
            &directory.0,
            options,
            limits,
        )
        .expect("locator staged writer");

        assert_eq!(staged, baseline);
        assert_eq!(staged_report.output, baseline_report);
        assert_eq!(staged_report.peak_locator_buffer_entries, LEAF_CAPACITY);
        assert!(staged_report.peak_page_frontier_entries < 2_003);
        assert_eq!(
            staged_report.locator_stage_bytes,
            2_003 * u64::try_from(STAGED_LOCATOR_BYTES).expect("locator width")
        );
        assert!(fs::read_dir(&directory.0).unwrap().next().is_none());
    }
}
