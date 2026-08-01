#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersistentSourceIdentity {
    pub length: u64,
    pub sha256: [u8; 32],
}

impl PersistentSourceIdentity {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ImmutableSourceError> {
        Ok(Self {
            length: u64::try_from(bytes.len())
                .map_err(|_| ImmutableSourceError::Limit("length"))?,
            sha256: Sha256::digest(bytes).into(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersistentSourceCopyOptions {
    pub max_write_request_bytes: usize,
}

impl Default for PersistentSourceCopyOptions {
    fn default() -> Self {
        Self {
            max_write_request_bytes: 64 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersistentSourceCopyPhase {
    Preflight,
    Copy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentSourceCopyError {
    Source(ImmutableSourceError),
    IdentityMismatch(PersistentSourceCopyPhase),
    Io(std::io::ErrorKind),
}

impl std::fmt::Display for PersistentSourceCopyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "persistent source copy failed: {error}"),
            Self::IdentityMismatch(PersistentSourceCopyPhase::Preflight) => {
                write!(formatter, "persistent source identity mismatch before output")
            }
            Self::IdentityMismatch(PersistentSourceCopyPhase::Copy) => {
                write!(formatter, "persistent source changed after output began")
            }
            Self::Io(kind) => write!(formatter, "persistent source sink failed: {kind:?}"),
        }
    }
}

impl std::error::Error for PersistentSourceCopyError {}

impl From<ImmutableSourceError> for PersistentSourceCopyError {
    fn from(error: ImmutableSourceError) -> Self {
        Self::Source(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentSourceCopyReport {
    pub identity: PersistentSourceIdentity,
    pub base_bytes_written: u64,
    pub tail_bytes_written: u64,
    pub read_operations: u64,
    pub bytes_read: u64,
    pub largest_read_request: usize,
    pub largest_write_request: usize,
    pub tail_sha256: [u8; 32],
}

struct PersistentSourceCopyBudget {
    read_operations: u64,
    bytes_read: u64,
    largest_read_request: usize,
}

impl PersistentSourceCopyBudget {
    fn new() -> Self {
        Self {
            read_operations: 0,
            bytes_read: 0,
            largest_read_request: 0,
        }
    }

    fn charge(
        &mut self,
        length: usize,
        limits: ImmutableSourceLimits,
    ) -> Result<(), PersistentSourceCopyError> {
        if self.read_operations >= limits.max_read_operations {
            return Err(ImmutableSourceError::Limit("read operations").into());
        }
        let length_u64 = u64::try_from(length)
            .map_err(|_| ImmutableSourceError::Limit("read bytes"))?;
        let next = self
            .bytes_read
            .checked_add(length_u64)
            .ok_or(ImmutableSourceError::Limit("read bytes"))?;
        if next > limits.max_total_bytes_read {
            return Err(ImmutableSourceError::Limit("read bytes").into());
        }
        self.read_operations += 1;
        self.bytes_read = next;
        self.largest_read_request = self.largest_read_request.max(length);
        Ok(())
    }
}

fn persistent_source_copy_length<S: ImmutableReadAt>(
    source: &mut S,
    identity: PersistentSourceIdentity,
    phase: PersistentSourceCopyPhase,
    limits: ImmutableSourceLimits,
) -> Result<usize, PersistentSourceCopyError> {
    let length = source.len()?;
    if length != identity.length {
        return Err(PersistentSourceCopyError::IdentityMismatch(phase));
    }
    let length = usize::try_from(length)
        .map_err(|_| ImmutableSourceError::Limit("length"))?;
    if length > limits.format.max_file_bytes {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit("file size")).into());
    }
    Ok(length)
}

fn persistent_source_copy_chunk(
    length: usize,
    limits: ImmutableSourceLimits,
) -> Result<usize, PersistentSourceCopyError> {
    if limits.max_read_request_bytes == 0 || limits.format.max_allocation_bytes == 0 {
        return Err(ImmutableSourceError::Limit("configuration").into());
    }
    Ok(length
        .max(1)
        .min(limits.max_read_request_bytes)
        .min(limits.format.max_allocation_bytes))
}

fn hash_persistent_source<S: ImmutableReadAt>(
    source: &mut S,
    identity: PersistentSourceIdentity,
    phase: PersistentSourceCopyPhase,
    limits: ImmutableSourceLimits,
    budget: &mut PersistentSourceCopyBudget,
) -> Result<[u8; 32], PersistentSourceCopyError> {
    let length = persistent_source_copy_length(source, identity, phase, limits)?;
    let chunk = persistent_source_copy_chunk(length, limits)?;
    let mut buffer = vec![0_u8; chunk];
    let mut hasher = Sha256::new();
    let mut offset = 0_usize;
    while offset < length {
        let take = (length - offset).min(buffer.len());
        budget.charge(take, limits)?;
        source.read_exact_at(
            u64::try_from(offset).map_err(|_| ImmutableSourceError::Limit("offset"))?,
            &mut buffer[..take],
        )?;
        hasher.update(&buffer[..take]);
        offset = offset
            .checked_add(take)
            .ok_or(ImmutableSourceError::Limit("offset"))?;
    }
    if source.len()? != identity.length {
        return Err(PersistentSourceCopyError::IdentityMismatch(phase));
    }
    Ok(hasher.finalize().into())
}

fn write_persistent_source_chunks<W: std::io::Write>(
    writer: &mut W,
    bytes: &[u8],
    max_request: usize,
    largest_write_request: &mut usize,
) -> Result<(), PersistentSourceCopyError> {
    for chunk in bytes.chunks(max_request) {
        *largest_write_request = (*largest_write_request).max(chunk.len());
        writer
            .write_all(chunk)
            .map_err(|error| PersistentSourceCopyError::Io(error.kind()))?;
    }
    Ok(())
}

/// Copies an independently identified immutable base from bounded random-access storage and then
/// appends a previously constructed tail.
///
/// The complete source is hashed once before the first sink write. The source is then read and
/// hashed a second time while being copied. The tail is withheld until the copied base still matches
/// the pinned length and SHA-256 identity. A mismatch during the second pass is terminal after base
/// output may have begun; atomic visibility therefore still requires private staging. This API does
/// not claim that an unversioned source cannot change between reads.
pub fn append_verified_source_with_tail_to<S: ImmutableReadAt, W: std::io::Write>(
    source: &mut S,
    writer: &mut W,
    identity: PersistentSourceIdentity,
    tail: &[u8],
    limits: ImmutableSourceLimits,
    options: PersistentSourceCopyOptions,
) -> Result<PersistentSourceCopyReport, PersistentSourceCopyError> {
    if options.max_write_request_bytes == 0 {
        return Err(ImmutableSourceError::Limit("write request").into());
    }
    let base_len = usize::try_from(identity.length)
        .map_err(|_| ImmutableSourceError::Limit("length"))?;
    let output_len = base_len
        .checked_add(tail.len())
        .ok_or(ImmutableSourceError::Format(ImmutableError::Limit("output")))?;
    if output_len > limits.format.max_output_bytes || output_len > limits.format.max_file_bytes {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit("output")).into());
    }
    let chunk = persistent_source_copy_chunk(base_len, limits)?;
    let operations_per_pass = base_len.div_ceil(chunk);
    let required_operations = u64::try_from(operations_per_pass)
        .map_err(|_| ImmutableSourceError::Limit("read operations"))?
        .checked_mul(2)
        .ok_or(ImmutableSourceError::Limit("read operations"))?;
    let required_bytes = identity
        .length
        .checked_mul(2)
        .ok_or(ImmutableSourceError::Limit("read bytes"))?;
    if required_operations > limits.max_read_operations {
        return Err(ImmutableSourceError::Limit("read operations").into());
    }
    if required_bytes > limits.max_total_bytes_read {
        return Err(ImmutableSourceError::Limit("read bytes").into());
    }

    let mut budget = PersistentSourceCopyBudget::new();
    let preflight_digest = hash_persistent_source(
        source,
        identity,
        PersistentSourceCopyPhase::Preflight,
        limits,
        &mut budget,
    )?;
    if preflight_digest != identity.sha256 {
        return Err(PersistentSourceCopyError::IdentityMismatch(
            PersistentSourceCopyPhase::Preflight,
        ));
    }

    let length = persistent_source_copy_length(
        source,
        identity,
        PersistentSourceCopyPhase::Copy,
        limits,
    )?;
    let mut buffer = vec![0_u8; chunk];
    let mut hasher = Sha256::new();
    let mut offset = 0_usize;
    let mut largest_write_request = 0_usize;
    while offset < length {
        let take = (length - offset).min(buffer.len());
        budget.charge(take, limits)?;
        source.read_exact_at(
            u64::try_from(offset).map_err(|_| ImmutableSourceError::Limit("offset"))?,
            &mut buffer[..take],
        )?;
        hasher.update(&buffer[..take]);
        write_persistent_source_chunks(
            writer,
            &buffer[..take],
            options.max_write_request_bytes,
            &mut largest_write_request,
        )?;
        offset = offset
            .checked_add(take)
            .ok_or(ImmutableSourceError::Limit("offset"))?;
    }
    let copied_digest: [u8; 32] = hasher.finalize().into();
    if source.len()? != identity.length || copied_digest != identity.sha256 {
        return Err(PersistentSourceCopyError::IdentityMismatch(
            PersistentSourceCopyPhase::Copy,
        ));
    }

    write_persistent_source_chunks(
        writer,
        tail,
        options.max_write_request_bytes,
        &mut largest_write_request,
    )?;
    Ok(PersistentSourceCopyReport {
        identity,
        base_bytes_written: identity.length,
        tail_bytes_written: u64::try_from(tail.len())
            .map_err(|_| ImmutableSourceError::Limit("tail bytes"))?,
        read_operations: budget.read_operations,
        bytes_read: budget.bytes_read,
        largest_read_request: budget.largest_read_request,
        largest_write_request,
        tail_sha256: Sha256::digest(tail).into(),
    })
}

#[cfg(test)]
mod persistent_source_copy_tests {
    use super::*;

    #[derive(Debug)]
    struct TestSource {
        bytes: Vec<u8>,
        reads: usize,
        mutate_at_read: Option<usize>,
    }

    impl TestSource {
        fn stable(bytes: Vec<u8>) -> Self {
            Self {
                bytes,
                reads: 0,
                mutate_at_read: None,
            }
        }
    }

    impl ImmutableReadAt for TestSource {
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

    #[derive(Debug)]
    struct FailingWriter {
        remaining: usize,
        bytes: Vec<u8>,
    }

    impl std::io::Write for FailingWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "injected"));
            }
            let take = self.remaining.min(buffer.len());
            self.bytes.extend_from_slice(&buffer[..take]);
            self.remaining -= take;
            Ok(take)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn object(object_id: u64, seed: u8) -> ImmutableObjectInput {
        ImmutableObjectInput::new(object_id, u16::from(seed % 31 + 1), vec![seed; 9])
    }

    #[test]
    fn bounded_source_copy_matches_persistent_successor() {
        let format = ImmutableLimits {
            max_file_bytes: 32 * 1024 * 1024,
            max_output_bytes: 32 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = build_genesis(
            &(1..=220)
                .map(|index| object(u64::try_from(index * 2).expect("id"), index as u8))
                .collect::<Vec<_>>(),
            format,
        )
        .expect("base");
        let owned = append_persistent_batch(
            &base,
            &[
                ImmutableBatchOperation::Put(object(2, 201)),
                ImmutableBatchOperation::Put(object(440, 202)),
            ],
            format,
        )
        .expect("owned");
        let tail = &owned.bytes[base.len()..];
        let identity = PersistentSourceIdentity::from_bytes(&base).expect("identity");
        let mut source = TestSource::stable(base.clone());
        let mut output = Vec::new();
        let report = append_verified_source_with_tail_to(
            &mut source,
            &mut output,
            identity,
            tail,
            ImmutableSourceLimits {
                format,
                max_read_request_bytes: 113,
                max_total_bytes_read: u64::try_from(base.len() * 2).expect("budget"),
                ..ImmutableSourceLimits::default()
            },
            PersistentSourceCopyOptions {
                max_write_request_bytes: 37,
            },
        )
        .expect("copy");

        assert_eq!(output, owned.bytes);
        assert_eq!(report.base_bytes_written, identity.length);
        assert_eq!(report.tail_bytes_written, u64::try_from(tail.len()).expect("tail"));
        assert_eq!(report.bytes_read, identity.length * 2);
        assert!(report.largest_read_request <= 113);
        assert!(report.largest_write_request <= 37);
    }

    #[test]
    fn preflight_identity_mismatch_writes_nothing() {
        let bytes = vec![7_u8; 4096];
        let mut identity = PersistentSourceIdentity::from_bytes(&bytes).expect("identity");
        identity.sha256[0] ^= 1;
        let mut source = TestSource::stable(bytes);
        let mut output = Vec::new();
        let error = append_verified_source_with_tail_to(
            &mut source,
            &mut output,
            identity,
            b"tail",
            ImmutableSourceLimits {
                max_read_request_bytes: 127,
                max_total_bytes_read: 8192,
                ..ImmutableSourceLimits::default()
            },
            PersistentSourceCopyOptions::default(),
        )
        .expect_err("mismatch");
        assert_eq!(
            error,
            PersistentSourceCopyError::IdentityMismatch(PersistentSourceCopyPhase::Preflight)
        );
        assert!(output.is_empty());
    }

    #[test]
    fn second_pass_mutation_withholds_tail() {
        let bytes = vec![11_u8; 2048];
        let identity = PersistentSourceIdentity::from_bytes(&bytes).expect("identity");
        let chunk = 128;
        let first_pass_reads = bytes.len().div_ceil(chunk);
        let mut source = TestSource {
            bytes,
            reads: 0,
            mutate_at_read: Some(first_pass_reads + 1),
        };
        let mut output = Vec::new();
        let error = append_verified_source_with_tail_to(
            &mut source,
            &mut output,
            identity,
            b"TAIL-MUST-NOT-APPEAR",
            ImmutableSourceLimits {
                max_read_request_bytes: chunk,
                max_total_bytes_read: identity.length * 2,
                ..ImmutableSourceLimits::default()
            },
            PersistentSourceCopyOptions {
                max_write_request_bytes: 53,
            },
        )
        .expect_err("changed source");
        assert_eq!(
            error,
            PersistentSourceCopyError::IdentityMismatch(PersistentSourceCopyPhase::Copy)
        );
        assert_eq!(output.len(), usize::try_from(identity.length).expect("length"));
        assert!(!output.ends_with(b"TAIL-MUST-NOT-APPEAR"));
    }

    #[test]
    fn insufficient_two_pass_budget_fails_before_output() {
        let bytes = vec![13_u8; 1024];
        let identity = PersistentSourceIdentity::from_bytes(&bytes).expect("identity");
        let mut source = TestSource::stable(bytes);
        let mut output = Vec::new();
        let error = append_verified_source_with_tail_to(
            &mut source,
            &mut output,
            identity,
            b"tail",
            ImmutableSourceLimits {
                max_read_request_bytes: 128,
                max_total_bytes_read: identity.length * 2 - 1,
                ..ImmutableSourceLimits::default()
            },
            PersistentSourceCopyOptions::default(),
        )
        .expect_err("budget");
        assert_eq!(error, PersistentSourceCopyError::Source(ImmutableSourceError::Limit("read bytes")));
        assert!(output.is_empty());
        assert_eq!(source.reads, 0);
    }

    #[test]
    fn sink_failure_is_terminal_and_tail_is_not_completed() {
        let bytes = vec![17_u8; 1024];
        let identity = PersistentSourceIdentity::from_bytes(&bytes).expect("identity");
        let mut source = TestSource::stable(bytes);
        let mut writer = FailingWriter {
            remaining: 300,
            bytes: Vec::new(),
        };
        let error = append_verified_source_with_tail_to(
            &mut source,
            &mut writer,
            identity,
            b"tail",
            ImmutableSourceLimits {
                max_read_request_bytes: 128,
                max_total_bytes_read: identity.length * 2,
                ..ImmutableSourceLimits::default()
            },
            PersistentSourceCopyOptions {
                max_write_request_bytes: 64,
            },
        )
        .expect_err("sink");
        assert_eq!(error, PersistentSourceCopyError::Io(std::io::ErrorKind::Other));
        assert_eq!(writer.bytes.len(), 300);
    }
}
