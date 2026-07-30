#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, scan_source_recovery, validate_source_at,
    validate_source_history, ImmutableObjectInput, ImmutableReadAt, ImmutableSourceError,
    ImmutableSourceLimits,
};

struct SliceSource<'a> {
    data: &'a [u8],
}

impl ImmutableReadAt for SliceSource<'_> {
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
        Ok(())
    }
}

fn limits() -> ImmutableSourceLimits {
    ImmutableSourceLimits {
        max_total_bytes_read: 8 << 20,
        max_read_operations: 32_768,
        max_read_request_bytes: 4096,
        hash_block_bytes: 4096,
        format: ucof_experiments::immutable_successor::ImmutableLimits {
            max_file_bytes: 2 << 20,
            max_objects: 64,
            max_pages: 128,
            max_depth: 4,
            max_allocation_bytes: 2 << 20,
            max_output_bytes: 2 << 20,
            max_history_entries: 8,
            max_recovery_scan_bytes: 2 << 20,
            max_recovery_attempts: 512,
            max_recovery_candidates: 16,
        },
    }
}

fuzz_target!(|data: &[u8]| {
    let source_limits = limits();

    let mut raw_source = SliceSource { data };
    let _ = validate_source_at(&mut raw_source, source_limits);
    let mut raw_history = SliceSource { data };
    let _ = validate_source_history(&mut raw_history, source_limits);
    let mut raw_recovery = SliceSource { data };
    let _ = scan_source_recovery(&mut raw_recovery, source_limits);

    let desired = data.first().map_or(1_usize, |byte| 1 + usize::from(*byte % 8));
    let source = data.get(1..).unwrap_or_default();
    let mut objects = Vec::with_capacity(desired);
    for index in 0..desired {
        let start = source.len().saturating_mul(index) / desired;
        let end = source.len().saturating_mul(index + 1) / desired;
        let payload = source.get(start..end).unwrap_or_default().to_vec();
        objects.push(ImmutableObjectInput::new(
            u64::try_from(index + 1).expect("small identifier"),
            u16::try_from(index % 31 + 1).expect("small kind"),
            payload,
        ));
    }

    let genesis = build_genesis(&objects, source_limits.format).expect("bounded genesis");
    let selected = data.first().map_or(0_usize, |byte| usize::from(*byte) % objects.len());
    let mut payload = objects[selected].payload.clone();
    payload.reverse();
    payload.extend_from_slice(b":source-history");
    let replacement = ImmutableObjectInput::new(
        objects[selected].object_id,
        objects[selected].kind,
        payload,
    );
    let appended = append_replacement(&genesis, &replacement, source_limits.format)
        .expect("bounded append");

    let mut strict_source = SliceSource { data: &appended };
    let strict = validate_source_at(&mut strict_source, source_limits)
        .expect("generated source validates");
    assert_eq!(strict.report.sequence, 1);
    assert_eq!(strict.report.object_count, objects.len());

    let mut history_source = SliceSource { data: &appended };
    let history = validate_source_history(&mut history_source, source_limits)
        .expect("generated history validates");
    let sequences: Vec<_> = history
        .history
        .entries
        .iter()
        .map(|entry| entry.report.sequence)
        .collect();
    assert_eq!(sequences, vec![1, 0]);

    let mut interrupted = appended;
    interrupted.extend_from_slice(data.get(..64).unwrap_or(data));
    let mut recovery_source = SliceSource { data: &interrupted };
    let recovery = scan_source_recovery(&mut recovery_source, source_limits)
        .expect("generated recovery remains bounded");
    assert!(recovery
        .recovery
        .candidates
        .iter()
        .any(|candidate| candidate.report.sequence == 1));
    assert!(recovery
        .recovery
        .candidates
        .iter()
        .any(|candidate| candidate.report.sequence == 0));
});
