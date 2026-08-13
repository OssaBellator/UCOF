/// One classified concrete HTTP transport attempt before retry/authentication policy is applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReqwestConditionalAttempt<T> {
    Accepted(T),
    Retry {
        error: ConditionalSourceError,
        server_minimum_millis: Option<u64>,
    },
    RefreshAuthentication,
}

impl ReqwestConditionalRangeClient {
    /// Execute one metadata HTTP attempt while preserving retry/authentication classification.
    pub async fn metadata_attempt(
        &self,
        authentication: ConditionalAuthenticationPolicy,
    ) -> Result<ReqwestConditionalAttempt<ConditionalObjectMetadata>, ConditionalSourceError> {
        self.control.check()?;
        let request = self
            .client
            .head(self.url.clone())
            .header(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
        let response = await_reqwest_controlled(&self.control, request.send()).await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        reject_non_identity_encoding(&headers)?;
        let head = response_head(status, &headers, 0)?;
        match classify_conditional_http_response(
            &ConditionalHttpRequest::Metadata,
            &head,
            authentication,
        ) {
            ConditionalHttpDecision::AcceptMetadata { length, version } => {
                Ok(ReqwestConditionalAttempt::Accepted(
                    ConditionalObjectMetadata {
                        length,
                        version: version.as_str().to_owned(),
                    },
                ))
            }
            ConditionalHttpDecision::Retry {
                error,
                server_minimum_millis,
            } => Ok(ReqwestConditionalAttempt::Retry {
                error,
                server_minimum_millis,
            }),
            ConditionalHttpDecision::RefreshAuthentication => {
                Ok(ReqwestConditionalAttempt::RefreshAuthentication)
            }
            ConditionalHttpDecision::Fail(error) => Err(error),
            ConditionalHttpDecision::AcceptRange { .. } => Err(ConditionalSourceError::Protocol(
                "metadata classified as range",
            )),
        }
    }

    /// Execute one conditional range HTTP attempt while preserving retry/authentication
    /// classification and returning body bytes only for a fully accepted 206 response.
    pub async fn read_range_attempt(
        &self,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
        authentication: ConditionalAuthenticationPolicy,
    ) -> Result<ReqwestConditionalAttempt<ConditionalRangeResponse>, ConditionalSourceError> {
        self.control.check()?;
        if length == 0 {
            return Err(ConditionalSourceError::Protocol("zero range"));
        }
        let length_u64 =
            u64::try_from(length).map_err(|_| ConditionalSourceError::Limit("range length"))?;
        let end_exclusive = offset
            .checked_add(length_u64)
            .ok_or(ConditionalSourceError::Protocol("range overflow"))?;
        if end_exclusive > total_length {
            return Err(ConditionalSourceError::Protocol("range outside object"));
        }
        let end_inclusive = end_exclusive
            .checked_sub(1)
            .ok_or(ConditionalSourceError::Protocol("zero range"))?;
        let if_match = HeaderValue::from_str(expected.as_str())
            .map_err(|_| ConditionalSourceError::InvalidVersionToken)?;
        let range = HeaderValue::from_str(&format!("bytes={offset}-{end_inclusive}"))
            .map_err(|_| ConditionalSourceError::Protocol("range header"))?;
        let request_description = ConditionalHttpRequest::Range {
            expected_version: expected.clone(),
            offset,
            length,
            total_length,
        };
        let request = self
            .client
            .get(self.url.clone())
            .header(ACCEPT_ENCODING, HeaderValue::from_static("identity"))
            .header(IF_MATCH, if_match)
            .header(RANGE, range);
        let response = await_reqwest_controlled(&self.control, request.send()).await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        reject_non_identity_encoding(&headers)?;

        if status != 206 {
            let head = response_head(status, &headers, 0)?;
            return match classify_conditional_http_response(
                &request_description,
                &head,
                authentication,
            ) {
                ConditionalHttpDecision::Retry {
                    error,
                    server_minimum_millis,
                } => Ok(ReqwestConditionalAttempt::Retry {
                    error,
                    server_minimum_millis,
                }),
                ConditionalHttpDecision::RefreshAuthentication => {
                    Ok(ReqwestConditionalAttempt::RefreshAuthentication)
                }
                ConditionalHttpDecision::Fail(error) => Err(error),
                ConditionalHttpDecision::AcceptMetadata { .. }
                | ConditionalHttpDecision::AcceptRange { .. } => Err(
                    ConditionalSourceError::Protocol("non-partial response accepted"),
                ),
            };
        }

        let body = await_reqwest_controlled(&self.control, response.bytes()).await?;
        let head = response_head(status, &headers, body.len())?;
        match classify_conditional_http_response(
            &request_description,
            &head,
            authentication,
        ) {
            ConditionalHttpDecision::AcceptRange {
                version,
                offset,
                total_length,
                ..
            } => Ok(ReqwestConditionalAttempt::Accepted(
                ConditionalRangeResponse {
                    version: version.as_str().to_owned(),
                    offset,
                    total_length,
                    body: body.to_vec(),
                },
            )),
            ConditionalHttpDecision::Retry {
                error,
                server_minimum_millis,
            } => Ok(ReqwestConditionalAttempt::Retry {
                error,
                server_minimum_millis,
            }),
            ConditionalHttpDecision::RefreshAuthentication => {
                Ok(ReqwestConditionalAttempt::RefreshAuthentication)
            }
            ConditionalHttpDecision::Fail(error) => Err(error),
            ConditionalHttpDecision::AcceptMetadata { .. } => Err(ConditionalSourceError::Protocol(
                "range classified as metadata",
            )),
        }
    }
}

/// Async operation-wide retry/backoff wrapper around the single-attempt Reqwest transport.
///
/// Metadata and all range calls share one transport-attempt counter and one cumulative backoff
/// budget. Reqwest itself remains configured for zero internal retries.
#[derive(Clone, Debug)]
pub struct AsyncRetryingReqwestConditionalClient {
    client: ReqwestConditionalRangeClient,
    control: ImmutableOperationControl,
    retry_policy: ConditionalRetryPolicy,
    backoff: ConditionalBackoffBudget,
    transport_attempts: u64,
}

impl AsyncRetryingReqwestConditionalClient {
    #[must_use]
    pub fn new(
        client: ReqwestConditionalRangeClient,
        retry_policy: ConditionalRetryPolicy,
        backoff_policy: ConditionalBackoffPolicy,
    ) -> Self {
        let control = client.control.clone();
        Self {
            client,
            control,
            retry_policy,
            backoff: ConditionalBackoffBudget::new(backoff_policy),
            transport_attempts: 0,
        }
    }

    #[must_use]
    pub fn transport_attempts(&self) -> u64 {
        self.transport_attempts
    }

    #[must_use]
    pub fn retries_planned(&self) -> u32 {
        self.backoff.retries_planned()
    }

    #[must_use]
    pub fn cumulative_delay_millis(&self) -> u64 {
        self.backoff.cumulative_delay_millis()
    }

    pub fn into_inner(self) -> ReqwestConditionalRangeClient {
        self.client
    }

    fn begin_attempt(&mut self) -> Result<(), ConditionalSourceError> {
        self.control.check()?;
        if self.transport_attempts >= self.retry_policy.max_transport_attempts() {
            return Err(ConditionalSourceError::Limit("transport attempts"));
        }
        self.transport_attempts = self
            .transport_attempts
            .checked_add(1)
            .ok_or(ConditionalSourceError::Limit("transport attempts"))?;
        Ok(())
    }

    async fn retry_after_decision(
        &mut self,
        decision: ConditionalHttpDecision,
    ) -> Result<(), ConditionalSourceError> {
        if self.transport_attempts >= self.retry_policy.max_transport_attempts() {
            return Err(ConditionalSourceError::Limit("transport attempts"));
        }
        let remaining = remaining_deadline_millis(&self.control)?;
        let wait = plan_conditional_http_retry(&decision, &mut self.backoff, remaining)?;
        await_controlled_retry_delay(&self.control, wait.delay_millis).await
    }

    async fn retry_after_transport_error(
        &mut self,
        error: ConditionalSourceError,
    ) -> Result<(), ConditionalSourceError> {
        match error {
            ConditionalSourceError::RetryableClient(_) => {
                self.retry_after_decision(ConditionalHttpDecision::Retry {
                    error,
                    server_minimum_millis: None,
                })
                .await
            }
            other => Err(other),
        }
    }

    pub async fn metadata(&mut self) -> Result<ConditionalObjectMetadata, ConditionalSourceError> {
        loop {
            self.begin_attempt()?;
            match self
                .client
                .metadata_attempt(ConditionalAuthenticationPolicy::Terminal)
                .await
            {
                Ok(ReqwestConditionalAttempt::Accepted(metadata)) => return Ok(metadata),
                Ok(ReqwestConditionalAttempt::Retry {
                    error,
                    server_minimum_millis,
                }) => {
                    self.retry_after_decision(ConditionalHttpDecision::Retry {
                        error,
                        server_minimum_millis,
                    })
                    .await?;
                }
                Ok(ReqwestConditionalAttempt::RefreshAuthentication) => {
                    return Err(ConditionalSourceError::Client(
                        "authentication refresh unavailable",
                    ));
                }
                Err(error) => self.retry_after_transport_error(error).await?,
            }
        }
    }

    pub async fn read_range_if_match(
        &mut self,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    ) -> Result<ConditionalRangeResponse, ConditionalSourceError> {
        loop {
            self.begin_attempt()?;
            match self
                .client
                .read_range_attempt(
                    expected,
                    offset,
                    length,
                    total_length,
                    ConditionalAuthenticationPolicy::Terminal,
                )
                .await
            {
                Ok(ReqwestConditionalAttempt::Accepted(response)) => return Ok(response),
                Ok(ReqwestConditionalAttempt::Retry {
                    error,
                    server_minimum_millis,
                }) => {
                    self.retry_after_decision(ConditionalHttpDecision::Retry {
                        error,
                        server_minimum_millis,
                    })
                    .await?;
                }
                Ok(ReqwestConditionalAttempt::RefreshAuthentication) => {
                    return Err(ConditionalSourceError::Client(
                        "authentication refresh unavailable",
                    ));
                }
                Err(error) => self.retry_after_transport_error(error).await?,
            }
        }
    }
}

fn remaining_deadline_millis(
    control: &ImmutableOperationControl,
) -> Result<Option<u64>, ConditionalSourceError> {
    control.check()?;
    let Some(deadline) = control.deadline else {
        return Ok(None);
    };
    let now = std::time::Instant::now();
    if now >= deadline {
        return Err(ConditionalSourceError::DeadlineExceeded);
    }
    let remaining = deadline.duration_since(now).as_millis();
    Ok(Some(u64::try_from(remaining).unwrap_or(u64::MAX)))
}

async fn await_controlled_retry_delay(
    control: &ImmutableOperationControl,
    delay_millis: u64,
) -> Result<(), ConditionalSourceError> {
    control.check()?;
    let sleep = tokio::time::sleep(std::time::Duration::from_millis(delay_millis));
    tokio::pin!(sleep);
    tokio::select! {
        biased;
        error = wait_for_control_failure(control) => Err(error),
        _ = &mut sleep => {
            control.check()?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod conditional_reqwest_async_retry_tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let read = socket.read(&mut chunk).await.expect("read request");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(bytes).expect("request text")
    }

    async fn scripted_server(
        responses: Vec<&'static [u8]>,
    ) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let handle = tokio::spawn(async move {
            let mut responses: VecDeque<_> = responses.into();
            while let Some(response) = responses.pop_front() {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let request = read_request(&mut socket).await;
                recorded.lock().expect("requests").push(request);
                socket.write_all(response).await.expect("write response");
                socket.shutdown().await.expect("shutdown");
            }
        });
        (format!("http://{address}/object"), requests, handle)
    }

    fn retry_policy(attempts: u64) -> ConditionalRetryPolicy {
        ConditionalRetryPolicy::new(attempts).expect("retry policy")
    }

    fn backoff_policy(base: u64, max: u64, cumulative: u64) -> ConditionalBackoffPolicy {
        ConditionalBackoffPolicy::new(base, max, cumulative).expect("backoff policy")
    }

    #[tokio::test]
    async fn transient_metadata_status_retries_then_succeeds() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: 6\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let client = ReqwestConditionalRangeClient::new(
            &url,
            ImmutableOperationControl::unlimited(),
        )
        .expect("client");
        let mut retrying = AsyncRetryingReqwestConditionalClient::new(
            client,
            retry_policy(3),
            backoff_policy(1, 10, 20),
        );
        let metadata = retrying.metadata().await.expect("metadata");
        assert_eq!(metadata.length, 6);
        assert_eq!(metadata.version, "\"v1\"");
        assert_eq!(retrying.transport_attempts(), 2);
        assert_eq!(retrying.retries_planned(), 1);
        assert_eq!(retrying.cumulative_delay_millis(), 1);
        server.await.expect("server");
        assert_eq!(requests.lock().expect("requests").len(), 2);
    }

    #[tokio::test]
    async fn server_retry_after_is_exposed_to_the_backoff_planner() {
        let (url, _, server) = scripted_server(vec![
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let client = ReqwestConditionalRangeClient::new(
            &url,
            ImmutableOperationControl::unlimited(),
        )
        .expect("client");
        let attempt = client
            .metadata_attempt(ConditionalAuthenticationPolicy::Terminal)
            .await
            .expect("attempt");
        let (error, server_minimum_millis) = match attempt {
            ReqwestConditionalAttempt::Retry {
                error,
                server_minimum_millis,
            } => (error, server_minimum_millis),
            other => panic!("expected retry, got {other:?}"),
        };
        assert_eq!(server_minimum_millis, Some(1_000));
        let decision = ConditionalHttpDecision::Retry {
            error,
            server_minimum_millis,
        };
        let mut budget = ConditionalBackoffBudget::new(backoff_policy(1, 2_000, 5_000));
        let planned = plan_conditional_http_retry(&decision, &mut budget, None)
            .expect("retry-after plan");
        assert_eq!(planned.delay_millis, 1_000);
        assert!(planned.used_server_minimum);
        server.await.expect("server");
    }

    #[tokio::test]
    async fn attempt_budget_is_shared_across_metadata_and_ranges() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: 6\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let client = ReqwestConditionalRangeClient::new(
            &url,
            ImmutableOperationControl::unlimited(),
        )
        .expect("client");
        let mut retrying = AsyncRetryingReqwestConditionalClient::new(
            client,
            retry_policy(2),
            backoff_policy(1, 10, 20),
        );
        let metadata = retrying.metadata().await.expect("metadata");
        let version = StrongVersionToken::parse(metadata.version).expect("version");
        assert_eq!(
            retrying
                .read_range_if_match(&version, 0, 1, metadata.length)
                .await,
            Err(ConditionalSourceError::Limit("transport attempts"))
        );
        assert_eq!(retrying.transport_attempts(), 2);
        assert_eq!(retrying.retries_planned(), 0);
        server.await.expect("server");
        assert_eq!(requests.lock().expect("requests").len(), 2);
    }

    #[tokio::test]
    async fn cancellation_interrupts_async_backoff_wait() {
        let (url, _, server) = scripted_server(vec![
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let (control, cancellation) = ImmutableOperationControl::new(None);
        let client = ReqwestConditionalRangeClient::new(&url, control).expect("client");
        let mut retrying = AsyncRetryingReqwestConditionalClient::new(
            client,
            retry_policy(3),
            backoff_policy(500, 500, 1_500),
        );
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancellation.cancel();
        });
        let result = tokio::time::timeout(Duration::from_secs(1), retrying.metadata())
            .await
            .expect("bounded cancellation latency");
        assert_eq!(result, Err(ConditionalSourceError::Cancelled));
        assert_eq!(retrying.transport_attempts(), 1);
        assert_eq!(retrying.retries_planned(), 1);
        canceller.await.expect("canceller");
        server.await.expect("server");
    }

    #[tokio::test]
    async fn deadline_prevents_retry_wait_that_cannot_finish_in_time() {
        let (url, _, server) = scripted_server(vec![
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let (control, _) = ImmutableOperationControl::new(Some(
            Instant::now() + Duration::from_millis(100),
        ));
        let client = ReqwestConditionalRangeClient::new(&url, control).expect("client");
        let mut retrying = AsyncRetryingReqwestConditionalClient::new(
            client,
            retry_policy(3),
            backoff_policy(500, 500, 1_500),
        );
        assert_eq!(
            retrying.metadata().await,
            Err(ConditionalSourceError::DeadlineExceeded)
        );
        assert_eq!(retrying.transport_attempts(), 1);
        assert_eq!(retrying.retries_planned(), 0);
        server.await.expect("server");
    }
}
