use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Failures specific to a strong-version conditional range operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalSourceError {
    /// A terminal client or transport failure that must not be retried automatically.
    Client(&'static str),
    /// A transient transport failure that a bounded retry wrapper may retry.
    RetryableClient(&'static str),
    InvalidVersionToken,
    VersionChanged,
    Protocol(&'static str),
    Cancelled,
    DeadlineExceeded,
    Limit(&'static str),
}

impl fmt::Display for ConditionalSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(label) => write!(formatter, "conditional source client failed: {label}"),
            Self::RetryableClient(label) => {
                write!(formatter, "conditional source client transient failure: {label}")
            }
            Self::InvalidVersionToken => write!(formatter, "strong version token required"),
            Self::VersionChanged => write!(formatter, "source version changed"),
            Self::Protocol(label) => write!(formatter, "conditional range protocol error: {label}"),
            Self::Cancelled => write!(formatter, "source operation cancelled"),
            Self::DeadlineExceeded => write!(formatter, "source operation deadline exceeded"),
            Self::Limit(label) => write!(formatter, "conditional source {label} limit exceeded"),
        }
    }
}

impl Error for ConditionalSourceError {}

/// A validated strong HTTP-style entity tag or equivalent immutable object-version token.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StrongVersionToken(String);

impl StrongVersionToken {
    pub fn parse(token: impl Into<String>) -> Result<Self, ConditionalSourceError> {
        let token = token.into();
        if token.len() < 2
            || token.starts_with("W/")
            || !token.starts_with('"')
            || !token.ends_with('"')
            || token[1..token.len() - 1].contains('"')
        {
            return Err(ConditionalSourceError::InvalidVersionToken);
        }
        Ok(Self(token))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionalObjectMetadata {
    pub length: u64,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionalRangeResponse {
    pub version: String,
    pub offset: u64,
    pub total_length: u64,
    pub body: Vec<u8>,
}

/// Transport-specific client used by [`ConditionalReadAt`].
///
/// HTTP implementations should use `If-Match`; immutable-version cloud APIs may bind the request
/// to a provider version identifier with equivalent no-mixing semantics.
pub trait ConditionalRangeClient {
    fn metadata(&mut self) -> Result<ConditionalObjectMetadata, ConditionalSourceError>;

    fn read_range_if_match(
        &mut self,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
    ) -> Result<ConditionalRangeResponse, ConditionalSourceError>;
}

#[derive(Clone, Debug)]
pub struct ImmutableCancellationHandle {
    cancelled: Arc<AtomicBool>,
}

impl ImmutableCancellationHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Shared cancellation and monotonic deadline state for one assurance operation.
#[derive(Clone, Debug)]
pub struct ImmutableOperationControl {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
}

impl ImmutableOperationControl {
    #[must_use]
    pub fn new(deadline: Option<Instant>) -> (Self, ImmutableCancellationHandle) {
        let cancelled = Arc::new(AtomicBool::new(false));
        (
            Self {
                cancelled: Arc::clone(&cancelled),
                deadline,
            },
            ImmutableCancellationHandle { cancelled },
        )
    }

    #[must_use]
    pub fn unlimited() -> Self {
        Self::new(None).0
    }

    pub fn check(&self) -> Result<(), ConditionalSourceError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(ConditionalSourceError::Cancelled);
        }
        if self.deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(ConditionalSourceError::DeadlineExceeded);
        }
        Ok(())
    }
}

/// Random-access adapter bound to one strong source version for its entire lifetime.
///
/// Successful bytes are copied to the caller only after version, range, total-length,
/// cancellation, and deadline checks pass. A failed operation must be restarted by constructing a
/// new adapter from newly acquired metadata; accepted bytes from the old operation are not reusable
/// assurance state.
#[derive(Debug)]
pub struct ConditionalReadAt<C> {
    client: C,
    control: ImmutableOperationControl,
    version: StrongVersionToken,
    length: u64,
    accepted_bytes: u64,
    requests: u64,
}

impl<C: ConditionalRangeClient> ConditionalReadAt<C> {
    pub fn new(
        mut client: C,
        control: ImmutableOperationControl,
    ) -> Result<Self, ConditionalSourceError> {
        control.check()?;
        let metadata = client.metadata()?;
        control.check()?;
        let version = StrongVersionToken::parse(metadata.version)?;
        Ok(Self {
            client,
            control,
            version,
            length: metadata.length,
            accepted_bytes: 0,
            requests: 0,
        })
    }

    #[must_use]
    pub fn version(&self) -> &StrongVersionToken {
        &self.version
    }

    #[must_use]
    pub fn accepted_bytes(&self) -> u64 {
        self.accepted_bytes
    }

    #[must_use]
    pub fn requests(&self) -> u64 {
        self.requests
    }

    pub fn into_inner(self) -> C {
        self.client
    }

    pub fn read_exact_controlled(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ConditionalSourceError> {
        self.control.check()?;
        let length = u64::try_from(buffer.len())
            .map_err(|_| ConditionalSourceError::Limit("range length"))?;
        let end = offset
            .checked_add(length)
            .ok_or(ConditionalSourceError::Protocol("range overflow"))?;
        if end > self.length {
            return Err(ConditionalSourceError::Protocol("range outside object"));
        }

        self.requests = self
            .requests
            .checked_add(1)
            .ok_or(ConditionalSourceError::Limit("request count"))?;
        let response = self
            .client
            .read_range_if_match(&self.version, offset, buffer.len())?;
        self.control.check()?;

        let response_version = StrongVersionToken::parse(response.version)?;
        if response_version != self.version {
            return Err(ConditionalSourceError::Protocol("response version token"));
        }
        if response.offset != offset || response.total_length != self.length {
            return Err(ConditionalSourceError::Protocol("content range"));
        }
        if response.body.len() != buffer.len() {
            return Err(ConditionalSourceError::Protocol("short response"));
        }

        buffer.copy_from_slice(&response.body);
        self.accepted_bytes = self
            .accepted_bytes
            .checked_add(length)
            .ok_or(ConditionalSourceError::Limit("accepted bytes"))?;
        Ok(())
    }
}

fn map_conditional_error(error: ConditionalSourceError) -> ImmutableSourceError {
    match error {
        ConditionalSourceError::Client(label)
        | ConditionalSourceError::RetryableClient(label) => ImmutableSourceError::Io(label),
        ConditionalSourceError::InvalidVersionToken => {
            ImmutableSourceError::Io("strong version token")
        }
        ConditionalSourceError::VersionChanged => ImmutableSourceError::Io("version changed"),
        ConditionalSourceError::Protocol(label) => ImmutableSourceError::Io(label),
        ConditionalSourceError::Cancelled => ImmutableSourceError::Io("cancelled"),
        ConditionalSourceError::DeadlineExceeded => ImmutableSourceError::Io("deadline"),
        ConditionalSourceError::Limit(label) => ImmutableSourceError::Limit(label),
    }
}

impl<C: ConditionalRangeClient> ImmutableReadAt for ConditionalReadAt<C> {
    fn len(&mut self) -> Result<u64, ImmutableSourceError> {
        self.control.check().map_err(map_conditional_error)?;
        Ok(self.length)
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), ImmutableSourceError> {
        self.read_exact_controlled(offset, buffer)
            .map_err(map_conditional_error)
    }
}
