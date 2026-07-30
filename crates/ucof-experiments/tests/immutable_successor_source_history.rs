use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, scan_source_recovery, validate_source_at,
    validate_source_history, ImmutableError, ImmutableObjectInput, ImmutableReadAt,
    ImmutableSourceError, ImmutableSourceLimits, OBJECT_HEADER_LEN,
};

#[derive(Debug)]
struct RecordingSource {
    data: Vec<u8>,
    ranges: Vec<(u64, usize)>,
}

impl RecordingSource {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            ranges: Vec::new(),
        }
    }
}

impl ImmutableReadAt for RecordingSource {
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
        let source = self
            .data
            .get(start..end)
            .ok_or(ImmutableSourceError::Io("range"))?;
        buffer.copy_from_slice(source);
        self.ranges.push((offset, buffer.len()));
        Ok(())
    }
}

fn base_objects() -> Vec<ImmutableObjectInput> {
    vec![
        ImmutableObjectInput::new(1, 1, b"alpha".to_vec()),
        ImmutableObjectInput::new(2, 2, b"bravo".to_vec()),
        ImmutableObjectInput::new(3, 3, b"charlie".to_vec()),
        ImmutableObjectInput::new(4, 1, b"delta".to_vec()),
    ]
}

fn two_commit_file() -> (Vec<u8>, usize) {
    let genesis =
        build_genesis(&base_objects(), ImmutableSourceLimits::default().format).expect("genesis");
    let genesis_len = genesis.len();
    let appended = append_replacement(
        &genesis,
        &ImmutableObjectInput::new(1, 9, b"alpha-v2".to_vec()),
        ImmutableSourceLimits::default().format,
    )
    .expect("append");
    (appended, genesis_len)
}

#[test]
fn validates_every_active_page_and_object_from_a_source() {
    let objects: Vec<_> = (1_u64..=400)
        .map(|object_id| {
            ImmutableObjectInput::new(
                object_id,
                u16::try_from(1 + object_id % 5).expect("kind"),
                format!("payload:{object_id}").into_bytes(),
            )
        })
        .collect();
    let bytes = build_genesis(&objects, ImmutableSourceLimits::default().format)
        .expect("multi-level genesis");
    let limits = ImmutableSourceLimits {
        max_read_request_bytes: 4096,
        hash_block_bytes: 4096,
        ..ImmutableSourceLimits::default()
    };
    let mut source = RecordingSource::new(bytes);
    let report = validate_source_at(&mut source, limits).expect("strict source validation");
    assert_eq!(report.report.sequence, 0);
    assert_eq!(report.report.object_count, 400);
    assert_eq!(report.report.page_count, 4);
    assert_eq!(report.report.root_level, 1);
    assert!(source.ranges.iter().all(|(_, length)| *length <= 4096));
    assert!(report.stats.bytes_hashed > 0);
    assert!(report.stats.largest_allocation <= 16 * 1024);
}

#[test]
fn linked_source_history_revalidates_newest_to_oldest() {
    let (bytes, _) = two_commit_file();
    let mut source = RecordingSource::new(bytes);
    let report = validate_source_history(&mut source, ImmutableSourceLimits::default())
        .expect("source history");
    let sequences: Vec<_> = report
        .history
        .entries
        .iter()
        .map(|entry| entry.report.sequence)
        .collect();
    assert_eq!(sequences, vec![1, 0]);
    assert!(report.stats.read_operations > 0);
    assert!(report.stats.bytes_read > 0);
}

#[test]
fn active_source_can_validate_while_history_rejects_a_corrupt_replaced_object() {
    let (mut bytes, _) = two_commit_file();
    bytes[64 + OBJECT_HEADER_LEN] ^= 0x01;

    let mut active_source = RecordingSource::new(bytes.clone());
    let active = validate_source_at(&mut active_source, ImmutableSourceLimits::default())
        .expect("active state no longer references replaced object");
    assert_eq!(active.report.sequence, 1);

    let mut history_source = RecordingSource::new(bytes);
    assert_eq!(
        validate_source_history(&mut history_source, ImmutableSourceLimits::default()),
        Err(ImmutableSourceError::Format(ImmutableError::Invalid(
            "object digest"
        )))
    );
}

#[test]
fn recovery_reports_strict_prefixes_without_selecting_one() {
    let (mut bytes, genesis_len) = two_commit_file();
    bytes.extend_from_slice(b"interrupted-next-publication");
    let mut source = RecordingSource::new(bytes);
    let report = scan_source_recovery(&mut source, ImmutableSourceLimits::default())
        .expect("bounded recovery");
    let sequences: Vec<_> = report
        .recovery
        .candidates
        .iter()
        .map(|candidate| candidate.report.sequence)
        .collect();
    assert_eq!(sequences, vec![1, 0]);
    assert_eq!(
        report
            .recovery
            .candidates
            .last()
            .expect("genesis")
            .prefix_len,
        u64::try_from(genesis_len).expect("genesis length")
    );
    assert!(!report.recovery.attempts_truncated);
    assert!(!report.recovery.candidates_truncated);
}

#[test]
fn recovery_candidate_cap_is_explicit() {
    let (bytes, _) = two_commit_file();
    let mut source = RecordingSource::new(bytes);
    let mut limits = ImmutableSourceLimits::default();
    limits.format.max_recovery_candidates = 1;
    let report = scan_source_recovery(&mut source, limits).expect("bounded recovery");
    assert_eq!(report.recovery.candidates.len(), 1);
    assert!(report.recovery.candidates_truncated);
}

#[test]
fn cumulative_source_budget_fails_closed() {
    let (bytes, _) = two_commit_file();
    let limits = ImmutableSourceLimits {
        max_total_bytes_read: 1024,
        max_read_operations: 1024,
        max_read_request_bytes: 128,
        hash_block_bytes: 128,
        ..ImmutableSourceLimits::default()
    };
    let mut source = RecordingSource::new(bytes);
    assert_eq!(
        validate_source_history(&mut source, limits),
        Err(ImmutableSourceError::Limit("read bytes"))
    );
    assert!(source.ranges.iter().all(|(_, length)| *length <= 128));
}
