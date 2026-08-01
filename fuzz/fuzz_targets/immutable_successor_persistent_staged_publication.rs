#![no_main]

use libfuzzer_sys::fuzz_target;
use sha2::{Digest, Sha256};
use ucof_experiments::immutable_successor::{
    stage_and_publish_versioned_source_with_tail, ImmutableLimits, ImmutableReadAt,
    ImmutableSourceError, ImmutableSourceLimits, PersistentPublicationLinkOutcome,
    PersistentPublicationStage, PersistentSourceCopyOptions, PersistentSourceIdentity,
    PersistentSourceVersion, PersistentStagedPublicationOutcome, PersistentStagingBackend,
    PersistentVersionedReadAt,
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

struct Backend {
    private: Vec<u8>,
    destination: Option<Vec<u8>>,
    begun: bool,
    expected_length: u64,
    link: PersistentPublicationLinkOutcome,
    indeterminate_publishes: bool,
    fail: Option<PersistentPublicationStage>,
    fail_abort: bool,
}

impl Backend {
    fn step(&self, stage: PersistentPublicationStage) -> Result<(), &'static str> {
        if self.fail == Some(stage) {
            Err("injected")
        } else {
            Ok(())
        }
    }
}

impl std::io::Write for Backend {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if !self.begun {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "not begun",
            ));
        }
        self.private.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl PersistentStagingBackend for Backend {
    fn begin_private(&mut self, expected_length: u64) -> Result<(), &'static str> {
        self.step(PersistentPublicationStage::BeginPrivate)?;
        self.begun = true;
        self.expected_length = expected_length;
        Ok(())
    }

    fn validate_private(
        &mut self,
        expected_length: u64,
        expected_sha256: [u8; 32],
    ) -> Result<(), &'static str> {
        self.step(PersistentPublicationStage::ValidatePrivate)?;
        if expected_length != self.expected_length
            || u64::try_from(self.private.len()).map_err(|_| "length")? != expected_length
            || <[u8; 32]>::from(Sha256::digest(&self.private)) != expected_sha256
        {
            return Err("validation");
        }
        Ok(())
    }

    fn sync_private(&mut self) -> Result<(), &'static str> {
        self.step(PersistentPublicationStage::SyncPrivate)
    }

    fn publish_no_replace(&mut self) -> Result<PersistentPublicationLinkOutcome, &'static str> {
        self.step(PersistentPublicationStage::PublishLink)?;
        match self.link {
            PersistentPublicationLinkOutcome::Linked => {
                if self.destination.is_some() {
                    return Ok(PersistentPublicationLinkOutcome::DestinationExists);
                }
                self.destination = Some(self.private.clone());
            }
            PersistentPublicationLinkOutcome::Indeterminate if self.indeterminate_publishes => {
                self.destination = Some(self.private.clone());
            }
            _ => {}
        }
        Ok(self.link)
    }

    fn sync_parent(&mut self) -> Result<(), &'static str> {
        self.step(PersistentPublicationStage::SyncParent)
    }

    fn retire_private(&mut self) -> Result<(), &'static str> {
        self.step(PersistentPublicationStage::RetirePrivate)?;
        self.private.clear();
        self.begun = false;
        Ok(())
    }

    fn abort_private(&mut self) -> Result<(), &'static str> {
        if self.fail_abort {
            return Err("abort injected");
        }
        self.private.clear();
        self.begun = false;
        Ok(())
    }
}

fn stage(byte: u8) -> Option<PersistentPublicationStage> {
    match byte % 8 {
        0 => None,
        1 => Some(PersistentPublicationStage::BeginPrivate),
        2 => Some(PersistentPublicationStage::ValidatePrivate),
        3 => Some(PersistentPublicationStage::SyncPrivate),
        4 => Some(PersistentPublicationStage::PublishLink),
        5 => Some(PersistentPublicationStage::SyncParent),
        6 => Some(PersistentPublicationStage::RetirePrivate),
        _ => Some(PersistentPublicationStage::AbortPrivate),
    }
}

fuzz_target!(|data: &[u8]| {
    let control = data.first().copied().unwrap_or(0);
    let split = data
        .get(1)
        .map_or(0_usize, |byte| usize::from(*byte) % (data.len() + 1));
    let mut base = data[..split].to_vec();
    if base.is_empty() {
        base.push(0);
    }
    let tail = &data[split..];
    let chunk = 1 + data.get(2).map_or(31_usize, |byte| usize::from(*byte));
    let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
    let first_pass_reads = base.len().div_ceil(chunk);
    let mutate_after_read = match control % 4 {
        1 => Some(1),
        2 if first_pass_reads > 1 => Some(first_pass_reads + 2),
        _ => None,
    };
    let mut source = VersionedSource {
        bytes: base.clone(),
        version: PersistentSourceVersion([13; 32]),
        reads: 0,
        mutate_after_read,
    };
    let link = match data.get(3).copied().unwrap_or(0) % 3 {
        0 => PersistentPublicationLinkOutcome::Linked,
        1 => PersistentPublicationLinkOutcome::DestinationExists,
        _ => PersistentPublicationLinkOutcome::Indeterminate,
    };
    let sentinel = vec![0xA5, 0x5A];
    let mut backend = Backend {
        private: Vec::new(),
        destination: (link == PersistentPublicationLinkOutcome::DestinationExists)
            .then(|| sentinel.clone()),
        begun: false,
        expected_length: 0,
        link,
        indeterminate_publishes: control & 0x10 != 0,
        fail: stage(data.get(4).copied().unwrap_or(0)),
        fail_abort: control & 0x20 != 0,
    };
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
    let result = stage_and_publish_versioned_source_with_tail(
        &mut source,
        &mut backend,
        identity,
        tail,
        limits,
        PersistentSourceCopyOptions {
            max_write_request_bytes: 1 + data.get(5).map_or(29_usize, |byte| usize::from(*byte)),
        },
    );
    let mut expected = base;
    expected.extend_from_slice(tail);

    if let Some(destination) = &backend.destination {
        assert!(destination == &expected || destination == &sentinel);
    }
    if link == PersistentPublicationLinkOutcome::DestinationExists {
        assert_eq!(backend.destination, Some(sentinel.clone()));
    }

    if let Ok(report) = result {
        match report.outcome {
            PersistentStagedPublicationOutcome::PublishedAndDurable { cleanup_pending } => {
                assert_eq!(backend.destination, Some(expected));
                assert_eq!(cleanup_pending, report.cleanup_error.is_some());
                if !cleanup_pending {
                    assert!(backend.private.is_empty());
                }
            }
            PersistentStagedPublicationOutcome::NotPublishedDestinationExists => {
                assert_eq!(backend.destination, Some(sentinel.clone()));
                if report.cleanup_error.is_none() {
                    assert!(backend.private.is_empty());
                }
            }
            PersistentStagedPublicationOutcome::PublicationIndeterminate { .. } => {
                assert!(!backend.private.is_empty());
            }
        }
    } else if backend.fail != Some(PersistentPublicationStage::SyncParent)
        && backend.fail != Some(PersistentPublicationStage::RetirePrivate)
    {
        assert!(backend.destination.is_none() || backend.destination == Some(sentinel));
    }
});
