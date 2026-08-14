#[derive(Clone, Debug)]
struct TinySource {
    object_id: u64,
    fail_version: bool,
}

impl TinySource {
    fn new(object_id: u64) -> Self {
        Self {
            object_id,
            fail_version: false,
        }
    }
}

impl ImmutableStreamingPayloadSource for TinySource {
    fn object_id(&self) -> u64 {
        self.object_id
    }

    fn kind(&self) -> u16 {
        u16::try_from(1 + self.object_id % 17).expect("kind")
    }

    fn logical_len(&self) -> u64 {
        1
    }

    fn strong_version(&mut self) -> Result<[u8; 32], &'static str> {
        if self.fail_version {
            Err("metadata version failure")
        } else {
            Ok([u8::try_from(self.object_id % 251).expect("version"); 32])
        }
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
    fn new(label: &str) -> Self {
        let sequence = NEXT_BOUNDED_STAGE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ucof-end-to-end-bounded-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create test directory");
        Self(path)
    }

    fn assert_empty(&self) {
        assert!(fs::read_dir(&self.0).unwrap().next().is_none());
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn spill_limits(run_records: usize, max_open_inputs: usize) -> BoundedSpillSortLimits {
    BoundedSpillSortLimits {
        record_bytes: DESCRIPTOR_STAGE_BYTES,
        run_records,
        max_records: 100_000,
        max_initial_runs: 100_000,
        max_open_inputs,
        max_merge_passes: 32,
        max_live_spill_bytes: 64 * 1024 * 1024,
        max_spill_bytes_written: 512 * 1024 * 1024,
        max_merge_bytes_read: 512 * 1024 * 1024,
        max_merge_bytes_written: 512 * 1024 * 1024,
    }
}

fn options() -> ImmutableSourceStreamingWriteOptions {
    ImmutableSourceStreamingWriteOptions {
        output: ImmutableStreamingWriteOptions {
            max_write_request_bytes: 4096,
        },
        max_source_read_bytes: 1,
    }
}

#[test]
fn end_to_end_bounded_candidate_matches_three_level_canonical_writer() {
    const OBJECTS: u64 = 70_671;
    let limits = ImmutableLimits::default();
    let original: Vec<_> = (1..=OBJECTS).rev().map(TinySource::new).collect();

    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        options(),
        limits,
    )
    .expect("baseline writer");
    assert_eq!(baseline_report.output.report.root_level, 2);
    assert_eq!(baseline_report.output.report.page_count, 386);

    let directory = TestDirectory::new("three-level");
    let mut sources = original;
    let mut actual = Vec::new();
    let evidence = write_genesis_sources_end_to_end_bounded_candidate(
        &mut actual,
        &mut sources,
        &directory.0,
        options(),
        limits,
        spill_limits(257, 8),
    )
    .expect("bounded writer");

    assert_eq!(actual, baseline);
    assert_eq!(evidence.output, baseline_report);
    assert_eq!(evidence.peak_locator_entries, LEAF_CAPACITY);
    assert_eq!(evidence.peak_page_ref_entries, INTERNAL_FANOUT);
    assert_eq!(
        evidence.descriptor_stage_bytes,
        OBJECTS * u64::try_from(DESCRIPTOR_STAGE_BYTES).expect("descriptor width")
    );
    assert_eq!(evidence.descriptor_spill.output_records, OBJECTS);
    assert!(evidence.descriptor_spill.initial_runs > 1);
    assert!(evidence.peak_live_retained_stage_bytes > evidence.descriptor_stage_bytes);
    directory.assert_empty();
}

#[test]
fn fixed_tree_allocation_limit_succeeds_where_workload_wide_writer_rejects() {
    const OBJECTS: u64 = 2_003;
    let original: Vec<_> = (1..=OBJECTS).rev().map(TinySource::new).collect();
    let default_limits = ImmutableLimits::default();

    let mut baseline_sources = original.clone();
    let mut baseline = Vec::new();
    let baseline_report = write_genesis_sources_to(
        &mut baseline,
        &mut baseline_sources,
        options(),
        default_limits,
    )
    .expect("baseline writer");

    let fixed_tree_allocation = (std::mem::size_of::<Locator>() * LEAF_CAPACITY)
        .max(std::mem::size_of::<PageRef>() * INTERNAL_FANOUT);
    assert!(std::mem::size_of::<Locator>() * usize::try_from(OBJECTS).unwrap() > fixed_tree_allocation);
    let mut bounded_limits = default_limits;
    bounded_limits.max_allocation_bytes = fixed_tree_allocation;

    let mut old_sources = original.clone();
    let mut old_output = Vec::new();
    assert!(write_genesis_sources_to(
        &mut old_output,
        &mut old_sources,
        options(),
        bounded_limits,
    )
    .is_err());
    assert!(old_output.is_empty());

    let directory = TestDirectory::new("allocation");
    let mut sources = original;
    let mut actual = Vec::new();
    let evidence = write_genesis_sources_end_to_end_bounded_candidate(
        &mut actual,
        &mut sources,
        &directory.0,
        options(),
        bounded_limits,
        spill_limits(31, 4),
    )
    .expect("bounded allocation writer");

    assert_eq!(actual, baseline);
    assert_eq!(evidence.output, baseline_report);
    assert!(evidence.peak_locator_entries <= LEAF_CAPACITY);
    assert!(evidence.peak_page_ref_entries <= INTERNAL_FANOUT);
    directory.assert_empty();
}

#[test]
fn duplicate_descriptor_across_runs_fails_before_output_and_cleans_stages() {
    let directory = TestDirectory::new("duplicate");
    let mut sources = [TinySource::new(3), TinySource::new(2), TinySource::new(3)];
    let mut output = Vec::new();
    let error = write_genesis_sources_end_to_end_bounded_candidate(
        &mut output,
        &mut sources,
        &directory.0,
        options(),
        ImmutableLimits::default(),
        spill_limits(1, 2),
    )
    .expect_err("duplicate must fail");

    assert!(error.contains("duplicate"));
    assert!(output.is_empty());
    directory.assert_empty();
}

#[test]
fn metadata_failure_after_completed_run_fails_before_output_and_cleans_stages() {
    let directory = TestDirectory::new("metadata");
    let mut sources = [TinySource::new(4), TinySource::new(3), TinySource::new(2)];
    sources[2].fail_version = true;
    let mut output = Vec::new();
    let error = write_genesis_sources_end_to_end_bounded_candidate(
        &mut output,
        &mut sources,
        &directory.0,
        options(),
        ImmutableLimits::default(),
        spill_limits(2, 2),
    )
    .expect_err("metadata failure must fail");

    assert!(error.contains("metadata version failure"));
    assert!(output.is_empty());
    directory.assert_empty();
}

#[test]
fn output_limit_after_descriptor_sort_fails_before_output_and_cleans_stages() {
    let directory = TestDirectory::new("output-limit");
    let mut sources = [TinySource::new(2), TinySource::new(1)];
    let limits = ImmutableLimits {
        max_output_bytes: FILE_HEADER_LEN,
        ..ImmutableLimits::default()
    };
    let mut output = Vec::new();
    let error = write_genesis_sources_end_to_end_bounded_candidate(
        &mut output,
        &mut sources,
        &directory.0,
        options(),
        limits,
        spill_limits(1, 2),
    )
    .expect_err("output limit must fail");

    assert!(error.contains("output limit"));
    assert!(output.is_empty());
    directory.assert_empty();
}
