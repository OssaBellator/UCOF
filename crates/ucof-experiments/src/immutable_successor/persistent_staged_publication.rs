#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersistentPublicationLinkOutcome {
    Linked,
    DestinationExists,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersistentPublicationStage {
    BeginPrivate,
    CopyPrivate,
    ValidatePrivate,
    SyncPrivate,
    PublishLink,
    SyncParent,
    RetirePrivate,
    AbortPrivate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentStagedPublicationOutcome {
    NotPublishedDestinationExists,
    PublishedAndDurable {
        cleanup_pending: bool,
    },
    PublicationIndeterminate {
        stage: PersistentPublicationStage,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentStagedPublicationError {
    Copy {
        error: PersistentVersionedSourceCopyError,
        cleanup_error: Option<&'static str>,
    },
    Backend {
        stage: PersistentPublicationStage,
        error: &'static str,
        cleanup_error: Option<&'static str>,
    },
    Invariant(&'static str),
}

impl std::fmt::Display for PersistentStagedPublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Copy {
                error,
                cleanup_error: None,
            } => write!(formatter, "private publication copy failed: {error}"),
            Self::Copy {
                error,
                cleanup_error: Some(cleanup),
            } => write!(
                formatter,
                "private publication copy failed: {error}; cleanup failed: {cleanup}"
            ),
            Self::Backend {
                stage,
                error,
                cleanup_error: None,
            } => write!(formatter, "private publication {stage:?} failed: {error}"),
            Self::Backend {
                stage,
                error,
                cleanup_error: Some(cleanup),
            } => write!(
                formatter,
                "private publication {stage:?} failed: {error}; cleanup failed: {cleanup}"
            ),
            Self::Invariant(label) => write!(formatter, "private publication invariant failed: {label}"),
        }
    }
}

impl std::error::Error for PersistentStagedPublicationError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentStagedPublicationReport {
    pub source: PersistentVersionedSourceCopyReport,
    pub output_length: u64,
    pub output_sha256: [u8; 32],
    pub outcome: PersistentStagedPublicationOutcome,
    pub cleanup_error: Option<&'static str>,
}

/// Backend contract for private staging and no-overwrite publication.
///
/// Implementations must keep writes private until `publish_no_replace`. `Linked` means the
/// destination name now refers to the staged artifact. `DestinationExists` means no link was
/// created. `Indeterminate` means callers must not assume either state. `sync_parent` must establish
/// destination-name durability for the backend's supported platform.
pub trait PersistentStagingBackend: std::io::Write {
    fn begin_private(&mut self, expected_length: u64) -> Result<(), &'static str>;
    fn validate_private(
        &mut self,
        expected_length: u64,
        expected_sha256: [u8; 32],
    ) -> Result<(), &'static str>;
    fn sync_private(&mut self) -> Result<(), &'static str>;
    fn publish_no_replace(&mut self) -> Result<PersistentPublicationLinkOutcome, &'static str>;
    fn sync_parent(&mut self) -> Result<(), &'static str>;
    fn retire_private(&mut self) -> Result<(), &'static str>;
    fn abort_private(&mut self) -> Result<(), &'static str>;
}

struct PersistentPublicationDigestWriter<'a, B> {
    inner: &'a mut B,
    hasher: Sha256,
    bytes_written: u64,
}

impl<B: std::io::Write> std::io::Write for PersistentPublicationDigestWriter<'_, B> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.hasher.update(&buffer[..written]);
        self.bytes_written = self
            .bytes_written
            .checked_add(u64::try_from(written).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "write count")
            })?)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "write count"))?;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn persistent_abort_cleanup<B: PersistentStagingBackend>(
    backend: &mut B,
) -> Option<&'static str> {
    backend.abort_private().err()
}

/// Copies one version-bound base and tail into private staging, validates and synchronizes the
/// private artifact, publishes without replacement, synchronizes the parent namespace, and retires
/// the private name.
///
/// Failures before a definite link attempt abort private staging. An indeterminate link or failed
/// parent synchronization retains private state and returns an explicit indeterminate outcome.
/// Cleanup failure after successful parent synchronization cannot downgrade durable publication.
pub fn stage_and_publish_versioned_source_with_tail<
    S: PersistentVersionedReadAt,
    B: PersistentStagingBackend,
>(
    source: &mut S,
    backend: &mut B,
    identity: PersistentSourceIdentity,
    tail: &[u8],
    limits: ImmutableSourceLimits,
    options: PersistentSourceCopyOptions,
) -> Result<PersistentStagedPublicationReport, PersistentStagedPublicationError> {
    let tail_length = u64::try_from(tail.len())
        .map_err(|_| PersistentStagedPublicationError::Invariant("tail length"))?;
    let output_length = identity
        .length
        .checked_add(tail_length)
        .ok_or(PersistentStagedPublicationError::Invariant("output length"))?;
    let max_output = u64::try_from(limits.format.max_output_bytes)
        .map_err(|_| PersistentStagedPublicationError::Invariant("output limit"))?;
    let max_file = u64::try_from(limits.format.max_file_bytes)
        .map_err(|_| PersistentStagedPublicationError::Invariant("file limit"))?;
    if output_length > max_output || output_length > max_file {
        return Err(PersistentStagedPublicationError::Invariant("output limit"));
    }

    backend.begin_private(output_length).map_err(|error| {
        PersistentStagedPublicationError::Backend {
            stage: PersistentPublicationStage::BeginPrivate,
            error,
            cleanup_error: None,
        }
    })?;

    let (source_report, staged_length, staged_sha256) = {
        let mut digest_writer = PersistentPublicationDigestWriter {
            inner: backend,
            hasher: Sha256::new(),
            bytes_written: 0,
        };
        let source_report = match append_versioned_source_with_tail_to(
            source,
            &mut digest_writer,
            identity,
            tail,
            limits,
            options,
        ) {
            Ok(report) => report,
            Err(error) => {
                let cleanup_error = persistent_abort_cleanup(digest_writer.inner);
                return Err(PersistentStagedPublicationError::Copy {
                    error,
                    cleanup_error,
                });
            }
        };
        (
            source_report,
            digest_writer.bytes_written,
            <[u8; 32]>::from(digest_writer.hasher.finalize()),
        )
    };

    if staged_length != output_length {
        let _ = backend.abort_private();
        return Err(PersistentStagedPublicationError::Invariant("staged length"));
    }

    if let Err(error) = backend.validate_private(output_length, staged_sha256) {
        let cleanup_error = persistent_abort_cleanup(backend);
        return Err(PersistentStagedPublicationError::Backend {
            stage: PersistentPublicationStage::ValidatePrivate,
            error,
            cleanup_error,
        });
    }
    if let Err(error) = backend.sync_private() {
        let cleanup_error = persistent_abort_cleanup(backend);
        return Err(PersistentStagedPublicationError::Backend {
            stage: PersistentPublicationStage::SyncPrivate,
            error,
            cleanup_error,
        });
    }

    match backend.publish_no_replace() {
        Ok(PersistentPublicationLinkOutcome::DestinationExists) => {
            let cleanup_error = persistent_abort_cleanup(backend);
            Ok(PersistentStagedPublicationReport {
                source: source_report,
                output_length,
                output_sha256: staged_sha256,
                outcome: PersistentStagedPublicationOutcome::NotPublishedDestinationExists,
                cleanup_error,
            })
        }
        Ok(PersistentPublicationLinkOutcome::Indeterminate) => {
            Ok(PersistentStagedPublicationReport {
                source: source_report,
                output_length,
                output_sha256: staged_sha256,
                outcome: PersistentStagedPublicationOutcome::PublicationIndeterminate {
                    stage: PersistentPublicationStage::PublishLink,
                },
                cleanup_error: None,
            })
        }
        Err(error) => {
            let cleanup_error = persistent_abort_cleanup(backend);
            Err(PersistentStagedPublicationError::Backend {
                stage: PersistentPublicationStage::PublishLink,
                error,
                cleanup_error,
            })
        }
        Ok(PersistentPublicationLinkOutcome::Linked) => {
            if backend.sync_parent().is_err() {
                return Ok(PersistentStagedPublicationReport {
                    source: source_report,
                    output_length,
                    output_sha256: staged_sha256,
                    outcome: PersistentStagedPublicationOutcome::PublicationIndeterminate {
                        stage: PersistentPublicationStage::SyncParent,
                    },
                    cleanup_error: None,
                });
            }
            let cleanup_error = backend.retire_private().err();
            Ok(PersistentStagedPublicationReport {
                source: source_report,
                output_length,
                output_sha256: staged_sha256,
                outcome: PersistentStagedPublicationOutcome::PublishedAndDurable {
                    cleanup_pending: cleanup_error.is_some(),
                },
                cleanup_error,
            })
        }
    }
}

#[cfg(test)]
mod persistent_staged_publication_tests {
    use super::*;

    struct VersionedBytes {
        bytes: Vec<u8>,
        version: PersistentSourceVersion,
        reads: usize,
        mutate_after_read: Option<usize>,
    }

    impl ImmutableReadAt for VersionedBytes {
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

    impl PersistentVersionedReadAt for VersionedBytes {
        fn version_token(&mut self) -> Result<PersistentSourceVersion, ImmutableSourceError> {
            Ok(self.version)
        }
    }

    struct MemoryBackend {
        private: Vec<u8>,
        destination: Option<Vec<u8>>,
        begun: bool,
        expected_length: u64,
        link: PersistentPublicationLinkOutcome,
        indeterminate_publishes: bool,
        fail: Option<PersistentPublicationStage>,
        calls: Vec<PersistentPublicationStage>,
    }

    impl MemoryBackend {
        fn new(link: PersistentPublicationLinkOutcome) -> Self {
            Self {
                private: Vec::new(),
                destination: None,
                begun: false,
                expected_length: 0,
                link,
                indeterminate_publishes: false,
                fail: None,
                calls: Vec::new(),
            }
        }

        fn step(&mut self, stage: PersistentPublicationStage) -> Result<(), &'static str> {
            self.calls.push(stage);
            if self.fail == Some(stage) {
                Err("injected")
            } else {
                Ok(())
            }
        }
    }

    impl std::io::Write for MemoryBackend {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            if !self.begun {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "private staging not begun",
                ));
            }
            self.private.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl PersistentStagingBackend for MemoryBackend {
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
                return Err("private validation");
            }
            Ok(())
        }

        fn sync_private(&mut self) -> Result<(), &'static str> {
            self.step(PersistentPublicationStage::SyncPrivate)
        }

        fn publish_no_replace(
            &mut self,
        ) -> Result<PersistentPublicationLinkOutcome, &'static str> {
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
            self.step(PersistentPublicationStage::AbortPrivate)?;
            self.private.clear();
            self.begun = false;
            Ok(())
        }
    }

    fn limits(length: usize, chunk: usize) -> ImmutableSourceLimits {
        ImmutableSourceLimits {
            format: ImmutableLimits {
                max_file_bytes: 1024 * 1024,
                max_output_bytes: 1024 * 1024,
                max_allocation_bytes: 1024 * 1024,
                ..ImmutableLimits::default()
            },
            max_read_request_bytes: chunk,
            max_total_bytes_read: u64::try_from(length * 2).expect("budget"),
            max_read_operations: 1_000_000,
            ..ImmutableSourceLimits::default()
        }
    }

    fn source(bytes: Vec<u8>) -> VersionedBytes {
        VersionedBytes {
            bytes,
            version: PersistentSourceVersion([11; 32]),
            reads: 0,
            mutate_after_read: None,
        }
    }

    #[test]
    fn durable_publication_uses_private_sync_link_parent_sync_and_retire() {
        let base = vec![41_u8; 1024];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut backend = MemoryBackend::new(PersistentPublicationLinkOutcome::Linked);
        let report = stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), 127),
            PersistentSourceCopyOptions {
                max_write_request_bytes: 31,
            },
        )
        .expect("publication");
        let mut expected = base;
        expected.extend_from_slice(b"tail");
        assert_eq!(backend.destination, Some(expected));
        assert!(backend.private.is_empty());
        assert_eq!(
            report.outcome,
            PersistentStagedPublicationOutcome::PublishedAndDurable {
                cleanup_pending: false
            }
        );
        assert_eq!(
            backend.calls,
            vec![
                PersistentPublicationStage::BeginPrivate,
                PersistentPublicationStage::ValidatePrivate,
                PersistentPublicationStage::SyncPrivate,
                PersistentPublicationStage::PublishLink,
                PersistentPublicationStage::SyncParent,
                PersistentPublicationStage::RetirePrivate,
            ]
        );
    }

    #[test]
    fn destination_exists_is_not_published_and_private_is_aborted() {
        let base = vec![43_u8; 512];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut backend = MemoryBackend::new(PersistentPublicationLinkOutcome::DestinationExists);
        backend.destination = Some(vec![1, 2, 3]);
        let report = stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), 64),
            PersistentSourceCopyOptions::default(),
        )
        .expect("destination exists");
        assert_eq!(
            report.outcome,
            PersistentStagedPublicationOutcome::NotPublishedDestinationExists
        );
        assert_eq!(backend.destination, Some(vec![1, 2, 3]));
        assert!(backend.private.is_empty());
    }

    #[test]
    fn indeterminate_link_retains_private_state() {
        let base = vec![47_u8; 512];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut backend = MemoryBackend::new(PersistentPublicationLinkOutcome::Indeterminate);
        backend.indeterminate_publishes = true;
        let report = stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), 64),
            PersistentSourceCopyOptions::default(),
        )
        .expect("indeterminate");
        assert_eq!(
            report.outcome,
            PersistentStagedPublicationOutcome::PublicationIndeterminate {
                stage: PersistentPublicationStage::PublishLink
            }
        );
        assert!(!backend.private.is_empty());
        assert!(backend.destination.is_some());
    }

    #[test]
    fn failed_parent_sync_is_indeterminate_and_keeps_private_name() {
        let base = vec![53_u8; 512];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut backend = MemoryBackend::new(PersistentPublicationLinkOutcome::Linked);
        backend.fail = Some(PersistentPublicationStage::SyncParent);
        let report = stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), 64),
            PersistentSourceCopyOptions::default(),
        )
        .expect("indeterminate");
        assert_eq!(
            report.outcome,
            PersistentStagedPublicationOutcome::PublicationIndeterminate {
                stage: PersistentPublicationStage::SyncParent
            }
        );
        assert!(backend.destination.is_some());
        assert!(!backend.private.is_empty());
    }

    #[test]
    fn cleanup_failure_does_not_downgrade_durable_publication() {
        let base = vec![59_u8; 512];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut backend = MemoryBackend::new(PersistentPublicationLinkOutcome::Linked);
        backend.fail = Some(PersistentPublicationStage::RetirePrivate);
        let report = stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), 64),
            PersistentSourceCopyOptions::default(),
        )
        .expect("durable");
        assert_eq!(
            report.outcome,
            PersistentStagedPublicationOutcome::PublishedAndDurable {
                cleanup_pending: true
            }
        );
        assert_eq!(report.cleanup_error, Some("injected"));
        assert!(backend.destination.is_some());
    }

    #[test]
    fn source_version_change_aborts_private_state_before_publication() {
        let base = vec![61_u8; 1024];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let chunk = 128;
        let first_pass_reads = base.len().div_ceil(chunk);
        let mut input = source(base.clone());
        input.mutate_after_read = Some(first_pass_reads + 2);
        let mut backend = MemoryBackend::new(PersistentPublicationLinkOutcome::Linked);
        let error = stage_and_publish_versioned_source_with_tail(
            &mut input,
            &mut backend,
            identity,
            b"tail",
            limits(base.len(), chunk),
            PersistentSourceCopyOptions::default(),
        )
        .expect_err("version change");
        assert!(matches!(
            error,
            PersistentStagedPublicationError::Copy {
                error: PersistentVersionedSourceCopyError::VersionChanged(
                    PersistentSourceCopyPhase::Copy
                ),
                cleanup_error: None
            }
        ));
        assert!(backend.private.is_empty());
        assert!(backend.destination.is_none());
        assert!(backend.calls.contains(&PersistentPublicationStage::AbortPrivate));
        assert!(!backend.calls.contains(&PersistentPublicationStage::PublishLink));
    }
}
