use std::sync::{Arc, Mutex};
use std::time::Instant;

use ucof_experiments::immutable_successor::{
    append_replacement, build_genesis, evaluate_freshness, validate, validate_source_at,
    ConditionalObjectMetadata, ConditionalRangeClient, ConditionalRangeResponse,
    ConditionalReadAt, ConditionalSourceError, FreshnessDecision, FreshnessError,
    ImmutableLimits, ImmutableObjectInput, ImmutableOperationControl, ImmutableSourceLimits,
    StrongVersionToken, TrustedFreshnessCheckpoint,
};

#[derive(Clone, Copy, Debug)]
enum Fault {
    WrongVersion,
    WrongOffset,
    WrongTotal,
    ShortBody,
}

#[derive(Debug)]
struct State {
    bytes: Vec<u8>,
    version: String,
    fault: Option<Fault>,
    requests: usize,
}

#[derive(Clone, Debug)]
struct FakeStore {
    state: Arc<Mutex<State>>,
}

impl FakeStore {
    fn new(bytes: Vec<u8>, version: &str) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                bytes,
                version: version.to_owned(),
                fault: None,
                requests: 0,
            })),
        }
    }

    fn publish(&self, bytes: Vec<u8>, version: &str) {
        let mut state = self.state.lock().expect("store lock");
        state.bytes = bytes;
        state.version = version.to_owned();
    }

    fn fault(&self, fault: Fault) {
        self.state.lock().expect("store lock").fault = Some(fault);
    }
}

impl ConditionalRangeClient for FakeStore {
    fn metadata(&mut self) -> Result<ConditionalObjectMetadata, ConditionalSourceError> {
        let state = self.state.lock().map_err(|_| ConditionalSourceError::Client("lock"))?;
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
        let mut state = self.state.lock().map_err(|_| ConditionalSourceError::Client("lock"))?;
        state.requests += 1;
        if expected.as_str() != state.version {
            return Err(ConditionalSourceError::VersionChanged);
        }
        let start = usize::try_from(offset)
            .map_err(|_| ConditionalSourceError::Protocol("offset"))?;
        let end = start
            .checked_add(length)
            .ok_or(ConditionalSourceError::Protocol("range"))?;
        let mut body = state
            .bytes
            .get(start..end)
            .ok_or(ConditionalSourceError::Protocol("range"))?
            .to_vec();
        let mut response_offset = offset;
        let mut total_length = u64::try_from(state.bytes.len())
            .map_err(|_| ConditionalSourceError::Limit("length"))?;
        let mut version = state.version.clone();
        match state.fault.take() {
            Some(Fault::WrongVersion) => version = "\"other\"".to_owned(),
            Some(Fault::WrongOffset) => response_offset += 1,
            Some(Fault::WrongTotal) => total_length += 1,
            Some(Fault::ShortBody) => {
                body.pop();
            }
            None => {}
        }
        Ok(ConditionalRangeResponse {
            version,
            offset: response_offset,
            total_length,
            body,
        })
    }
}

fn objects() -> Vec<ImmutableObjectInput> {
    vec![
        ImmutableObjectInput::new(1, 1, b"alpha".to_vec()),
        ImmutableObjectInput::new(2, 1, b"bravo".to_vec()),
        ImmutableObjectInput::new(3, 1, b"charlie".to_vec()),
    ]
}

#[test]
fn conditional_adapter_validates_a_complete_source_under_one_version() {
    let bytes = build_genesis(&objects(), ImmutableLimits::default()).expect("genesis");
    let store = FakeStore::new(bytes, "\"version-a\"");
    let mut source = ConditionalReadAt::new(store, ImmutableOperationControl::unlimited())
        .expect("conditional source");
    let strict = validate_source_at(&mut source, ImmutableSourceLimits::default())
        .expect("strict source validation");
    assert_eq!(strict.report.object_count, 3);
    assert!(source.requests() > 1);
    assert!(source.accepted_bytes() > 0);
}

#[test]
fn version_change_cancellation_and_deadline_are_terminal() {
    let bytes = build_genesis(&objects(), ImmutableLimits::default()).expect("genesis");
    let store = FakeStore::new(bytes.clone(), "\"version-a\"");
    let mut source = ConditionalReadAt::new(store.clone(), ImmutableOperationControl::unlimited())
        .expect("conditional source");
    let mut first = [0_u8; 8];
    source
        .read_exact_controlled(0, &mut first)
        .expect("first range");
    store.publish(bytes, "\"version-b\"");
    let accepted = source.accepted_bytes();
    assert_eq!(
        source.read_exact_controlled(8, &mut [0_u8; 8]),
        Err(ConditionalSourceError::VersionChanged)
    );
    assert_eq!(source.accepted_bytes(), accepted);

    let store = FakeStore::new(vec![0_u8; 16], "\"version-a\"");
    let (control, cancellation) = ImmutableOperationControl::new(None);
    let mut cancelled = ConditionalReadAt::new(store, control).expect("conditional source");
    cancellation.cancel();
    assert_eq!(
        cancelled.read_exact_controlled(0, &mut [0_u8; 4]),
        Err(ConditionalSourceError::Cancelled)
    );
    assert_eq!(cancelled.accepted_bytes(), 0);

    let store = FakeStore::new(vec![0_u8; 16], "\"version-a\"");
    let (control, _) = ImmutableOperationControl::new(Some(Instant::now()));
    assert!(matches!(
        ConditionalReadAt::new(store, control),
        Err(ConditionalSourceError::DeadlineExceeded)
    ));
}

#[test]
fn malformed_response_metadata_is_rejected_before_copying_bytes() {
    for fault in [
        Fault::WrongVersion,
        Fault::WrongOffset,
        Fault::WrongTotal,
        Fault::ShortBody,
    ] {
        let store = FakeStore::new(vec![1_u8; 16], "\"version-a\"");
        store.fault(fault);
        let mut source = ConditionalReadAt::new(store, ImmutableOperationControl::unlimited())
            .expect("conditional source");
        let mut output = [0_u8; 4];
        assert!(matches!(
            source.read_exact_controlled(0, &mut output),
            Err(ConditionalSourceError::Protocol(_))
        ));
        assert_eq!(output, [0_u8; 4]);
        assert_eq!(source.accepted_bytes(), 0);
    }

    assert_eq!(
        StrongVersionToken::parse("W/\"weak\""),
        Err(ConditionalSourceError::InvalidVersionToken)
    );
}

#[test]
fn trusted_checkpoint_detects_rollback_and_same_sequence_forks() {
    let limits = ImmutableLimits::default();
    let genesis = build_genesis(&objects(), limits).expect("genesis");
    let genesis_report = validate(&genesis, limits).expect("genesis report");
    assert_eq!(
        evaluate_freshness(&genesis_report, None),
        Ok(FreshnessDecision::Unpinned)
    );

    let appended = append_replacement(
        &genesis,
        &ImmutableObjectInput::new(1, 2, b"alpha-v2".to_vec()),
        limits,
    )
    .expect("append");
    let appended_report = validate(&appended, limits).expect("append report");
    let genesis_checkpoint = TrustedFreshnessCheckpoint::from(&genesis_report);
    assert_eq!(
        evaluate_freshness(&appended_report, Some(genesis_checkpoint)),
        Ok(FreshnessDecision::Advances {
            previous_sequence: 0,
            candidate_sequence: 1,
        })
    );

    let appended_checkpoint = TrustedFreshnessCheckpoint::from(&appended_report);
    assert_eq!(
        evaluate_freshness(&genesis_report, Some(appended_checkpoint)),
        Err(FreshnessError::Rollback {
            trusted_sequence: 1,
            candidate_sequence: 0,
        })
    );

    let mut fork = appended_report.clone();
    fork.snapshot_digest[0] ^= 1;
    assert!(matches!(
        evaluate_freshness(&fork, Some(appended_checkpoint)),
        Err(FreshnessError::ForkAtTrustedSequence { sequence: 1, .. })
    ));
}
