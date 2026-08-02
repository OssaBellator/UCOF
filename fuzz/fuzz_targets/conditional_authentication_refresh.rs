#![no_main]

use std::collections::VecDeque;

use libfuzzer_sys::fuzz_target;
use ucof_experiments::immutable_successor::{
    execute_conditional_http_with_refresh, ConditionalAuthenticationPolicy,
    ConditionalAuthenticationRefresher, ConditionalHttpDecision, ConditionalHttpExchange,
    ConditionalHttpRequest, ConditionalHttpResponseHead, ConditionalSourceError,
    ImmutableCancellationHandle, ImmutableOperationControl,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResponseKind {
    Unauthorized,
    Success,
    Forbidden,
    Retryable,
    TransportFailure,
}

fn response(kind: ResponseKind) -> Result<ConditionalHttpResponseHead, ConditionalSourceError> {
    match kind {
        ResponseKind::Unauthorized => Ok(ConditionalHttpResponseHead {
            status: 401,
            version: None,
            content_length: None,
            content_range: None,
            body_length: 0,
            retry_after_millis: None,
        }),
        ResponseKind::Success => Ok(ConditionalHttpResponseHead {
            status: 200,
            version: Some("\"v1\"".into()),
            content_length: Some(7),
            content_range: None,
            body_length: 0,
            retry_after_millis: None,
        }),
        ResponseKind::Forbidden => Ok(ConditionalHttpResponseHead {
            status: 403,
            version: None,
            content_length: None,
            content_range: None,
            body_length: 0,
            retry_after_millis: None,
        }),
        ResponseKind::Retryable => Ok(ConditionalHttpResponseHead {
            status: 503,
            version: None,
            content_length: None,
            content_range: None,
            body_length: 0,
            retry_after_millis: Some(1),
        }),
        ResponseKind::TransportFailure => Err(ConditionalSourceError::RetryableClient(
            "injected transport failure",
        )),
    }
}

struct ScriptedExchange {
    responses: VecDeque<Result<ConditionalHttpResponseHead, ConditionalSourceError>>,
    calls: usize,
    cancel_at: Option<usize>,
    cancellation: ImmutableCancellationHandle,
}

impl ConditionalHttpExchange for ScriptedExchange {
    fn exchange(
        &mut self,
        _request: &ConditionalHttpRequest,
    ) -> Result<ConditionalHttpResponseHead, ConditionalSourceError> {
        self.calls += 1;
        if self.cancel_at == Some(self.calls) {
            self.cancellation.cancel();
        }
        self.responses
            .pop_front()
            .expect("executor performs at most two exchanges")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefreshMode {
    Success,
    Failure,
    Cancel,
}

struct ScriptedRefresher {
    mode: RefreshMode,
    calls: usize,
    cancellation: ImmutableCancellationHandle,
}

impl ConditionalAuthenticationRefresher for ScriptedRefresher {
    fn refresh_authentication(&mut self) -> Result<(), ConditionalSourceError> {
        self.calls += 1;
        match self.mode {
            RefreshMode::Success => Ok(()),
            RefreshMode::Failure => Err(ConditionalSourceError::Client(
                "injected authentication refresh failure",
            )),
            RefreshMode::Cancel => {
                self.cancellation.cancel();
                Ok(())
            }
        }
    }
}

fn first_kind(byte: u8) -> ResponseKind {
    match byte % 5 {
        0 => ResponseKind::Unauthorized,
        1 => ResponseKind::Success,
        2 => ResponseKind::Forbidden,
        3 => ResponseKind::Retryable,
        _ => ResponseKind::TransportFailure,
    }
}

fn replay_kind(byte: u8) -> ResponseKind {
    match byte % 4 {
        0 => ResponseKind::Success,
        1 => ResponseKind::Unauthorized,
        2 => ResponseKind::Forbidden,
        _ => ResponseKind::Retryable,
    }
}

fuzz_target!(|data: &[u8]| {
    let byte = |index: usize| data.get(index).copied().unwrap_or(index as u8);
    let authentication = if byte(0) & 1 == 0 {
        ConditionalAuthenticationPolicy::Terminal
    } else {
        ConditionalAuthenticationPolicy::OneRefreshPermitted
    };
    let first = first_kind(byte(1));
    let replay = replay_kind(byte(2));
    let refresh_mode = match byte(3) % 3 {
        0 => RefreshMode::Success,
        1 => RefreshMode::Failure,
        _ => RefreshMode::Cancel,
    };
    let cancel_at = match byte(4) % 3 {
        0 => None,
        1 => Some(1),
        _ => Some(2),
    };

    let (control, handle) = ImmutableOperationControl::new(None);
    let mut exchange = ScriptedExchange {
        responses: [response(first), response(replay)].into_iter().collect(),
        calls: 0,
        cancel_at,
        cancellation: handle.clone(),
    };
    let mut refresher = ScriptedRefresher {
        mode: refresh_mode,
        calls: 0,
        cancellation: handle,
    };

    let result = execute_conditional_http_with_refresh(
        &ConditionalHttpRequest::Metadata,
        &mut exchange,
        &mut refresher,
        &control,
        authentication,
    );

    assert!(exchange.calls <= 2);
    assert!(refresher.calls <= 1);
    if authentication == ConditionalAuthenticationPolicy::Terminal {
        assert_eq!(refresher.calls, 0);
        assert_eq!(exchange.calls, 1);
    }
    if refresher.calls == 1 {
        assert_eq!(authentication, ConditionalAuthenticationPolicy::OneRefreshPermitted);
        assert_eq!(first, ResponseKind::Unauthorized);
        assert_eq!(exchange.calls, 1 + usize::from(refresh_mode == RefreshMode::Success && cancel_at != Some(1)));
    }
    if exchange.calls == 2 {
        assert_eq!(authentication, ConditionalAuthenticationPolicy::OneRefreshPermitted);
        assert_eq!(first, ResponseKind::Unauthorized);
        assert_eq!(refresher.calls, 1);
        assert_eq!(refresh_mode, RefreshMode::Success);
    }

    if let Ok(execution) = result {
        assert_eq!(
            execution.report.transport_attempts,
            u64::try_from(exchange.calls).expect("bounded exchanges")
        );
        assert_eq!(
            execution.report.refresh_attempts,
            u64::try_from(refresher.calls).expect("bounded refreshes")
        );
        assert_ne!(
            execution.decision,
            ConditionalHttpDecision::RefreshAuthentication
        );
        if exchange.calls == 2 && replay == ResponseKind::Unauthorized {
            assert_eq!(
                execution.decision,
                ConditionalHttpDecision::Fail(ConditionalSourceError::Client("http unauthorized"))
            );
        }
    }
});
