#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    append_versioned_source_with_tail_to, ImmutableLimits, ImmutableReadAt, ImmutableSourceError,
    ImmutableSourceLimits, PersistentSourceCopyOptions, PersistentSourceCopyPhase,
    PersistentSourceIdentity, PersistentSourceVersion, PersistentVersionedReadAt,
    PersistentVersionedSourceCopyError,
};

struct VersionedSource {
    bytes: Vec<u8>,
    version: PersistentSourceVersion,
    reads: usize,
    mutate_after_read: Option<usize>,
}

impl ImmutableReadAt for VersionedSource {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        u64::try_from(self.bytes.len()).map_err(|_| ImmutableSourceError::Limit("length"))
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
        buffer.copy_from_slice(
            self.bytes
                .get(start..end)
                .ok_or(ImmutableSourceError::Io("range"))?,
        );
        self.reads += 1;
        if self.mutate_after_read == Some(self.reads) {
            self.version.0[0] ^= 1;
        }
        Ok(())
    }
}

impl PersistentVersionedReadAt for VersionedSource {
    fn version_token(&mut self) -> Result<PersistentSourceVersion, ImmutableSourceError> {
        Ok(self.version)
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
    let chunk = 1 + data.get(1).map_or(31_usize, |byte| usize::from(*byte));
    let write_chunk = 1 + data.get(2).map_or(29_usize, |byte| usize::from(*byte));
    let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
    let limits = ImmutableSourceLimits {
        format: ImmutableLimits {
            max_file_bytes: 1024 * 1024,
            max_output_bytes: 1024 * 1024,
            max_allocation_bytes: 1024 * 1024,
            ..ImmutableLimits::default()
        },
        max_read_request_bytes: chunk,
        max_total_bytes_read: identity.length * 2,
        max_read_operations: 2_000_000,
        ..ImmutableSourceLimits::default()
    };
    let options = PersistentSourceCopyOptions {
        max_write_request_bytes: write_chunk,
    };

    let version = PersistentSourceVersion([9; 32]);
    let mut stable = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: None,
    };
    let mut output = Vec::new();
    let report = append_versioned_source_with_tail_to(
        &mut stable,
        &mut output,
        identity,
        tail,
        limits,
        options,
    )
    .expect("stable version");
    let mut expected = base.clone();
    expected.extend_from_slice(tail);
    assert_eq!(output, expected);
    assert_eq!(report.version, version);
    assert!(report.version_checks > 0);

    let first_pass_reads = base.len().div_ceil(chunk);
    let mut changed = VersionedSource {
        bytes: base.clone(),
        version,
        reads: 0,
        mutate_after_read: Some(first_pass_reads + 1),
    };
    let mut rejected = Vec::new();
    let error = append_versioned_source_with_tail_to(
        &mut changed,
        &mut rejected,
        identity,
        tail,
        limits,
        options,
    )
    .expect_err("changed version");
    assert_eq!(
        error,
        PersistentVersionedSourceCopyError::VersionChanged(PersistentSourceCopyPhase::Preflight)
    );
    assert!(rejected.is_empty());

    if first_pass_reads > 1 {
        let mut changed = VersionedSource {
            bytes: base.clone(),
            version,
            reads: 0,
            mutate_after_read: Some(first_pass_reads + 2),
        };
        let mut partial = Vec::new();
        let error = append_versioned_source_with_tail_to(
            &mut changed,
            &mut partial,
            identity,
            tail,
            limits,
            options,
        )
        .expect_err("copy version change");
        assert_eq!(
            error,
            PersistentVersionedSourceCopyError::VersionChanged(PersistentSourceCopyPhase::Copy)
        );
        assert!(!partial.is_empty());
        assert!(partial.len() < base.len());
        assert_eq!(partial, base[..partial.len()]);
    }
});
