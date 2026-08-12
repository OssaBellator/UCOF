/// Operation-wide transport retry policy for one strong-version assurance operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionalRetryPolicy {
    max_transport_attempts: u64,
}

impl ConditionalRetryPolicy {
    pub fn new(max_transport_attempts: u64) -> Result<Self, ConditionalSourceError> {
        if max_transport_attempts == 0 {
            return Err(ConditionalSourceError::Limit("transport attempts"));
        }
        Ok(Self {
            max_transport_attempts,
        })
    }

    #[must_use]
    pub fn max_transport_attempts(self) -> u64 {
        self.max_transport_attempts
    }
}

impl Default for ConditionalRetryPolicy {
    fn default() -> Self {
        Self {
            max_transport_attempts: 8,
        }
    }
}

/// Bounded retry wrapper for a conditional range client.
///
/// The budget includes metadata and every range attempt across the complete adapter lifetime.
/// Only [`ConditionalSourceError::RetryableClient`] is retried. Version changes, protocol errors,
/// cancellation, deadlines, limits, and terminal client failures return immediately.
#[derive(Debug)]
pub struct RetryingConditionalClient<C> {
    client: C,
    control: ImmutableOperationControl,
    policy: ConditionalRetryPolicy,
    transport_attempts: u64,
}

impl<C> RetryingConditionalClient<C> {
    #[must_use]
    pub fn new(
        client: C,
        control: ImmutableOperationControl,
        policy: ConditionalRetryPolicy,
    ) -> Self {
        Self {
            client,
            control,
            policy,
            transport_attempts: 0,
        }
    }

    #[must_use]
    pub fn transport_attempts(&self) -> u64 {
        self.transport_attempts
    }

    pub fn into_inner(self) -> C {
        self.client
    }

    fn begin_attempt(&mut self) -> Result<(), ConditionalSourceError> {
        self.control.check()?;
        if self.transport_attempts >= self.policy.max_transport_attempts {
            return Err(ConditionalSourceError::Limit("transport attempts"));
        }
        self.transport_attempts = self
            .transport_attempts
            .checked_add(1)
            .ok_or(ConditionalSourceError::Limit("transport attempts"))?;
        Ok(())
    }

    fn retry_or_return<T>(
        &self,
        result: Result<T, ConditionalSourceError>,
    ) -> Result<Option<T>, ConditionalSourceError> {
        self.control.check()?;
        match result {
            Ok(value) => Ok(Some(value)),
            Err(ConditionalSourceError::RetryableClient(_))
                if self.transport_attempts < self.policy.max_transport_attempts =>
            {
                Ok(None)
            }
            Err(ConditionalSourceError::RetryableClient(_)) => {
                Err(ConditionalSourceError::Limit("transport attempts"))
            }
            Err(error) => Err(error),
        }
    }
}

impl<C: ConditionalRangeClient> ConditionalRangeClient for RetryingConditionalClient<C> {
    fn metadata(&mut self) -> Result<ConditionalObjectMetadata, ConditionalSourceError> {
        loop {
            self.begin_attempt()?;
            let result = self.client.metadata();
            if let Some(metadata) = self.retry_or_return(result)? {
                return Ok(metadata);
            }
        }
    }

    fn read_range_if_match(
        &mut self,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
    ) -> Result<ConditionalRangeResponse, ConditionalSourceError> {
        loop {
            self.begin_attempt()?;
            let result = self.client.read_range_if_match(expected, offset, length);
            if let Some(response) = self.retry_or_return(result)? {
                return Ok(response);
            }
        }
    }
}

impl<C: ConditionalRangeClient> ConditionalReadAt<RetryingConditionalClient<C>> {
    /// Constructs a strong-version adapter with one operation-wide bounded transport-attempt budget.
    pub fn new_with_retry(
        client: C,
        control: ImmutableOperationControl,
        policy: ConditionalRetryPolicy,
    ) -> Result<Self, ConditionalSourceError> {
        let retrying = RetryingConditionalClient::new(client, control.clone(), policy);
        Self::new(retrying, control)
    }

    #[must_use]
    pub fn transport_attempts(&self) -> u64 {
        self.client.transport_attempts()
    }
}
