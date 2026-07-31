use std::sync::{Arc, Mutex};

use ucof_experiments::immutable_successor::{
    ConditionalObjectMetadata, ConditionalRangeClient, ConditionalRangeResponse,
    ConditionalReadAt, ConditionalRetryPolicy, ConditionalSourceError, ImmutableOperationControl,
    StrongVersionToken,
};

#[derive(Clone, Debug)]
struct RetryStore {
    state: Arc<Mutex<RetryState>>,
}

#[derive(Debug)]
struct RetryState {
    bytes: Vec<u8>,
    version: String,
    metadata_transient_failures: usize,
    range_transient_failures: usize,
    terminal_range_failure: bool,
    version_changed: bool,
}

impl RetryStore {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            state: Arc::new(Mutex::new(RetryState {
                bytes,
                version: "\"version-a\"".to_owned(),
                metadata_transient_failures: 0,
                range_transient_failures: 0,
                terminal_range_failure: false,
                version_changed: false,
            })),
        }
    }

    fn metadata_failures(&self, count: usize) {
        self.state
            .lock()
            .expect("store lock")
            .metadata_transient_failures = count;
    }

    fn range_failures(&self, count: usize) {
        self.state
            .lock()
            .expect("store lock")
            .range_transient_failures = count;
    }

    fn terminal_range_failure(&self) {
        self.state
            .lock()
            .expect("store lock")
            .terminal_range_failure = true;
    }

    fn change_version(&self) {
        self.state.lock().expect("store lock").version_changed = true;
    }
}

impl ConditionalRangeClient for RetryStore {
    fn metadata(&mut self) -> Result<ConditionalObjectMetadata, ConditionalSourceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ConditionalSourceError::Client("lock"))?;
        if state.metadata_transient_failures > 0 {
            state.metadata_transient_failures -= 1;
            return Err(ConditionalSourceError::RetryableClient("metadata timeout"));
        }
        Ok(ConditionalObjectMetadata {
            length: u64::try_from(state.bytes.len())
                .map_err(|_| ConditionalSourceError::Limit("length"))?,
            version: state.version.clone(),
        })
    }

    fn read_range_if_match(
        &mut self,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
    ) -> Result<ConditionalRangeResponse, ConditionalSourceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ConditionalSourceError::Client("lock"))?;
        if state.version_changed || expected.as_str() != state.version {
            return Err(ConditionalSourceError::VersionChanged);
        }
        if state.terminal_range_failure {
            return Err(ConditionalSourceError::Client("authorization"));
        }
        if state.range_transient_failures > 0 {
            state.range_transient_failures -= 1;
            return Err(ConditionalSourceError::RetryableClient("range timeout"));
        }
        let start =
            usize::try_from(offset).map_err(|_| ConditionalSourceError::Protocol("offset"))?;
        let end = start
            .checked_add(length)
            .ok_or(ConditionalSourceError::Protocol("range"))?;
        let body = state
            .bytes
            .get(start..end)
            .ok_or(ConditionalSourceError::Protocol("range"))?
            .to_vec();
        Ok(ConditionalRangeResponse {
            version: state.version.clone(),
            offset,
            total_length: u64::try_from(state.bytes.len())
                .map_err(|_| ConditionalSourceError::Limit("length"))?,
            body,
        })
    }
}

#[test]
fn retries_metadata_and_ranges_under_one_attempt_budget() {
    let store = RetryStore::new((0_u8..16).collect());
    store.metadata_failures(2);
    store.range_failures(1);
    let policy = ConditionalRetryPolicy::new(5).expect("retry policy");
    let mut source = ConditionalReadAt::new_with_retry(
        store,
        ImmutableOperationControl::unlimited(),
        policy,
    )
    .expect("metadata succeeds on third attempt");
    assert_eq!(source.transport_attempts(), 3);

    let mut bytes = [0_u8; 4];
    source
        .read_exact_controlled(4, &mut bytes)
        .expect("range succeeds on second attempt");
    assert_eq!(bytes, [4, 5, 6, 7]);
    assert_eq!(source.transport_attempts(), 5);
    assert_eq!(source.requests(), 1);
    assert_eq!(source.accepted_bytes(), 4);
}

#[test]
fn exhaustion_is_fail_closed_and_does_not_copy_partial_bytes() {
    let store = RetryStore::new(vec![7_u8; 16]);
    store.range_failures(4);
    let policy = ConditionalRetryPolicy::new(3).expect("retry policy");
    let mut source = ConditionalReadAt::new_with_retry(
        store,
        ImmutableOperationControl::unlimited(),
        policy,
    )
    .expect("metadata consumes one attempt");
    let mut bytes = [0_u8; 4];
    assert_eq!(
        source.read_exact_controlled(0, &mut bytes),
        Err(ConditionalSourceError::Limit("transport attempts"))
    );
    assert_eq!(source.transport_attempts(), 3);
    assert_eq!(source.requests(), 1);
    assert_eq!(source.accepted_bytes(), 0);
    assert_eq!(bytes, [0_u8; 4]);
}

#[test]
fn terminal_failures_and_version_changes_are_never_retried() {
    let store = RetryStore::new(vec![3_u8; 16]);
    let mut source = ConditionalReadAt::new_with_retry(
        store.clone(),
        ImmutableOperationControl::unlimited(),
        ConditionalRetryPolicy::new(8).expect("retry policy"),
    )
    .expect("metadata");
    store.terminal_range_failure();
    assert_eq!(
        source.read_exact_controlled(0, &mut [0_u8; 4]),
        Err(ConditionalSourceError::Client("authorization"))
    );
    assert_eq!(source.transport_attempts(), 2);

    let store = RetryStore::new(vec![3_u8; 16]);
    let mut source = ConditionalReadAt::new_with_retry(
        store.clone(),
        ImmutableOperationControl::unlimited(),
        ConditionalRetryPolicy::new(8).expect("retry policy"),
    )
    .expect("metadata");
    store.change_version();
    assert_eq!(
        source.read_exact_controlled(0, &mut [0_u8; 4]),
        Err(ConditionalSourceError::VersionChanged)
    );
    assert_eq!(source.transport_attempts(), 2);
}

#[test]
fn cancellation_stops_before_consuming_another_attempt() {
    let store = RetryStore::new(vec![5_u8; 16]);
    let (control, cancellation) = ImmutableOperationControl::new(None);
    let mut source = ConditionalReadAt::new_with_retry(
        store,
        control,
        ConditionalRetryPolicy::new(8).expect("retry policy"),
    )
    .expect("metadata");
    assert_eq!(source.transport_attempts(), 1);
    cancellation.cancel();
    assert_eq!(
        source.read_exact_controlled(0, &mut [0_u8; 4]),
        Err(ConditionalSourceError::Cancelled)
    );
    assert_eq!(source.transport_attempts(), 1);
}

#[test]
fn zero_attempt_policy_is_rejected() {
    assert_eq!(
        ConditionalRetryPolicy::new(0),
        Err(ConditionalSourceError::Limit("transport attempts"))
    );
}
