#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_verified_source_with_tail_to, ImmutableLimits, ImmutableReadAt, ImmutableSourceError,
    ImmutableSourceLimits, PersistentSourceCopyError, PersistentSourceCopyOptions,
    PersistentSourceCopyPhase, PersistentSourceIdentity,
};

#[derive(Debug)]
struct FuzzSource {
    bytes: Vec<u8>,
    reads: usize,
    mutate_at_read: Option<usize>,
}

impl ImmutableReadAt for FuzzSource {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        u64::try_from(self.bytes.len()).map_err(|_| ImmutableSourceError::Limit("length"))
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        self.reads += 1;
        if self.mutate_at_read == Some(self.reads) && !self.bytes.is_empty() {
            self.bytes[0] ^= 0x80;
        }
        let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
        let end = start
            .checked_add(buffer.len())
            .ok_or(ImmutableSourceError::Io("range"))?;
        let source = self
            .bytes
            .get(start..end)
            .ok_or(ImmutableSourceError::Io("range"))?;
        buffer.copy_from_slice(source);
        Ok(())
    }
}

fuzz_target!(|data: &[u8]| {
    let split = data
        .first()
        .map_or(0_usize, |byte| usize::from(*byte) % (data.len() + 1));
    let mut base = data[..split].to_vec();
    if base.is_empty() {
        base.push(0);
    }
    let tail = &data[split..];
    let read_chunk = 1 + data.get(1).map_or(31_usize, |byte| usize::from(*byte));
    let write_chunk = 1 + data.get(2).map_or(29_usize, |byte| usize::from(*byte));
    let identity = PersistentSourceIdentity::from_bytes(&base).expect("bounded identity");
    let format = ImmutableLimits {
        max_file_bytes: 1024 * 1024,
        max_output_bytes: 1024 * 1024,
        max_allocation_bytes: 1024 * 1024,
        ..ImmutableLimits::default()
    };
    let source_limits = ImmutableSourceLimits {
        format,
        max_read_request_bytes: read_chunk,
        max_total_bytes_read: identity.length * 2,
        max_read_operations: 2_000_000,
        ..ImmutableSourceLimits::default()
    };
    let options = PersistentSourceCopyOptions {
        max_write_request_bytes: write_chunk,
    };

    let mut stable = FuzzSource {
        bytes: base.clone(),
        reads: 0,
        mutate_at_read: None,
    };
    let mut output = Vec::new();
    let report = append_verified_source_with_tail_to(
        &mut stable,
        &mut output,
        identity,
        tail,
        source_limits,
        options,
    )
    .expect("stable source copy");
    let mut expected = base.clone();
    expected.extend_from_slice(tail);
    assert_eq!(output, expected);
    assert_eq!(report.bytes_read, identity.length * 2);
    assert!(report.largest_read_request <= read_chunk);
    assert!(report.largest_write_request <= write_chunk);

    let first_pass_reads = base.len().div_ceil(read_chunk);
    let mut changed = FuzzSource {
        bytes: base.clone(),
        reads: 0,
        mutate_at_read: Some(first_pass_reads + 1),
    };
    let mut rejected = Vec::new();
    let error = append_verified_source_with_tail_to(
        &mut changed,
        &mut rejected,
        identity,
        tail,
        source_limits,
        options,
    )
    .expect_err("second-pass mutation");
    assert_eq!(
        error,
        PersistentSourceCopyError::IdentityMismatch(PersistentSourceCopyPhase::Copy)
    );
    assert_eq!(rejected.len(), base.len());

    let mut wrong = identity;
    wrong.sha256[0] ^= 1;
    let mut mismatched = FuzzSource {
        bytes: base,
        reads: 0,
        mutate_at_read: None,
    };
    let mut untouched = Vec::new();
    let error = append_verified_source_with_tail_to(
        &mut mismatched,
        &mut untouched,
        wrong,
        tail,
        source_limits,
        options,
    )
    .expect_err("preflight mismatch");
    assert_eq!(
        error,
        PersistentSourceCopyError::IdentityMismatch(PersistentSourceCopyPhase::Preflight)
    );
    assert!(untouched.is_empty());
});
