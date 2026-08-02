use std::collections::VecDeque;

/// Adapter-neutral execution contract for one HTTP-style conditional request.
pub trait ConditionalHttpExchange {
    fn exchange(
        &mut self,
        request: &ConditionalHttpRequest,
    ) -> Result<ConditionalHttpResponseHead, ConditionalSourceError>;
}

/// Application-owned authentication refresh operation.
///
/// Implementations may replace credentials or session state, but this contract grants no provider
/// policy and does not authorize a second refresh.
pub trait ConditionalAuthenticationRefresher {
    fn refresh_authentication(&mut self) -> Result<(), ConditionalSourceError>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConditionalAuthenticationRefreshReport {
    pub transport_attempts: u64,
    pub refresh_attempts: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionalHttpExecution {
    pub decision: ConditionalHttpDecision,
    pub report: ConditionalAuthenticationRefreshReport,
}

/// Executes one conditional HTTP-style request with at most one explicitly authorized refresh.
///
/// The first response is classified with the caller's authentication policy. When and only when it
/// yields [`ConditionalHttpDecision::RefreshAuthentication`], the application-owned refresher is
/// invoked once and the same request is replayed once. The replay is classified with terminal
/// authentication policy, so a second 401 cannot trigger another refresh. Cancellation and the
/// monotonic operation deadline are checked before and after every exchange and refresh call.
/// Transport errors, refresh errors, cancellation, and deadline expiry return immediately; this
/// function does not plan waits or consume backoff state.
pub fn execute_conditional_http_with_refresh<E, R>(
    request: &ConditionalHttpRequest,
    exchange: &mut E,
    refresher: &mut R,
    control: &ImmutableOperationControl,
    authentication: ConditionalAuthenticationPolicy,
) -> Result<ConditionalHttpExecution, ConditionalSourceError>
where
    E: ConditionalHttpExchange,
    R: ConditionalAuthenticationRefresher,
{
    let mut refresh_permitted =
        authentication == ConditionalAuthenticationPolicy::OneRefreshPermitted;
    let mut report = ConditionalAuthenticationRefreshReport::default();

    loop {
        control.check()?;
        report.transport_attempts = report
            .transport_attempts
            .checked_add(1)
            .ok_or(ConditionalSourceError::Limit("transport attempts"))?;
        let response = exchange.exchange(request)?;
        control.check()?;

        let decision = classify_conditional_http_response(
            request,
            &response,
            if refresh_permitted {
                ConditionalAuthenticationPolicy::OneRefreshPermitted
            } else {
                ConditionalAuthenticationPolicy::Terminal
            },
        );
        if decision != ConditionalHttpDecision::RefreshAuthentication {
            return Ok(ConditionalHttpExecution { decision, report });
        }

        if !refresh_permitted {
            return Err(ConditionalSourceError::Client(
                "authentication refresh exhausted",
            ));
        }
        refresh_permitted = false;
        report.refresh_attempts = report
            .refresh_attempts
            .checked_add(1)
            .ok_or(ConditionalSourceError::Limit("authentication refreshes"))?;
        control.check()?;
        refresher.refresh_authentication()?;
        control.check()?;
    }
}

#[cfg(test)]
mod conditional_authentication_tests {
    use super::*;

    struct SequenceExchange {
        responses: VecDeque<Result<ConditionalHttpResponseHead, ConditionalSourceError>>,
        calls: usize,
    }

    impl SequenceExchange {
        fn new(
            responses: impl IntoIterator<
                Item = Result<ConditionalHttpResponseHead, ConditionalSourceError>,
            >,
        ) -> Self {
            Self {
                responses: responses.into_iter().collect(),
                calls: 0,
            }
        }
    }

    impl ConditionalHttpExchange for SequenceExchange {
        fn exchange(
            &mut self,
            _request: &ConditionalHttpRequest,
        ) -> Result<ConditionalHttpResponseHead, ConditionalSourceError> {
            self.calls += 1;
            self.responses
                .pop_front()
                .expect("scripted conditional HTTP response")
        }
    }

    #[derive(Default)]
    struct RecordingRefresher {
        calls: usize,
        error: Option<ConditionalSourceError>,
        cancellation: Option<ImmutableCancellationHandle>,
    }

    impl ConditionalAuthenticationRefresher for RecordingRefresher {
        fn refresh_authentication(&mut self) -> Result<(), ConditionalSourceError> {
            self.calls += 1;
            if let Some(handle) = &self.cancellation {
                handle.cancel();
            }
            if let Some(error) = &self.error {
                return Err(error.clone());
            }
            Ok(())
        }
    }

    fn response(status: u16) -> ConditionalHttpResponseHead {
        ConditionalHttpResponseHead {
            status,
            version: None,
            content_length: None,
            content_range: None,
            body_length: 0,
            retry_after_millis: None,
        }
    }

    fn metadata_success() -> ConditionalHttpResponseHead {
        ConditionalHttpResponseHead {
            status: 200,
            version: Some("\"v1\"".into()),
            content_length: Some(9),
            content_range: None,
            body_length: 0,
            retry_after_millis: None,
        }
    }

    #[test]
    fn one_authorized_refresh_replays_once() {
        let mut exchange = SequenceExchange::new([Ok(response(401)), Ok(metadata_success())]);
        let mut refresher = RecordingRefresher::default();
        let execution = execute_conditional_http_with_refresh(
            &ConditionalHttpRequest::Metadata,
            &mut exchange,
            &mut refresher,
            &ImmutableOperationControl::unlimited(),
            ConditionalAuthenticationPolicy::OneRefreshPermitted,
        )
        .expect("authorized refresh");

        assert_eq!(exchange.calls, 2);
        assert_eq!(refresher.calls, 1);
        assert_eq!(execution.report.transport_attempts, 2);
        assert_eq!(execution.report.refresh_attempts, 1);
        assert_eq!(
            execution.decision,
            ConditionalHttpDecision::AcceptMetadata {
                length: 9,
                version: StrongVersionToken::parse("\"v1\"").expect("version"),
            }
        );
    }

    #[test]
    fn replayed_unauthorized_response_is_terminal() {
        let mut exchange = SequenceExchange::new([Ok(response(401)), Ok(response(401))]);
        let mut refresher = RecordingRefresher::default();
        let execution = execute_conditional_http_with_refresh(
            &ConditionalHttpRequest::Metadata,
            &mut exchange,
            &mut refresher,
            &ImmutableOperationControl::unlimited(),
            ConditionalAuthenticationPolicy::OneRefreshPermitted,
        )
        .expect("terminal replay classification");

        assert_eq!(exchange.calls, 2);
        assert_eq!(refresher.calls, 1);
        assert_eq!(execution.report.transport_attempts, 2);
        assert_eq!(execution.report.refresh_attempts, 1);
        assert_eq!(
            execution.decision,
            ConditionalHttpDecision::Fail(ConditionalSourceError::Client("http unauthorized"))
        );
    }

    #[test]
    fn terminal_policy_never_calls_refresher() {
        let mut exchange = SequenceExchange::new([Ok(response(401))]);
        let mut refresher = RecordingRefresher::default();
        let execution = execute_conditional_http_with_refresh(
            &ConditionalHttpRequest::Metadata,
            &mut exchange,
            &mut refresher,
            &ImmutableOperationControl::unlimited(),
            ConditionalAuthenticationPolicy::Terminal,
        )
        .expect("terminal classification");

        assert_eq!(exchange.calls, 1);
        assert_eq!(refresher.calls, 0);
        assert_eq!(execution.report.transport_attempts, 1);
        assert_eq!(execution.report.refresh_attempts, 0);
        assert_eq!(
            execution.decision,
            ConditionalHttpDecision::Fail(ConditionalSourceError::Client("http unauthorized"))
        );
    }

    #[test]
    fn refresh_and_transport_failures_are_not_hidden() {
        let mut exchange = SequenceExchange::new([Ok(response(401))]);
        let mut refresher = RecordingRefresher {
            error: Some(ConditionalSourceError::Client("refresh failed")),
            ..RecordingRefresher::default()
        };
        assert_eq!(
            execute_conditional_http_with_refresh(
                &ConditionalHttpRequest::Metadata,
                &mut exchange,
                &mut refresher,
                &ImmutableOperationControl::unlimited(),
                ConditionalAuthenticationPolicy::OneRefreshPermitted,
            ),
            Err(ConditionalSourceError::Client("refresh failed"))
        );
        assert_eq!(exchange.calls, 1);
        assert_eq!(refresher.calls, 1);

        let mut transport = SequenceExchange::new([Err(ConditionalSourceError::RetryableClient(
            "injected transport failure",
        ))]);
        let mut unused = RecordingRefresher::default();
        assert_eq!(
            execute_conditional_http_with_refresh(
                &ConditionalHttpRequest::Metadata,
                &mut transport,
                &mut unused,
                &ImmutableOperationControl::unlimited(),
                ConditionalAuthenticationPolicy::OneRefreshPermitted,
            ),
            Err(ConditionalSourceError::RetryableClient(
                "injected transport failure"
            ))
        );
        assert_eq!(transport.calls, 1);
        assert_eq!(unused.calls, 0);
    }

    #[test]
    fn cancellation_after_refresh_prevents_replay() {
        let (control, handle) = ImmutableOperationControl::new(None);
        let mut exchange = SequenceExchange::new([Ok(response(401)), Ok(metadata_success())]);
        let mut refresher = RecordingRefresher {
            cancellation: Some(handle),
            ..RecordingRefresher::default()
        };
        assert_eq!(
            execute_conditional_http_with_refresh(
                &ConditionalHttpRequest::Metadata,
                &mut exchange,
                &mut refresher,
                &control,
                ConditionalAuthenticationPolicy::OneRefreshPermitted,
            ),
            Err(ConditionalSourceError::Cancelled)
        );
        assert_eq!(exchange.calls, 1);
        assert_eq!(refresher.calls, 1);
    }
}
