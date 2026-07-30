use std::io::Cursor;

use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, lookup_at, ImmutableLookupResult, ImmutableObjectInput,
    ImmutableReadAt, ImmutableSeekSource, ImmutableSliceSource, ImmutableSourceError,
    ImmutableSourceLimits, OBJECT_HEADER_LEN,
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

    fn intersects(&self, start: usize, end: usize) -> bool {
        self.ranges.iter().any(|(offset, length)| {
            let read_start = usize::try_from(*offset).expect("recorded offset");
            let read_end = read_start + length;
            read_start < end && start < read_end
        })
    }

    fn total_bytes_read(&self) -> usize {
        self.ranges.iter().map(|(_, length)| *length).sum()
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

fn small_genesis() -> Vec<u8> {
    build_genesis(
        &[
            ImmutableObjectInput::new(1, 1, b"alpha".to_vec()),
            ImmutableObjectInput::new(2, 2, b"bravo".to_vec()),
            ImmutableObjectInput::new(3, 3, b"charlie".to_vec()),
            ImmutableObjectInput::new(4, 1, b"delta".to_vec()),
        ],
        ImmutableSourceLimits::default().format,
    )
    .expect("genesis")
}

#[test]
fn slice_and_seek_sources_return_equivalent_found_and_absent_evidence() {
    let bytes = small_genesis();
    let mut slice = ImmutableSliceSource::new(&bytes);
    let found = lookup_at(&mut slice, 2, ImmutableSourceLimits::default()).expect("slice lookup");
    assert!(matches!(
        found.result,
        ImmutableLookupResult::Found {
            object_id: 2,
            kind: 2,
            logical_len: 5,
            ..
        }
    ));
    assert_eq!(found.sequence, 0);

    let mut seek = ImmutableSeekSource::new(Cursor::new(bytes.clone()));
    let equivalent =
        lookup_at(&mut seek, 2, ImmutableSourceLimits::default()).expect("seek lookup");
    assert_eq!(found, equivalent);

    let mut absent_source = ImmutableSliceSource::new(&bytes);
    let absent = lookup_at(&mut absent_source, 99, ImmutableSourceLimits::default())
        .expect("absence lookup");
    assert_eq!(
        absent.result,
        ImmutableLookupResult::Absent { object_id: 99 }
    );
}

#[test]
fn lookup_skips_unrelated_large_historical_payload() {
    let large = vec![0x5a; 1024 * 1024];
    let genesis = build_genesis(
        &[
            ImmutableObjectInput::new(1, 1, large.clone()),
            ImmutableObjectInput::new(2, 2, b"small".to_vec()),
        ],
        ImmutableSourceLimits::default().format,
    )
    .expect("large genesis");
    let appended = append_replacement(
        &genesis,
        &ImmutableObjectInput::new(2, 9, b"small-v2".to_vec()),
        ImmutableSourceLimits::default().format,
    )
    .expect("append");

    let large_payload_start = 64 + OBJECT_HEADER_LEN;
    let large_payload_end = large_payload_start + large.len();
    let limits = ImmutableSourceLimits {
        max_read_request_bytes: 4 * 1024,
        hash_block_bytes: 4 * 1024,
        ..ImmutableSourceLimits::default()
    };
    let mut source = RecordingSource::new(appended);
    let report = lookup_at(&mut source, 2, limits).expect("targeted lookup");
    assert!(matches!(
        report.result,
        ImmutableLookupResult::Found {
            object_id: 2,
            kind: 9,
            ..
        }
    ));
    assert!(!source.intersects(large_payload_start, large_payload_end));
    assert!(source.ranges.iter().all(|(_, length)| *length <= 4 * 1024));
    assert!(report.stats.bytes_read < 128 * 1024);
}

#[test]
fn targeted_lookup_does_not_upgrade_unrelated_historical_damage() {
    let large = vec![0x33; 64 * 1024];
    let genesis = build_genesis(
        &[
            ImmutableObjectInput::new(1, 1, large),
            ImmutableObjectInput::new(2, 2, b"small".to_vec()),
        ],
        ImmutableSourceLimits::default().format,
    )
    .expect("genesis");
    let mut appended = append_replacement(
        &genesis,
        &ImmutableObjectInput::new(2, 9, b"small-v2".to_vec()),
        ImmutableSourceLimits::default().format,
    )
    .expect("append");
    appended[64 + OBJECT_HEADER_LEN] ^= 0x01;

    let mut selected_source = ImmutableSliceSource::new(&appended);
    let selected = lookup_at(&mut selected_source, 2, ImmutableSourceLimits::default())
        .expect("selected object remains path-authenticated");
    assert!(matches!(
        selected.result,
        ImmutableLookupResult::Found { object_id: 2, .. }
    ));

    let mut damaged_source = ImmutableSliceSource::new(&appended);
    assert_eq!(
        lookup_at(&mut damaged_source, 1, ImmutableSourceLimits::default()),
        Err(ImmutableSourceError::Format(
            ucof_experiments::immutable_successor::ImmutableError::Invalid("object digest")
        ))
    );
}

#[test]
fn source_budgets_fail_before_excess_reads() {
    let bytes = small_genesis();
    let limits = ImmutableSourceLimits {
        max_total_bytes_read: 100,
        max_read_operations: 100,
        max_read_request_bytes: 32,
        hash_block_bytes: 32,
        ..ImmutableSourceLimits::default()
    };
    let mut source = RecordingSource::new(bytes);
    assert_eq!(
        lookup_at(&mut source, 1, limits),
        Err(ImmutableSourceError::Limit("read bytes"))
    );
    assert!(source.total_bytes_read() <= 100);
    assert!(source.ranges.iter().all(|(_, length)| *length <= 32));
}
