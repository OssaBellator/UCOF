#[cfg(test)]
mod bounded_page_ref_stage_candidate_tests {
    use super::*;
    mod group_iter {
        include!("../canonical_group_iter_candidate.rs");
    }
    use group_iter::CanonicalGroupSizesIter;
    use std::fs::{self, File, OpenOptions};
    use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const LOCATOR_STAGE_BYTES: usize = 72;
    const PAGE_REF_STAGE_BYTES: usize = 64;
    static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);
    type CandidateResult<T> = Result<T, String>;

    struct FixedStage {
        path: PathBuf,
        file: Option<File>,
        records: usize,
        record_bytes: usize,
    }

    impl FixedStage {
        fn create(directory: &Path, label: &str, record_bytes: usize) -> CandidateResult<Self> {
            let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let path = directory.join(format!(
                ".ucof-{label}-{}-{sequence}.bin",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let file = options.open(&path).map_err(|error| error.to_string())?;
            Ok(Self {
                path,
                file: Some(file),
                records: 0,
                record_bytes,
            })
        }

        fn writer(&self) -> CandidateResult<BufWriter<File>> {
            self.file
                .as_ref()
                .ok_or_else(|| "closed stage".to_owned())?
                .try_clone()
                .map(BufWriter::new)
                .map_err(|error| error.to_string())
        }

        fn reader(&self) -> CandidateResult<BufReader<File>> {
            let mut file = self
                .file
                .as_ref()
                .ok_or_else(|| "closed stage".to_owned())?
                .try_clone()
                .map_err(|error| error.to_string())?;
            file.seek(SeekFrom::Start(0))
                .map_err(|error| error.to_string())?;
            Ok(BufReader::new(file))
        }

        fn note_record(&mut self) -> CandidateResult<()> {
            self.records = self
                .records
                .checked_add(1)
                .ok_or_else(|| "stage record overflow".to_owned())?;
            Ok(())
        }

        fn bytes(&self) -> CandidateResult<u64> {
            u64::try_from(self.records)
                .map_err(|_| "stage count overflow".to_owned())?
                .checked_mul(u64::try_from(self.record_bytes).expect("record width fits u64"))
                .ok_or_else(|| "stage byte overflow".to_owned())
        }
    }

    impl Drop for FixedStage {
        fn drop(&mut self) {
            drop(self.file.take());
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

    fn encode_locator(locator: &Locator) -> [u8; LOCATOR_STAGE_BYTES] {
        let mut bytes = [0u8; LOCATOR_STAGE_BYTES];
        bytes[..8].copy_from_slice(&locator.object_id.to_le_bytes());
        bytes[8..10].copy_from_slice(&locator.kind.to_le_bytes());
        bytes[16..24].copy_from_slice(&locator.record_offset.to_le_bytes());
        bytes[24..32].copy_from_slice(&locator.record_len.to_le_bytes());
        bytes[32..40].copy_from_slice(&locator.logical_len.to_le_bytes());
        bytes[40..72].copy_from_slice(&locator.digest);
        bytes
    }

    fn decode_locator(bytes: &[u8; LOCATOR_STAGE_BYTES]) -> CandidateResult<Locator> {
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

    fn encode_page_ref(reference: &PageRef) -> [u8; PAGE_REF_STAGE_BYTES] {
        let mut bytes = [0u8; PAGE_REF_STAGE_BYTES];
        bytes[..8].copy_from_slice(&reference.minimum.to_le_bytes());
        bytes[8..16].copy_from_slice(&reference.maximum.to_le_bytes());
        bytes[16..24].copy_from_slice(&reference.offset.to_le_bytes());
        bytes[24] = reference.level;
        bytes[32..64].copy_from_slice(&reference.digest);
        bytes
    }

    fn decode_page_ref(bytes: &[u8; PAGE_REF_STAGE_BYTES]) -> CandidateResult<PageRef> {
        if bytes[25..32].iter().any(|byte| *byte != 0) {
            return Err("page-ref reserved bytes".into());
        }
        Ok(PageRef {
            minimum: u64::from_le_bytes(bytes[..8].try_into().expect("page-ref field")),
            maximum: u64::from_le_bytes(bytes[8..16].try_into().expect("page-ref field")),
            offset: u64::from_le_bytes(bytes[16..24].try_into().expect("page-ref field")),
            level: bytes[24],
            digest: bytes[32..64].try_into().expect("page-ref digest"),
        })
    }

    fn read_exact_end(reader: &mut BufReader<File>, label: &str) -> CandidateResult<()> {
        let mut trailing = [0u8; 1];
        match reader.read(&mut trailing) {
            Ok(0) => Ok(()),
            Ok(_) => Err(format!("{label} trailing bytes")),
            Err(error) => Err(error.to_string()),
        }
    }

    fn build_staged_tree<W: Write>(
        sink: &mut StreamingSink<'_, W>,
        directory: &Path,
        locator_stage: FixedStage,
        limits: ImmutableLimits,
    ) -> CandidateResult<(PageRef, usize, usize, usize, u64)> {
        let locator_stage_bytes = locator_stage.bytes()?;
        let mut locator_reader = locator_stage.reader()?;
        let mut leaf_stage = FixedStage::create(directory, "leaf-refs", PAGE_REF_STAGE_BYTES)?;
        let mut leaf_writer = leaf_stage.writer()?;
        let mut pages = 0usize;
        let mut peak_locator_entries = 0usize;
        let mut raw_locator = [0u8; LOCATOR_STAGE_BYTES];
        for size in groups(locator_stage.records, LEAF_CAPACITY, LEAF_MIN_OCCUPANCY)? {
            peak_locator_entries = peak_locator_entries.max(size);
            let mut locators = Vec::with_capacity(size);
            for _ in 0..size {
                locator_reader
                    .read_exact(&mut raw_locator)
                    .map_err(|error| error.to_string())?;
                locators.push(decode_locator(&raw_locator)?);
            }
            let reference = sink
                .write_page(&encode_leaf(&locators).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
            leaf_writer
                .write_all(&encode_page_ref(&reference))
                .map_err(|error| error.to_string())?;
            leaf_stage.note_record()?;
            pages += 1;
        }
        read_exact_end(&mut locator_reader, "locator stage")?;
        leaf_writer.flush().map_err(|error| error.to_string())?;
        drop(leaf_writer);
        drop(locator_reader);
        drop(locator_stage);

        let mut current = leaf_stage;
        let mut peak_page_ref_entries = 1usize;
        let mut peak_page_stage_bytes = current.bytes()?;
        while current.records > 1 {
            let mut reader = current.reader()?;
            let parent_level = decode_first_level(&mut reader, &current)?
                .checked_add(1)
                .ok_or_else(|| "page depth overflow".to_owned())?;
            if parent_level > limits.max_depth {
                return Err("page depth limit".into());
            }
            let mut next = FixedStage::create(directory, "parent-refs", PAGE_REF_STAGE_BYTES)?;
            let mut next_writer = next.writer()?;
            let mut raw = [0u8; PAGE_REF_STAGE_BYTES];
            for size in groups(current.records, INTERNAL_FANOUT, INTERNAL_MIN_OCCUPANCY)? {
                peak_page_ref_entries = peak_page_ref_entries.max(size);
                let mut children = Vec::with_capacity(size);
                for _ in 0..size {
                    reader
                        .read_exact(&mut raw)
                        .map_err(|error| error.to_string())?;
                    let child = decode_page_ref(&raw)?;
                    if child.level + 1 != parent_level {
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
                next_writer
                    .write_all(&encode_page_ref(&reference))
                    .map_err(|error| error.to_string())?;
                next.note_record()?;
                pages += 1;
            }
            read_exact_end(&mut reader, "page-ref stage")?;
            next_writer.flush().map_err(|error| error.to_string())?;
            drop(next_writer);
            peak_page_stage_bytes = peak_page_stage_bytes.max(
                current
                    .bytes()?
                    .checked_add(next.bytes()?)
                    .ok_or_else(|| "page-ref live-byte overflow".to_owned())?,
            );
            drop(reader);
            drop(current);
            current = next;
        }

        let mut reader = current.reader()?;
        let mut raw = [0u8; PAGE_REF_STAGE_BYTES];
        reader
            .read_exact(&mut raw)
            .map_err(|error| error.to_string())?;
        let root = decode_page_ref(&raw)?;
        read_exact_end(&mut reader, "root page-ref stage")?;
        drop(reader);
        drop(current);
        Ok((
            root,
            pages,
            peak_locator_entries,
            peak_page_ref_entries,
            locator_stage_bytes
                .checked_add(peak_page_stage_bytes)
                .ok_or_else(|| "private stage byte overflow".to_owned())?,
        ))
    }

    fn decode_first_level(
        reader: &mut BufReader<File>,
        stage: &FixedStage,
    ) -> CandidateResult<u8> {
        let mut raw = [0u8; PAGE_REF_STAGE_BYTES];
        reader
            .read_exact(&mut raw)
            .map_err(|error| error.to_string())?;
        let level = decode_page_ref(&raw)?.level;
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        if stage.records == 0 {
            return Err("empty page-ref stage".into());
        }
        Ok(level)
    }

    #[derive(Clone, Debug)]
    struct TinySource {
        object_id: u64,
    }

    impl ImmutableStreamingPayloadSource for TinySource {
        fn object_id(&self) -> u64 {
            self.object_id
        }
        fn kind(&self) -> u16 {
            1
        }
        fn logical_len(&self) -> u64 {
            1
        }
        fn strong_version(&mut self) -> Result<[u8; 32], &'static str> {
            Ok([u8::try_from(self.object_id % 251).expect("version"); 32])
        }
        fn read_exact_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), &'static str> {
            if offset != 0 || buffer.len() != 1 {
                return Err("tiny source range");
            }
            buffer[0] = u8::try_from(self.object_id % 251).expect("payload");
            Ok(())
        }
    }

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ucof-page-ref-stage-test-{}-{sequence}",
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
    fn staged_locator_and_page_refs_match_current_writer_with_constant_tree_buffers() {
        const OBJECTS: u64 = 74_003;
        let limits = ImmutableLimits::default();
        let options = ImmutableSourceStreamingWriteOptions {
            output: ImmutableStreamingWriteOptions {
                max_write_request_bytes: 4096,
            },
            max_source_read_bytes: 1,
        };
        let original: Vec<_> = (1..=OBJECTS)
            .rev()
            .map(|object_id| TinySource { object_id })
            .collect();

        let mut baseline_sources = original.clone();
        let mut baseline = Vec::new();
        let baseline_report = write_genesis_sources_to(
            &mut baseline,
            &mut baseline_sources,
            options,
            limits,
        )
        .expect("baseline writer");
        assert_eq!(baseline_report.output.report.root_level, 2);
        assert_eq!(baseline_report.output.report.page_count, 404);

        let directory = TestDirectory::new();
        let mut sources = original;
        let preflight = preflight_source_streaming(&mut sources, options, limits)
            .expect("source preflight");
        let mut output = Vec::new();
        let staged_report;
        let peak_locator_entries;
        let peak_page_ref_entries;
        let peak_private_stage_bytes;
        {
            let mut sink = StreamingSink::new(&mut output, options.output.max_write_request_bytes)
                .expect("streaming sink");
            let mut header = [0u8; FILE_HEADER_LEN];
            header[..8].copy_from_slice(FILE_MAGIC);
            sink.write_commit_bytes(&header).expect("write header");
            let mut locator_stage =
                FixedStage::create(&directory.0, "locators", LOCATOR_STAGE_BYTES)
                    .expect("locator stage");
            let mut locator_writer = locator_stage.writer().expect("locator writer");
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
                locator_writer
                    .write_all(&encode_locator(&locator))
                    .expect("write locator stage");
                locator_stage.note_record().expect("locator count");
            }
            locator_writer.flush().expect("flush locators");
            drop(locator_writer);
            assert_eq!(locator_stage.records, usize::try_from(OBJECTS).expect("object count"));

            let (root, page_count, locator_peak, page_ref_peak, private_stage_peak) =
                build_staged_tree(&mut sink, &directory.0, locator_stage, limits)
                    .expect("build staged tree");
            peak_locator_entries = locator_peak;
            peak_page_ref_entries = page_ref_peak;
            peak_private_stage_bytes = private_stage_peak;
            assert_eq!(page_count, preflight.expected_pages);
            assert_eq!(root.level, preflight.expected_root_level);
            let mut report = write_streaming_publication(&mut sink, &root, page_count)
                .expect("write publication");
            report.object_count = usize::try_from(OBJECTS).expect("object count");
            assert_eq!(sink.offset, preflight.expected_bytes);
            staged_report = ImmutableSourceStreamingWriteReport {
                output: ImmutableStreamingWriteReport {
                    report,
                    bytes_written: sink.offset,
                    largest_write_request: sink.largest_write_request,
                    locator_entries: usize::try_from(OBJECTS).expect("object count"),
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
        assert_eq!(peak_page_ref_entries, INTERNAL_FANOUT);
        let locator_stage_bytes = OBJECTS
            * u64::try_from(LOCATOR_STAGE_BYTES).expect("locator width");
        assert!(peak_private_stage_bytes > locator_stage_bytes);
        assert!(fs::read_dir(&directory.0).unwrap().next().is_none());
    }
}
