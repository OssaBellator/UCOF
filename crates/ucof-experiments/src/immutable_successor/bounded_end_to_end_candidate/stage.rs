const DESCRIPTOR_STAGE_BYTES: usize = 64;
const LOCATOR_STAGE_BYTES: usize = 72;
const PAGE_REF_STAGE_BYTES: usize = 64;
static NEXT_BOUNDED_STAGE: AtomicU64 = AtomicU64::new(1);
type CandidateResult<T> = Result<T, String>;

struct FixedStage {
    path: PathBuf,
    file: Option<File>,
    records: usize,
    record_bytes: usize,
}

impl FixedStage {
    fn create(directory: &Path, label: &str, record_bytes: usize) -> CandidateResult<Self> {
        let sequence = NEXT_BOUNDED_STAGE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            ".ucof-bounded-{label}-{}-{sequence}.bin",
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

    fn set_records_u64(&mut self, records: u64) -> CandidateResult<()> {
        self.records = usize::try_from(records).map_err(|_| "stage record count".to_owned())?;
        Ok(())
    }

    fn bytes(&self) -> CandidateResult<u64> {
        u64::try_from(self.records)
            .map_err(|_| "stage count overflow".to_owned())?
            .checked_mul(u64::try_from(self.record_bytes).expect("record width fits u64"))
            .ok_or_else(|| "stage byte overflow".to_owned())
    }

    fn validate_bytes(&self) -> CandidateResult<u64> {
        let expected = self.bytes()?;
        let actual = self
            .file
            .as_ref()
            .ok_or_else(|| "closed stage".to_owned())?
            .metadata()
            .map_err(|error| error.to_string())?
            .len();
        if actual != expected {
            return Err("stage byte length".into());
        }
        Ok(expected)
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
    let minimum = u64::from_le_bytes(bytes[..8].try_into().expect("page-ref field"));
    let maximum = u64::from_le_bytes(bytes[8..16].try_into().expect("page-ref field"));
    if minimum > maximum {
        return Err("page-ref range".into());
    }
    Ok(PageRef {
        minimum,
        maximum,
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

fn decode_first_level(reader: &mut BufReader<File>, stage: &FixedStage) -> CandidateResult<u8> {
    if stage.records == 0 {
        return Err("empty page-ref stage".into());
    }
    let mut raw = [0u8; PAGE_REF_STAGE_BYTES];
    reader
        .read_exact(&mut raw)
        .map_err(|error| error.to_string())?;
    let level = decode_page_ref(&raw)?.level;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    Ok(level)
}

#[derive(Debug)]
struct TreeStageEvidence {
    root: PageRef,
    page_count: usize,
    peak_locator_entries: usize,
    peak_page_ref_entries: usize,
    peak_live_tree_stage_bytes: u64,
}

fn build_staged_tree<W: Write>(
    sink: &mut StreamingSink<'_, W>,
    directory: &Path,
    locator_stage: FixedStage,
    limits: ImmutableLimits,
) -> CandidateResult<TreeStageEvidence> {
    let locator_stage_bytes = locator_stage.validate_bytes()?;
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
        pages = pages
            .checked_add(1)
            .ok_or_else(|| "page count overflow".to_owned())?;
    }
    read_exact_end(&mut locator_reader, "locator stage")?;
    leaf_writer.flush().map_err(|error| error.to_string())?;
    drop(leaf_writer);
    let leaf_stage_bytes = leaf_stage.validate_bytes()?;
    let mut peak_live_tree_stage_bytes = locator_stage_bytes
        .checked_add(leaf_stage_bytes)
        .ok_or_else(|| "tree stage byte overflow".to_owned())?;
    drop(locator_reader);
    drop(locator_stage);

    let mut current = leaf_stage;
    let mut peak_page_ref_entries = 1usize;
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
            next_writer
                .write_all(&encode_page_ref(&reference))
                .map_err(|error| error.to_string())?;
            next.note_record()?;
            pages = pages
                .checked_add(1)
                .ok_or_else(|| "page count overflow".to_owned())?;
        }
        read_exact_end(&mut reader, "page-ref stage")?;
        next_writer.flush().map_err(|error| error.to_string())?;
        drop(next_writer);
        let live_bytes = current
            .validate_bytes()?
            .checked_add(next.validate_bytes()?)
            .ok_or_else(|| "page-ref live-byte overflow".to_owned())?;
        peak_live_tree_stage_bytes = peak_live_tree_stage_bytes.max(live_bytes);
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

    Ok(TreeStageEvidence {
        root,
        page_count: pages,
        peak_locator_entries,
        peak_page_ref_entries,
        peak_live_tree_stage_bytes,
    })
}
