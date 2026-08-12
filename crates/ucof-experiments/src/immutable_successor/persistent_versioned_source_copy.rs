#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PersistentSourceVersion(pub [u8; 32]);

/// Random-access source whose version token has strong non-ABA semantics.
///
/// Equal tokens must identify the same immutable object bytes and length for the lifetime of one
/// operation. Provider adapters are responsible for deriving this token from a strong entity tag,
/// generation, or equivalent immutable object identity.
pub trait PersistentVersionedReadAt: ImmutableReadAt {
    fn version_token(&mut self) -> Result<PersistentSourceVersion, ImmutableSourceError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentVersionedSourceCopyError {
    Version(ImmutableSourceError),
    VersionChanged(PersistentSourceCopyPhase),
    Copy(PersistentSourceCopyError),
}

impl std::fmt::Display for PersistentVersionedSourceCopyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(error) => write!(formatter, "persistent source version failed: {error}"),
            Self::VersionChanged(PersistentSourceCopyPhase::Preflight) => {
                write!(formatter, "persistent source version changed before output")
            }
            Self::VersionChanged(PersistentSourceCopyPhase::Copy) => {
                write!(formatter, "persistent source version changed after output began")
            }
            Self::Copy(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for PersistentVersionedSourceCopyError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentVersionedSourceCopyReport {
    pub copy: PersistentSourceCopyReport,
    pub version: PersistentSourceVersion,
    pub version_checks: u64,
}

struct PersistentStableSource<'a, S> {
    inner: &'a mut S,
    expected: PersistentSourceVersion,
    version_checks: u64,
    version_changed: bool,
    version_error: Option<ImmutableSourceError>,
}

impl<'a, S: PersistentVersionedReadAt> PersistentStableSource<'a, S> {
    fn new(inner: &'a mut S, expected: PersistentSourceVersion) -> Self {
        Self {
            inner,
            expected,
            version_checks: 0,
            version_changed: false,
            version_error: None,
        }
    }

    fn ensure_stable(&mut self) -> Result<(), ImmutableSourceError> {
        self.version_checks = self
            .version_checks
            .checked_add(1)
            .ok_or(ImmutableSourceError::Limit("version checks"))?;
        match self.inner.version_token() {
            Ok(actual) if actual == self.expected => Ok(()),
            Ok(_) => {
                self.version_changed = true;
                Err(ImmutableSourceError::Io("version changed"))
            }
            Err(error) => {
                self.version_error = Some(error.clone());
                Err(error)
            }
        }
    }
}

impl<S: PersistentVersionedReadAt> ImmutableReadAt for PersistentStableSource<'_, S> {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        self.ensure_stable()?;
        let length = self.inner.len()?;
        self.ensure_stable()?;
        Ok(length)
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        self.ensure_stable()?;
        self.inner.read_exact_at(offset, buffer)?;
        self.ensure_stable()
    }
}

struct PersistentCountingWriter<'a, W> {
    inner: &'a mut W,
    bytes_written: u64,
}

impl<W: std::io::Write> std::io::Write for PersistentCountingWriter<'_, W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
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

/// Copies one strongly version-bound source and then appends a preconstructed persistent tail.
///
/// The whole-file length and SHA-256 identity are still checked by the bounded two-pass executor.
/// In addition, the same strong non-ABA version token is required immediately before and after every
/// source length or range operation. Bytes from a range whose token changes are never handed to the
/// sink. A later copy-phase change can still leave earlier base chunks in the sink, so atomic
/// visibility continues to require private staging.
pub fn append_versioned_source_with_tail_to<S: PersistentVersionedReadAt, W: std::io::Write>(
    source: &mut S,
    writer: &mut W,
    identity: PersistentSourceIdentity,
    tail: &[u8],
    limits: ImmutableSourceLimits,
    options: PersistentSourceCopyOptions,
) -> Result<PersistentVersionedSourceCopyReport, PersistentVersionedSourceCopyError> {
    let version = source
        .version_token()
        .map_err(PersistentVersionedSourceCopyError::Version)?;
    let mut stable = PersistentStableSource::new(source, version);
    let mut counted = PersistentCountingWriter {
        inner: writer,
        bytes_written: 0,
    };
    let result = append_verified_source_with_tail_to(
        &mut stable,
        &mut counted,
        identity,
        tail,
        limits,
        options,
    );
    let phase = if counted.bytes_written == 0 {
        PersistentSourceCopyPhase::Preflight
    } else {
        PersistentSourceCopyPhase::Copy
    };
    if stable.version_changed {
        return Err(PersistentVersionedSourceCopyError::VersionChanged(phase));
    }
    if let Some(error) = stable.version_error {
        return Err(PersistentVersionedSourceCopyError::Version(error));
    }
    let copy = result.map_err(PersistentVersionedSourceCopyError::Copy)?;
    Ok(PersistentVersionedSourceCopyReport {
        copy,
        version,
        version_checks: stable.version_checks,
    })
}

#[cfg(test)]
mod persistent_versioned_source_copy_tests {
    use super::*;

    #[derive(Debug)]
    struct VersionedBytes {
        bytes: Vec<u8>,
        version: PersistentSourceVersion,
        reads: usize,
        mutate_after_read: Option<usize>,
        fail_version_check: Option<usize>,
        version_checks: usize,
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
            let source = self
                .bytes
                .get(start..end)
                .ok_or(ImmutableSourceError::Io("range"))?;
            buffer.copy_from_slice(source);
            self.reads += 1;
            if self.mutate_after_read == Some(self.reads) {
                self.version.0[0] ^= 1;
            }
            Ok(())
        }
    }

    impl PersistentVersionedReadAt for VersionedBytes {
        fn version_token(&mut self) -> Result<PersistentSourceVersion, ImmutableSourceError> {
            self.version_checks += 1;
            if self.fail_version_check == Some(self.version_checks) {
                return Err(ImmutableSourceError::Io("version service"));
            }
            Ok(self.version)
        }
    }

    fn source(bytes: Vec<u8>) -> VersionedBytes {
        VersionedBytes {
            bytes,
            version: PersistentSourceVersion([7; 32]),
            reads: 0,
            mutate_after_read: None,
            fail_version_check: None,
            version_checks: 0,
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

    #[test]
    fn stable_versioned_copy_matches_base_and_tail() {
        let base = vec![19_u8; 4096];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        let mut output = Vec::new();
        let report = append_versioned_source_with_tail_to(
            &mut input,
            &mut output,
            identity,
            b"tail",
            limits(base.len(), 127),
            PersistentSourceCopyOptions {
                max_write_request_bytes: 43,
            },
        )
        .expect("copy");
        let mut expected = base;
        expected.extend_from_slice(b"tail");
        assert_eq!(output, expected);
        assert_eq!(report.version, PersistentSourceVersion([7; 32]));
        assert!(report.version_checks > 4);
        assert_eq!(report.copy.bytes_read, identity.length * 2);
    }

    #[test]
    fn preflight_version_change_writes_nothing() {
        let base = vec![23_u8; 1024];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        input.mutate_after_read = Some(1);
        let mut output = Vec::new();
        let error = append_versioned_source_with_tail_to(
            &mut input,
            &mut output,
            identity,
            b"tail",
            limits(base.len(), 128),
            PersistentSourceCopyOptions::default(),
        )
        .expect_err("version change");
        assert_eq!(
            error,
            PersistentVersionedSourceCopyError::VersionChanged(
                PersistentSourceCopyPhase::Preflight
            )
        );
        assert!(output.is_empty());
    }

    #[test]
    fn copy_phase_version_change_withholds_tail() {
        let base = vec![29_u8; 1024];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let chunk = 128;
        let first_pass_reads = base.len().div_ceil(chunk);
        let mut input = source(base.clone());
        input.mutate_after_read = Some(first_pass_reads + 2);
        let mut output = Vec::new();
        let error = append_versioned_source_with_tail_to(
            &mut input,
            &mut output,
            identity,
            b"TAIL",
            limits(base.len(), chunk),
            PersistentSourceCopyOptions {
                max_write_request_bytes: 31,
            },
        )
        .expect_err("version change");
        assert_eq!(
            error,
            PersistentVersionedSourceCopyError::VersionChanged(PersistentSourceCopyPhase::Copy)
        );
        assert_eq!(output, base[..chunk]);
    }

    #[test]
    fn version_service_failure_is_distinct() {
        let base = vec![31_u8; 512];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut input = source(base.clone());
        input.fail_version_check = Some(2);
        let mut output = Vec::new();
        let error = append_versioned_source_with_tail_to(
            &mut input,
            &mut output,
            identity,
            b"tail",
            limits(base.len(), 64),
            PersistentSourceCopyOptions::default(),
        )
        .expect_err("version service");
        assert_eq!(
            error,
            PersistentVersionedSourceCopyError::Version(ImmutableSourceError::Io(
                "version service"
            ))
        );
        assert!(output.is_empty());
    }
}
