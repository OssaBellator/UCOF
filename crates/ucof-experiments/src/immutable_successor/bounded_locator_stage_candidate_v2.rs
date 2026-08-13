#[cfg(test)]
mod bounded_locator_stage_candidate_v2_tests {
    use super::*;
    mod group_iter {
        include!("../canonical_group_iter_candidate.rs");
    }
    use group_iter::CanonicalGroupSizesIter;
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Seek, SeekFrom, Write};
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const STAGED_LOCATOR_BYTES: usize = 72;
    static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);
    type CandidateResult<T> = Result<T, String>;

    fn encode_locator(locator: &Locator) -> [u8; STAGED_LOCATOR_BYTES] {
        let mut bytes = [0u8; STAGED_LOCATOR_BYTES];
        bytes[..8].copy_from_slice(&locator.object_id.to_le_bytes());
        bytes[8..10].copy_from_slice(&locator.kind.to_le_bytes());
        bytes[16..24].copy_from_slice(&locator.record_offset.to_le_bytes());
        bytes[24..32].copy_from_slice(&locator.record_len.to_le_bytes());
        bytes[32..40].copy_from_slice(&locator.logical_len.to_le_bytes());
        bytes[40..].copy_from_slice(&locator.digest);
        bytes
    }

    fn decode_locator(bytes: &[u8; STAGED_LOCATOR_BYTES]) -> CandidateResult<Locator> {
        if bytes[10..16].iter().any(|byte| *byte != 0) {
            return Err("locator reserved bytes".into());
        }
        let object_id = u64::from_le_bytes(bytes[..8].try_into().expect("locator field"));
        let kind = u16::from_le_bytes(bytes[8..10].try_into().expect("locator field"));
        if object_id == 0 || kind == 0 {
            return Err("locator identity".into());
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
        fn create(directory: &Path) -> CandidateResult<Self> {
            let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let path = directory.join(format!(
                ".ucof-locator-stage-v2-{}-{sequence}.bin",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let file = options.open(&path).map_err(|error| error.to_string())?;
            Ok(Self {
                path,
                file,
                records: 0,
            })
        }

        fn push(&mut self, locator: &Locator) -> CandidateResult<()> {
            self.file
                .write_all(&encode_locator(locator))
                .map_err(|error| error.to_string())?;
            self.records = self
                .records
                .checked_add(1)
                .ok_or_else(|| "locator count overflow".to_owned())?;
            Ok(())
        }

        fn rewind(&mut self) -> CandidateResult<()> {
            self.file.flush().map_err(|error| error.to_string())?;
            self.file
                .seek(SeekFrom::Start(0))
                .map_err(|error| error.to_string())?;
            Ok(())
        }
    }

    impl Drop for LocatorStage {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn groups(
        total: usize,
        capacity: usize,
        minimum: usize,
    ) -> CandidateResult<CanonicalGroupSizesIter> {
        CanonicalGroupSizesIter::new(total, capacity, minimum)
            .map_err(|error| format!("canonical grouping failed: {error:?}"))
    }

    fn write_tree<W: Write>(
        sink: &mut StreamingSink<'_, W>,
        stage: &mut LocatorStage,
        limits: ImmutableLimits,
    ) -> CandidateResult<(PageRef, usize, usize, usize)> {
        stage.rewind()?;
        let mut pages = 0usize;
        let mut peak_locator_entries = 0usize;
        let mut level = Vec::new();
        for size in groups(stage.records, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY)? {
            peak_locator_entries = peak_locator_entries.max(size);
            let mut locators = Vec::with_capacity(size);
            let mut raw = [0u8; STAGED_LOCATOR_BYTES];
            for _ in 0..size {
                stage
                    .file
                    .read_exact(&mut raw)
                    .map_err(|error| error.to_string())?;
                locators.push(decode_locator(&raw)?);
            }
            level.push(
                sink.write_page(&encode_leaf(&locators).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?,
            );
            pages += 1;
        }
        let mut trailing = [0u8; 1];
        if stage
            .file
            .read(&mut trailing)
            .map_err(|error| error.to_string())?
            != 0
        {
            return Err("locator stage has trailing bytes".into());
        }

        let mut peak_frontier_entries = level.len();
        while level.len() > 1 {
            let parent_level = level[0]
                .level
                .checked_add(1)
                .ok_or_else(|| "page depth overflow".to_owned())?;
            if parent_level > limits.max_depth {
                return Err("page depth limit".into());
            }
            let mut next = Vec::with_capacity(level.len().div_ceil(INTERNAL_FANOUT));
            let mut start = 0usize;
            for size in groups(level.len(), INTERNAL_FANOUT, INTERNAL_MIN_OCCUPANCY)? {
                let end = start
                    .checked_add(size)
                    .ok_or_else(|| "page count overflow".to_owned())?;
                next.push(
                    sink.write_page(
                        &encode_internal(&level[start..end], parent_level)
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?,
                );
                pages += 1;
                start = end;
            }
            level = next;
            peak_frontier_entries = peak_frontier_entries.max(level.len());
        }
        Ok((
            level.pop().ok_or_else(|| "empty staged tree".to_owned())?,
            pages,
            peak_locator_entries,
            peak_frontier_entries,
        ))
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
            let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ucof-locator-stage-v2-test-{}-{sequence}",
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
    fn staged_locators_match_current_writer_and_cap_locator_ram_to_one_leaf() {
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
        .expect("baseline writer");

        let directory = TestDirectory::new();
        let mut sources = original;
        let preflight = preflight_source_streaming(&mut sources, options, limits)
            .expect("source preflight");
        let mut output = Vec::new();
        let mut stage = LocatorStage::create(&directory.0).expect("locator stage");
        let staged_report;
        let peak_locator_entries;
        let peak_frontier_entries;
        {
            let mut sink = StreamingSink::new(&mut output, options.output.max_write_request_bytes)
                .expect("streaming sink");
            let mut header = [0u8; FILE_HEADER_LEN];
            header[..8].copy_from_slice(FILE_MAGIC);
            sink.write_commit_bytes(&header).expect("write header");
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
                )
                .expect("write source object");
                stage.push(&locator).expect("stage locator");
            }
            let (root, page_count, locator_peak, frontier_peak) =
                write_tree(&mut sink, &mut stage, limits).expect("write staged tree");
            peak_locator_entries = locator_peak;
            peak_frontier_entries = frontier_peak;
            assert_eq!(page_count, preflight.expected_pages);
            assert_eq!(root.level, preflight.expected_root_level);
            let mut report = write_streaming_publication(&mut sink, &root, page_count)
                .expect("write publication");
            report.object_count = stage.records;
            assert_eq!(sink.offset, preflight.expected_bytes);
            staged_report = ImmutableSourceStreamingWriteReport {
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
            };
        }

        assert_eq!(output, baseline);
        assert_eq!(staged_report, baseline_report);
        assert_eq!(peak_locator_entries, LEAF_CAPACITY);
        assert!(peak_frontier_entries < 2_003);
        let expected_stage_bytes = 2_003u64
            * u64::try_from(STAGED_LOCATOR_BYTES).expect("staged locator width");
        assert_eq!(stage.file.metadata().unwrap().len(), expected_stage_bytes);
        drop(stage);
        assert!(fs::read_dir(&directory.0).unwrap().next().is_none());
    }
}
