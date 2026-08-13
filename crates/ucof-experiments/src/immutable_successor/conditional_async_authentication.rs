use std::future::Future;
use std::pin::Pin;

/// Redacted HTTP `Authorization` value supplied by application-owned credential policy.
///
/// The value is marked sensitive before use so debug-formatting layers that honor the HTTP
/// sensitivity flag do not expose credential bytes. UCOF does not interpret the authentication
/// scheme and never derives credentials from a 401 response.
#[derive(Clone, PartialEq, Eq)]
pub struct ReqwestAuthorizationHeader(HeaderValue);

impl std::fmt::Debug for ReqwestAuthorizationHeader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ReqwestAuthorizationHeader(<redacted>)")
    }
}

impl ReqwestAuthorizationHeader {
    pub fn parse(value: &str) -> Result<Self, ConditionalSourceError> {
        let mut value = HeaderValue::from_str(value)
            .map_err(|_| ConditionalSourceError::Client("authorization header"))?;
        value.set_sensitive(true);
        Ok(Self(value))
    }

    pub fn from_header_value(mut value: HeaderValue) -> Self {
        value.set_sensitive(true);
        Self(value)
    }

    fn value(&self) -> HeaderValue {
        self.0.clone()
    }
}

/// Application-owned asynchronous authentication state and refresh operation.
///
/// `current_authorization` is called immediately before each HTTP transport attempt. The refresher
/// may replace its own credential/session state, but UCOF grants at most one refresh per logical
/// metadata/range request. The returned authorization value is treated as opaque credential data.
pub trait AsyncConditionalAuthenticationRefresher {
    fn current_authorization(
        &self,
    ) -> Result<Option<ReqwestAuthorizationHeader>, ConditionalSourceError>;

    fn refresh_authentication<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConditionalSourceError>> + Send + 'a>>;
}

impl ReqwestConditionalRangeClient {
    async fn metadata_attempt_with_authorization(
        &self,
        authorization: Option<&ReqwestAuthorizationHeader>,
        authentication: ConditionalAuthenticationPolicy,
    ) -> Result<ReqwestConditionalAttempt<ConditionalObjectMetadata>, ConditionalSourceError> {
        self.control.check()?;
        let mut request = self
            .client
            .head(self.url.clone())
            .header(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
        if let Some(authorization) = authorization {
            request = request.header(reqwest::header::AUTHORIZATION, authorization.value());
        }
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

    async fn read_range_attempt_with_authorization(
        &self,
        authorization: Option<&ReqwestAuthorizationHeader>,
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
        let mut request = self
            .client
            .get(self.url.clone())
            .header(ACCEPT_ENCODING, HeaderValue::from_static("identity"))
            .header(IF_MATCH, if_match)
            .header(RANGE, range);
        if let Some(authorization) = authorization {
            request = request.header(reqwest::header::AUTHORIZATION, authorization.value());
        }
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

impl AsyncRetryingReqwestConditionalClient {
    async fn metadata_with_authentication<R>(
        &mut self,
        refresher: &R,
        authentication: ConditionalAuthenticationPolicy,
    ) -> Result<ReqwestConditionalAttempt<ConditionalObjectMetadata>, ConditionalSourceError>
    where
        R: AsyncConditionalAuthenticationRefresher,
    {
        loop {
            self.begin_attempt()?;
            let authorization = refresher.current_authorization()?;
            match self
                .client
                .metadata_attempt_with_authorization(authorization.as_ref(), authentication)
                .await
            {
                Ok(ReqwestConditionalAttempt::Accepted(metadata)) => {
                    return Ok(ReqwestConditionalAttempt::Accepted(metadata));
                }
                Ok(ReqwestConditionalAttempt::RefreshAuthentication) => {
                    return Ok(ReqwestConditionalAttempt::RefreshAuthentication);
                }
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
                Err(error) => self.retry_after_transport_error(error).await?,
            }
        }
    }

    async fn read_range_with_authentication<R>(
        &mut self,
        refresher: &R,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
        authentication: ConditionalAuthenticationPolicy,
    ) -> Result<ReqwestConditionalAttempt<ConditionalRangeResponse>, ConditionalSourceError>
    where
        R: AsyncConditionalAuthenticationRefresher,
    {
        loop {
            self.begin_attempt()?;
            let authorization = refresher.current_authorization()?;
            match self
                .client
                .read_range_attempt_with_authorization(
                    authorization.as_ref(),
                    expected,
                    offset,
                    length,
                    total_length,
                    authentication,
                )
                .await
            {
                Ok(ReqwestConditionalAttempt::Accepted(response)) => {
                    return Ok(ReqwestConditionalAttempt::Accepted(response));
                }
                Ok(ReqwestConditionalAttempt::RefreshAuthentication) => {
                    return Ok(ReqwestConditionalAttempt::RefreshAuthentication);
                }
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
                Err(error) => self.retry_after_transport_error(error).await?,
            }
        }
    }
}

/// Async concrete conditional HTTP client with application-owned credential refresh.
///
/// Every metadata or range operation permits at most one explicit refresh. A refresh is never
/// inferred from credentials or transport errors: it occurs only after the generic HTTP classifier
/// returns `RefreshAuthentication` for a 401 under `OneRefreshPermitted`. The replay is classified
/// with terminal authentication policy and consumes the same operation-wide transport-attempt
/// budget owned by the underlying retrying client.
pub struct AsyncAuthenticatedReqwestConditionalClient<R> {
    retrying: AsyncRetryingReqwestConditionalClient,
    refresher: R,
    refresh_attempts: u64,
}

impl<R> AsyncAuthenticatedReqwestConditionalClient<R>
where
    R: AsyncConditionalAuthenticationRefresher,
{
    #[must_use]
    pub fn new(retrying: AsyncRetryingReqwestConditionalClient, refresher: R) -> Self {
        Self {
            retrying,
            refresher,
            refresh_attempts: 0,
        }
    }

    #[must_use]
    pub fn transport_attempts(&self) -> u64 {
        self.retrying.transport_attempts()
    }

    #[must_use]
    pub fn refresh_attempts(&self) -> u64 {
        self.refresh_attempts
    }

    #[must_use]
    pub fn retries_planned(&self) -> u32 {
        self.retrying.retries_planned()
    }

    #[must_use]
    pub fn cumulative_delay_millis(&self) -> u64 {
        self.retrying.cumulative_delay_millis()
    }

    pub fn into_parts(self) -> (AsyncRetryingReqwestConditionalClient, R) {
        (self.retrying, self.refresher)
    }

    async fn refresh_once(&mut self) -> Result<(), ConditionalSourceError> {
        self.refresh_attempts = self
            .refresh_attempts
            .checked_add(1)
            .ok_or(ConditionalSourceError::Limit("authentication refreshes"))?;
        let control = self.retrying.control.clone();
        control.check()?;
        let refresh = self.refresher.refresh_authentication();
        tokio::pin!(refresh);
        tokio::select! {
            biased;
            error = wait_for_control_failure(&control) => Err(error),
            result = &mut refresh => {
                control.check()?;
                result
            }
        }
    }

    pub async fn metadata(&mut self) -> Result<ConditionalObjectMetadata, ConditionalSourceError> {
        match self
            .retrying
            .metadata_with_authentication(
                &self.refresher,
                ConditionalAuthenticationPolicy::OneRefreshPermitted,
            )
            .await?
        {
            ReqwestConditionalAttempt::Accepted(metadata) => Ok(metadata),
            ReqwestConditionalAttempt::RefreshAuthentication => {
                self.refresh_once().await?;
                match self
                    .retrying
                    .metadata_with_authentication(
                        &self.refresher,
                        ConditionalAuthenticationPolicy::Terminal,
                    )
                    .await?
                {
                    ReqwestConditionalAttempt::Accepted(metadata) => Ok(metadata),
                    ReqwestConditionalAttempt::RefreshAuthentication => Err(
                        ConditionalSourceError::Client("authentication refresh exhausted"),
                    ),
                    ReqwestConditionalAttempt::Retry { .. } => Err(
                        ConditionalSourceError::Client("retry escaped operation wrapper"),
                    ),
                }
            }
            ReqwestConditionalAttempt::Retry { .. } => Err(ConditionalSourceError::Client(
                "retry escaped operation wrapper",
            )),
        }
    }

    pub async fn read_range_if_match(
        &mut self,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    ) -> Result<ConditionalRangeResponse, ConditionalSourceError> {
        match self
            .retrying
            .read_range_with_authentication(
                &self.refresher,
                expected,
                offset,
                length,
                total_length,
                ConditionalAuthenticationPolicy::OneRefreshPermitted,
            )
            .await?
        {
            ReqwestConditionalAttempt::Accepted(response) => Ok(response),
            ReqwestConditionalAttempt::RefreshAuthentication => {
                self.refresh_once().await?;
                match self
                    .retrying
                    .read_range_with_authentication(
                        &self.refresher,
                        expected,
                        offset,
                        length,
                        total_length,
                        ConditionalAuthenticationPolicy::Terminal,
                    )
                    .await?
                {
                    ReqwestConditionalAttempt::Accepted(response) => Ok(response),
                    ReqwestConditionalAttempt::RefreshAuthentication => Err(
                        ConditionalSourceError::Client("authentication refresh exhausted"),
                    ),
                    ReqwestConditionalAttempt::Retry { .. } => Err(
                        ConditionalSourceError::Client("retry escaped operation wrapper"),
                    ),
                }
            }
            ReqwestConditionalAttempt::Retry { .. } => Err(ConditionalSourceError::Client(
                "retry escaped operation wrapper",
            )),
        }
    }
}

#[cfg(test)]
mod conditional_reqwest_async_authentication_tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

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

    struct RotatingBearerRefresher {
        current: String,
        refreshed: String,
        calls: usize,
        error: Option<ConditionalSourceError>,
        refresh_started: Option<oneshot::Sender<()>>,
        stall_refresh: bool,
    }

    impl RotatingBearerRefresher {
        fn new(current: &str, refreshed: &str) -> Self {
            Self {
                current: current.to_owned(),
                refreshed: refreshed.to_owned(),
                calls: 0,
                error: None,
                refresh_started: None,
                stall_refresh: false,
            }
        }
    }

    impl AsyncConditionalAuthenticationRefresher for RotatingBearerRefresher {
        fn current_authorization(
            &self,
        ) -> Result<Option<ReqwestAuthorizationHeader>, ConditionalSourceError> {
            ReqwestAuthorizationHeader::parse(&format!("Bearer {}", self.current)).map(Some)
        }

        fn refresh_authentication<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), ConditionalSourceError>> + Send + 'a>> {
            Box::pin(async move {
                self.calls += 1;
                if let Some(sender) = self.refresh_started.take() {
                    let _ = sender.send(());
                }
                if self.stall_refresh {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                if let Some(error) = &self.error {
                    return Err(error.clone());
                }
                self.current = self.refreshed.clone();
                Ok(())
            })
        }
    }

    fn retry_policy(attempts: u64) -> ConditionalRetryPolicy {
        ConditionalRetryPolicy::new(attempts).expect("retry policy")
    }

    fn backoff_policy() -> ConditionalBackoffPolicy {
        ConditionalBackoffPolicy::new(1, 10, 20).expect("backoff policy")
    }

    fn authenticated_client(
        url: &str,
        control: ImmutableOperationControl,
        attempts: u64,
        refresher: RotatingBearerRefresher,
    ) -> AsyncAuthenticatedReqwestConditionalClient<RotatingBearerRefresher> {
        let transport = ReqwestConditionalRangeClient::new(url, control).expect("transport");
        let retrying = AsyncRetryingReqwestConditionalClient::new(
            transport,
            retry_policy(attempts),
            backoff_policy(),
        );
        AsyncAuthenticatedReqwestConditionalClient::new(retrying, refresher)
    }

    #[tokio::test]
    async fn one_refresh_replays_same_metadata_request_with_new_authorization() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: 6\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let mut client = authenticated_client(
            &url,
            ImmutableOperationControl::unlimited(),
            4,
            RotatingBearerRefresher::new("stale", "fresh"),
        );
        let metadata = client.metadata().await.expect("metadata after refresh");
        assert_eq!(metadata.length, 6);
        assert_eq!(client.transport_attempts(), 2);
        assert_eq!(client.refresh_attempts(), 1);
        server.await.expect("server");

        let captured = requests.lock().expect("requests");
        assert_eq!(captured.len(), 2);
        assert!(captured[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer stale\r\n"));
        assert!(captured[1]
            .to_ascii_lowercase()
            .contains("authorization: bearer fresh\r\n"));
    }

    #[tokio::test]
    async fn second_unauthorized_response_is_terminal_without_second_refresh() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let mut client = authenticated_client(
            &url,
            ImmutableOperationControl::unlimited(),
            4,
            RotatingBearerRefresher::new("stale", "fresh"),
        );
        assert_eq!(
            client.metadata().await,
            Err(ConditionalSourceError::Client("http unauthorized"))
        );
        assert_eq!(client.transport_attempts(), 2);
        assert_eq!(client.refresh_attempts(), 1);
        server.await.expect("server");
        assert_eq!(requests.lock().expect("requests").len(), 2);
    }

    #[tokio::test]
    async fn refresh_failure_prevents_replay() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let mut refresher = RotatingBearerRefresher::new("stale", "fresh");
        refresher.error = Some(ConditionalSourceError::Client("refresh failed"));
        let mut client = authenticated_client(
            &url,
            ImmutableOperationControl::unlimited(),
            4,
            refresher,
        );
        assert_eq!(
            client.metadata().await,
            Err(ConditionalSourceError::Client("refresh failed"))
        );
        assert_eq!(client.transport_attempts(), 1);
        assert_eq!(client.refresh_attempts(), 1);
        server.await.expect("server");
        assert_eq!(requests.lock().expect("requests").len(), 1);
    }

    #[tokio::test]
    async fn replay_consumes_the_same_transport_attempt_budget() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let mut client = authenticated_client(
            &url,
            ImmutableOperationControl::unlimited(),
            1,
            RotatingBearerRefresher::new("stale", "fresh"),
        );
        assert_eq!(
            client.metadata().await,
            Err(ConditionalSourceError::Limit("transport attempts"))
        );
        assert_eq!(client.transport_attempts(), 1);
        assert_eq!(client.refresh_attempts(), 1);
        server.await.expect("server");
        assert_eq!(requests.lock().expect("requests").len(), 1);
    }

    #[tokio::test]
    async fn cancellation_during_async_refresh_prevents_replay() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let (control, cancellation) = ImmutableOperationControl::new(None);
        let (refresh_started_tx, refresh_started_rx) = oneshot::channel();
        let mut refresher = RotatingBearerRefresher::new("stale", "fresh");
        refresher.refresh_started = Some(refresh_started_tx);
        refresher.stall_refresh = true;
        let mut client = authenticated_client(&url, control, 4, refresher);
        let operation = tokio::spawn(async move { client.metadata().await });
        refresh_started_rx.await.expect("refresh started");
        cancellation.cancel();
        let result = tokio::time::timeout(Duration::from_secs(1), operation)
            .await
            .expect("bounded cancellation latency")
            .expect("operation task");
        assert_eq!(result, Err(ConditionalSourceError::Cancelled));
        server.await.expect("server");
        assert_eq!(requests.lock().expect("requests").len(), 1);
    }

    #[tokio::test]
    async fn refreshed_credentials_apply_to_later_conditional_range() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: 6\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 206 Partial Content\r\nETag: \"v1\"\r\nContent-Length: 3\r\nContent-Range: bytes 1-3/6\r\nConnection: close\r\n\r\nbcd",
        ])
        .await;
        let mut client = authenticated_client(
            &url,
            ImmutableOperationControl::unlimited(),
            5,
            RotatingBearerRefresher::new("stale", "fresh"),
        );
        let metadata = client.metadata().await.expect("metadata");
        let version = StrongVersionToken::parse(metadata.version).expect("version");
        let response = client
            .read_range_if_match(&version, 1, 3, metadata.length)
            .await
            .expect("range");
        assert_eq!(response.body, b"bcd");
        assert_eq!(client.transport_attempts(), 3);
        assert_eq!(client.refresh_attempts(), 1);
        server.await.expect("server");

        let captured = requests.lock().expect("requests");
        assert_eq!(captured.len(), 3);
        assert!(captured[2]
            .to_ascii_lowercase()
            .contains("authorization: bearer fresh\r\n"));
        assert!(captured[2]
            .to_ascii_lowercase()
            .contains("if-match: \"v1\"\r\n"));
    }
}
