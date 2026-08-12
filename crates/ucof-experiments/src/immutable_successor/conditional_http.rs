/// Application authorization for one explicit authentication refresh after an HTTP 401 response.
///
/// The classifier never infers refresh authority from credentials, status text, or provider name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConditionalAuthenticationPolicy {
    #[default]
    Terminal,
    OneRefreshPermitted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionalHttpContentRange {
    pub start: u64,
    pub end_inclusive: u64,
    pub total_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalHttpRequest {
    Metadata,
    Range {
        expected_version: StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionalHttpResponseHead {
    pub status: u16,
    pub version: Option<String>,
    pub content_length: Option<u64>,
    pub content_range: Option<ConditionalHttpContentRange>,
    pub body_length: usize,
    pub retry_after_millis: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalHttpDecision {
    AcceptMetadata {
        length: u64,
        version: StrongVersionToken,
    },
    AcceptRange {
        version: StrongVersionToken,
        offset: u64,
        total_length: u64,
        body_length: usize,
    },
    Retry {
        error: ConditionalSourceError,
        server_minimum_millis: Option<u64>,
    },
    RefreshAuthentication,
    Fail(ConditionalSourceError),
}

fn retryable_status_label(status: u16) -> Option<&'static str> {
    match status {
        408 => Some("http request timeout"),
        425 => Some("http too early"),
        429 => Some("http rate limited"),
        500 => Some("http internal server error"),
        502 => Some("http bad gateway"),
        503 => Some("http service unavailable"),
        504 => Some("http gateway timeout"),
        _ => None,
    }
}

fn normalized_retry_after(value: Option<u64>) -> Option<u64> {
    value.filter(|delay| *delay > 0)
}

fn fail(error: ConditionalSourceError) -> ConditionalHttpDecision {
    ConditionalHttpDecision::Fail(error)
}

fn classify_metadata_success(
    response: &ConditionalHttpResponseHead,
) -> ConditionalHttpDecision {
    if response.content_range.is_some() || response.body_length != 0 {
        return fail(ConditionalSourceError::Protocol("metadata response shape"));
    }
    let Some(length) = response.content_length else {
        return fail(ConditionalSourceError::Protocol("metadata length"));
    };
    let Some(version) = response.version.clone() else {
        return fail(ConditionalSourceError::InvalidVersionToken);
    };
    match StrongVersionToken::parse(version) {
        Ok(version) => ConditionalHttpDecision::AcceptMetadata { length, version },
        Err(error) => fail(error),
    }
}

fn classify_range_success(
    response: &ConditionalHttpResponseHead,
    expected_version: &StrongVersionToken,
    offset: u64,
    length: usize,
    total_length: u64,
) -> ConditionalHttpDecision {
    if length == 0 {
        return fail(ConditionalSourceError::Protocol("zero range"));
    }
    let length_u64 = match u64::try_from(length) {
        Ok(value) => value,
        Err(_) => return fail(ConditionalSourceError::Limit("range length")),
    };
    let expected_end = match offset
        .checked_add(length_u64)
        .and_then(|end| end.checked_sub(1))
    {
        Some(value) => value,
        None => return fail(ConditionalSourceError::Protocol("range overflow")),
    };
    if offset
        .checked_add(length_u64)
        .is_none_or(|end| end > total_length)
    {
        return fail(ConditionalSourceError::Protocol("range outside object"));
    }
    if response.content_length != Some(length_u64) || response.body_length != length {
        return fail(ConditionalSourceError::Protocol("partial response length"));
    }
    if response.content_range
        != Some(ConditionalHttpContentRange {
            start: offset,
            end_inclusive: expected_end,
            total_length,
        })
    {
        return fail(ConditionalSourceError::Protocol("content range"));
    }
    let Some(version) = response.version.clone() else {
        return fail(ConditionalSourceError::InvalidVersionToken);
    };
    let version = match StrongVersionToken::parse(version) {
        Ok(version) => version,
        Err(error) => return fail(error),
    };
    if &version != expected_version {
        return fail(ConditionalSourceError::Protocol("response version token"));
    }
    ConditionalHttpDecision::AcceptRange {
        version,
        offset,
        total_length,
        body_length: length,
    }
}

/// Conservatively classifies one HTTP-style metadata or conditional range response.
///
/// Only an explicit small transient-status allowlist is retryable. HTTP 412 is always a terminal
/// version change. Redirects are never followed automatically. HTTP 401 permits an authentication
/// refresh only when the application explicitly supplied refresh authority; HTTP 403 is terminal.
/// Successful range reads require exact 206, strong-version, content-length, content-range, and body
/// length agreement. Provider-specific statuses must be mapped by a maintained adapter rather than
/// guessed by this generic classifier.
#[must_use]
pub fn classify_conditional_http_response(
    request: &ConditionalHttpRequest,
    response: &ConditionalHttpResponseHead,
    authentication: ConditionalAuthenticationPolicy,
) -> ConditionalHttpDecision {
    if let Some(label) = retryable_status_label(response.status) {
        return ConditionalHttpDecision::Retry {
            error: ConditionalSourceError::RetryableClient(label),
            server_minimum_millis: normalized_retry_after(response.retry_after_millis),
        };
    }
    match response.status {
        401 if authentication == ConditionalAuthenticationPolicy::OneRefreshPermitted => {
            return ConditionalHttpDecision::RefreshAuthentication;
        }
        401 => return fail(ConditionalSourceError::Client("http unauthorized")),
        403 => return fail(ConditionalSourceError::Client("http forbidden")),
        404 => return fail(ConditionalSourceError::Client("http object not found")),
        412 => return fail(ConditionalSourceError::VersionChanged),
        416 => return fail(ConditionalSourceError::Client("http range unsatisfiable")),
        301 | 302 | 303 | 307 | 308 => {
            return fail(ConditionalSourceError::Protocol("redirect"));
        }
        _ => {}
    }

    match request {
        ConditionalHttpRequest::Metadata if response.status == 200 => {
            classify_metadata_success(response)
        }
        ConditionalHttpRequest::Range {
            expected_version,
            offset,
            length,
            total_length,
        } if response.status == 206 => classify_range_success(
            response,
            expected_version,
            *offset,
            *length,
            *total_length,
        ),
        ConditionalHttpRequest::Metadata if (200..300).contains(&response.status) => {
            fail(ConditionalSourceError::Protocol("metadata success status"))
        }
        ConditionalHttpRequest::Range { .. } if (200..300).contains(&response.status) => {
            fail(ConditionalSourceError::Protocol("range success status"))
        }
        _ if (400..500).contains(&response.status) => {
            fail(ConditionalSourceError::Client("http client status"))
        }
        _ if (500..600).contains(&response.status) => {
            fail(ConditionalSourceError::Client("http server status"))
        }
        _ => fail(ConditionalSourceError::Protocol("unexpected http status")),
    }
}

/// Plans a bounded delay only for a response already classified as retryable.
///
/// Terminal, successful, and authentication-refresh decisions do not consume retry-delay state.
pub fn plan_conditional_http_retry(
    decision: &ConditionalHttpDecision,
    budget: &mut ConditionalBackoffBudget,
    remaining_deadline_millis: Option<u64>,
) -> Result<ConditionalBackoffDecision, ConditionalSourceError> {
    match decision {
        ConditionalHttpDecision::Retry {
            server_minimum_millis,
            ..
        } => budget.plan_next_delay(*server_minimum_millis, remaining_deadline_millis),
        ConditionalHttpDecision::Fail(error) => Err(error.clone()),
        ConditionalHttpDecision::RefreshAuthentication => {
            Err(ConditionalSourceError::Client("authentication refresh required"))
        }
        ConditionalHttpDecision::AcceptMetadata { .. }
        | ConditionalHttpDecision::AcceptRange { .. } => {
            Err(ConditionalSourceError::Protocol("response does not require retry"))
        }
    }
}

#[cfg(test)]
mod conditional_http_tests {
    use super::*;

    fn version() -> StrongVersionToken {
        StrongVersionToken::parse("\"v1\"").expect("strong version")
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

    #[test]
    fn metadata_requires_exact_shape_and_strong_version() {
        let decision = classify_conditional_http_response(
            &ConditionalHttpRequest::Metadata,
            &ConditionalHttpResponseHead {
                status: 200,
                version: Some("\"v1\"".into()),
                content_length: Some(99),
                content_range: None,
                body_length: 0,
                retry_after_millis: None,
            },
            ConditionalAuthenticationPolicy::Terminal,
        );
        assert_eq!(
            decision,
            ConditionalHttpDecision::AcceptMetadata {
                length: 99,
                version: version()
            }
        );
        let mut weak = response(200);
        weak.version = Some("W/\"v1\"".into());
        weak.content_length = Some(99);
        assert_eq!(
            classify_conditional_http_response(
                &ConditionalHttpRequest::Metadata,
                &weak,
                ConditionalAuthenticationPolicy::Terminal,
            ),
            ConditionalHttpDecision::Fail(ConditionalSourceError::InvalidVersionToken)
        );
    }

    #[test]
    fn range_requires_exact_partial_response_and_version() {
        let request = ConditionalHttpRequest::Range {
            expected_version: version(),
            offset: 10,
            length: 5,
            total_length: 100,
        };
        let exact = ConditionalHttpResponseHead {
            status: 206,
            version: Some("\"v1\"".into()),
            content_length: Some(5),
            content_range: Some(ConditionalHttpContentRange {
                start: 10,
                end_inclusive: 14,
                total_length: 100,
            }),
            body_length: 5,
            retry_after_millis: None,
        };
        assert_eq!(
            classify_conditional_http_response(
                &request,
                &exact,
                ConditionalAuthenticationPolicy::Terminal,
            ),
            ConditionalHttpDecision::AcceptRange {
                version: version(),
                offset: 10,
                total_length: 100,
                body_length: 5
            }
        );
        let mut wrong_range = exact.clone();
        wrong_range.content_range = Some(ConditionalHttpContentRange {
            start: 11,
            end_inclusive: 15,
            total_length: 100,
        });
        assert_eq!(
            classify_conditional_http_response(
                &request,
                &wrong_range,
                ConditionalAuthenticationPolicy::Terminal,
            ),
            ConditionalHttpDecision::Fail(ConditionalSourceError::Protocol("content range"))
        );
        let mut wrong_version = exact;
        wrong_version.version = Some("\"v2\"".into());
        assert_eq!(
            classify_conditional_http_response(
                &request,
                &wrong_version,
                ConditionalAuthenticationPolicy::Terminal,
            ),
            ConditionalHttpDecision::Fail(ConditionalSourceError::Protocol(
                "response version token"
            ))
        );
    }

    #[test]
    fn version_authentication_redirect_and_range_failures_are_terminal() {
        assert_eq!(
            classify_conditional_http_response(
                &ConditionalHttpRequest::Metadata,
                &response(412),
                ConditionalAuthenticationPolicy::Terminal,
            ),
            ConditionalHttpDecision::Fail(ConditionalSourceError::VersionChanged)
        );
        assert_eq!(
            classify_conditional_http_response(
                &ConditionalHttpRequest::Metadata,
                &response(401),
                ConditionalAuthenticationPolicy::OneRefreshPermitted,
            ),
            ConditionalHttpDecision::RefreshAuthentication
        );
        assert_eq!(
            classify_conditional_http_response(
                &ConditionalHttpRequest::Metadata,
                &response(403),
                ConditionalAuthenticationPolicy::OneRefreshPermitted,
            ),
            ConditionalHttpDecision::Fail(ConditionalSourceError::Client("http forbidden"))
        );
        assert_eq!(
            classify_conditional_http_response(
                &ConditionalHttpRequest::Metadata,
                &response(307),
                ConditionalAuthenticationPolicy::Terminal,
            ),
            ConditionalHttpDecision::Fail(ConditionalSourceError::Protocol("redirect"))
        );
        assert_eq!(
            classify_conditional_http_response(
                &ConditionalHttpRequest::Metadata,
                &response(416),
                ConditionalAuthenticationPolicy::Terminal,
            ),
            ConditionalHttpDecision::Fail(ConditionalSourceError::Client(
                "http range unsatisfiable"
            ))
        );
    }

    #[test]
    fn only_explicit_transient_statuses_consume_backoff_budget() {
        let mut transient = response(503);
        transient.retry_after_millis = Some(750);
        let decision = classify_conditional_http_response(
            &ConditionalHttpRequest::Metadata,
            &transient,
            ConditionalAuthenticationPolicy::Terminal,
        );
        assert_eq!(
            decision,
            ConditionalHttpDecision::Retry {
                error: ConditionalSourceError::RetryableClient("http service unavailable"),
                server_minimum_millis: Some(750)
            }
        );
        let mut budget = ConditionalBackoffBudget::new(
            ConditionalBackoffPolicy::new(100, 1_000, 5_000).expect("policy"),
        );
        assert_eq!(
            plan_conditional_http_retry(&decision, &mut budget, Some(2_000))
                .expect("retry plan")
                .delay_millis,
            750
        );
        let terminal = classify_conditional_http_response(
            &ConditionalHttpRequest::Metadata,
            &response(501),
            ConditionalAuthenticationPolicy::Terminal,
        );
        assert_eq!(
            terminal,
            ConditionalHttpDecision::Fail(ConditionalSourceError::Client(
                "http server status"
            ))
        );
        assert!(plan_conditional_http_retry(&terminal, &mut budget, None).is_err());
        assert_eq!(budget.retries_planned(), 1);
    }

    #[test]
    fn full_body_range_success_and_oversized_retry_after_fail_closed() {
        let request = ConditionalHttpRequest::Range {
            expected_version: version(),
            offset: 0,
            length: 5,
            total_length: 5,
        };
        let mut full = response(200);
        full.version = Some("\"v1\"".into());
        full.content_length = Some(5);
        full.body_length = 5;
        assert_eq!(
            classify_conditional_http_response(
                &request,
                &full,
                ConditionalAuthenticationPolicy::Terminal,
            ),
            ConditionalHttpDecision::Fail(ConditionalSourceError::Protocol(
                "range success status"
            ))
        );

        let mut transient = response(429);
        transient.retry_after_millis = Some(1_001);
        let decision = classify_conditional_http_response(
            &ConditionalHttpRequest::Metadata,
            &transient,
            ConditionalAuthenticationPolicy::Terminal,
        );
        let mut budget = ConditionalBackoffBudget::new(
            ConditionalBackoffPolicy::new(100, 1_000, 5_000).expect("policy"),
        );
        assert_eq!(
            plan_conditional_http_retry(&decision, &mut budget, None),
            Err(ConditionalSourceError::Limit("server retry delay"))
        );
        assert_eq!(budget.retries_planned(), 0);
    }
}
