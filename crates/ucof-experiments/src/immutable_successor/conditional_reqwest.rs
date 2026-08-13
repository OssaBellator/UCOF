use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, ETAG,
    IF_MATCH, RANGE, RETRY_AFTER,
};
use std::future::Future;
use std::time::Duration;

const ASYNC_CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Concrete asynchronous HTTP transport for the strong-version conditional-source experiments.
///
/// This adapter performs exactly one transport attempt per method call. It deliberately disables
/// Reqwest's own redirects, system proxies, automatic decompression, and request retries so those
/// behaviours cannot bypass UCOF's explicit response classification and operation-wide budgets.
/// Higher-level retry/authentication policy remains separate.
#[derive(Clone, Debug)]
pub struct ReqwestConditionalRangeClient {
    client: reqwest::Client,
    url: reqwest::Url,
    control: ImmutableOperationControl,
}

impl ReqwestConditionalRangeClient {
    pub fn new(
        url: &str,
        control: ImmutableOperationControl,
    ) -> Result<Self, ConditionalSourceError> {
        let url = reqwest::Url::parse(url)
            .map_err(|_| ConditionalSourceError::Client("invalid http URL"))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(ConditionalSourceError::Client("unsupported http URL"));
        }
        let host = url
            .host_str()
            .ok_or(ConditionalSourceError::Client("http URL host"))?
            .to_owned();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .no_gzip()
            .no_brotli()
            .no_zstd()
            .no_deflate()
            .referer(false)
            .retry(reqwest::retry::for_host(host).max_retries_per_request(0))
            .build()
            .map_err(|_| ConditionalSourceError::Client("reqwest client build"))?;
        Ok(Self {
            client,
            url,
            control,
        })
    }

    #[must_use]
    pub fn url(&self) -> &reqwest::Url {
        &self.url
    }

    /// Acquire exact object length and one strong HTTP entity tag using HEAD.
    pub async fn metadata(&self) -> Result<ConditionalObjectMetadata, ConditionalSourceError> {
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
            ConditionalAuthenticationPolicy::Terminal,
        ) {
            ConditionalHttpDecision::AcceptMetadata { length, version } => {
                Ok(ConditionalObjectMetadata {
                    length,
                    version: version.as_str().to_owned(),
                })
            }
            ConditionalHttpDecision::Retry { error, .. }
            | ConditionalHttpDecision::Fail(error) => Err(error),
            ConditionalHttpDecision::RefreshAuthentication => Err(ConditionalSourceError::Client(
                "authentication refresh unavailable",
            )),
            ConditionalHttpDecision::AcceptRange { .. } => Err(ConditionalSourceError::Protocol(
                "metadata classified as range",
            )),
        }
    }

    /// Read one exact range under `If-Match`, accepting bytes only after full response validation.
    pub async fn read_range_if_match(
        &self,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    ) -> Result<ConditionalRangeResponse, ConditionalSourceError> {
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
                ConditionalAuthenticationPolicy::Terminal,
            ) {
                ConditionalHttpDecision::Retry { error, .. }
                | ConditionalHttpDecision::Fail(error) => Err(error),
                ConditionalHttpDecision::RefreshAuthentication => {
                    Err(ConditionalSourceError::Client("authentication refresh unavailable"))
                }
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
            ConditionalAuthenticationPolicy::Terminal,
        ) {
            ConditionalHttpDecision::AcceptRange {
                version,
                offset,
                total_length,
                ..
            } => Ok(ConditionalRangeResponse {
                version: version.as_str().to_owned(),
                offset,
                total_length,
                body: body.to_vec(),
            }),
            ConditionalHttpDecision::Retry { error, .. }
            | ConditionalHttpDecision::Fail(error) => Err(error),
            ConditionalHttpDecision::RefreshAuthentication => Err(ConditionalSourceError::Client(
                "authentication refresh unavailable",
            )),
            ConditionalHttpDecision::AcceptMetadata { .. } => Err(ConditionalSourceError::Protocol(
                "range classified as metadata",
            )),
        }
    }
}

async fn wait_for_control_failure(control: &ImmutableOperationControl) -> ConditionalSourceError {
    loop {
        if let Err(error) = control.check() {
            return error;
        }
        tokio::time::sleep(ASYNC_CONTROL_POLL_INTERVAL).await;
    }
}

async fn await_reqwest_controlled<T, F>(
    control: &ImmutableOperationControl,
    future: F,
) -> Result<T, ConditionalSourceError>
where
    F: Future<Output = Result<T, reqwest::Error>>,
{
    control.check()?;
    tokio::pin!(future);
    tokio::select! {
        biased;
        error = wait_for_control_failure(control) => Err(error),
        result = &mut future => {
            control.check()?;
            result.map_err(map_reqwest_error)
        }
    }
}

fn map_reqwest_error(error: reqwest::Error) -> ConditionalSourceError {
    if error.is_timeout() {
        ConditionalSourceError::DeadlineExceeded
    } else if error.is_connect() {
        ConditionalSourceError::RetryableClient("http connect")
    } else if error.is_body() {
        ConditionalSourceError::RetryableClient("http response body")
    } else if error.is_request() {
        ConditionalSourceError::RetryableClient("http request")
    } else {
        ConditionalSourceError::Client("reqwest transport")
    }
}

fn response_head(
    status: u16,
    headers: &HeaderMap,
    body_length: usize,
) -> Result<ConditionalHttpResponseHead, ConditionalSourceError> {
    Ok(ConditionalHttpResponseHead {
        status,
        version: optional_header_string(headers, ETAG)?,
        content_length: optional_header_u64(headers, CONTENT_LENGTH)?,
        content_range: optional_content_range(headers)?,
        body_length,
        retry_after_millis: optional_retry_after_millis(headers)?,
    })
}

fn optional_header_string(
    headers: &HeaderMap,
    name: reqwest::header::HeaderName,
) -> Result<Option<String>, ConditionalSourceError> {
    headers
        .get(name)
        .map(|value| {
            value
                .to_str()
                .map(str::to_owned)
                .map_err(|_| ConditionalSourceError::Protocol("http header text"))
        })
        .transpose()
}

fn optional_header_u64(
    headers: &HeaderMap,
    name: reqwest::header::HeaderName,
) -> Result<Option<u64>, ConditionalSourceError> {
    optional_header_string(headers, name)?
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| ConditionalSourceError::Protocol("http integer header"))
        })
        .transpose()
}

fn optional_content_range(
    headers: &HeaderMap,
) -> Result<Option<ConditionalHttpContentRange>, ConditionalSourceError> {
    let Some(value) = optional_header_string(headers, CONTENT_RANGE)? else {
        return Ok(None);
    };
    let value = value
        .strip_prefix("bytes ")
        .ok_or(ConditionalSourceError::Protocol("content range unit"))?;
    let (range, total) = value
        .split_once('/')
        .ok_or(ConditionalSourceError::Protocol("content range shape"))?;
    let (start, end) = range
        .split_once('-')
        .ok_or(ConditionalSourceError::Protocol("content range bounds"))?;
    let start = start
        .parse::<u64>()
        .map_err(|_| ConditionalSourceError::Protocol("content range start"))?;
    let end_inclusive = end
        .parse::<u64>()
        .map_err(|_| ConditionalSourceError::Protocol("content range end"))?;
    let total_length = total
        .parse::<u64>()
        .map_err(|_| ConditionalSourceError::Protocol("content range total"))?;
    if start > end_inclusive || end_inclusive >= total_length {
        return Err(ConditionalSourceError::Protocol("content range bounds"));
    }
    Ok(Some(ConditionalHttpContentRange {
        start,
        end_inclusive,
        total_length,
    }))
}

fn optional_retry_after_millis(
    headers: &HeaderMap,
) -> Result<Option<u64>, ConditionalSourceError> {
    let Some(value) = optional_header_string(headers, RETRY_AFTER)? else {
        return Ok(None);
    };
    let seconds = value
        .parse::<u64>()
        .map_err(|_| ConditionalSourceError::Protocol("retry-after date unsupported"))?;
    seconds
        .checked_mul(1_000)
        .map(Some)
        .ok_or(ConditionalSourceError::Limit("retry-after"))
}

fn reject_non_identity_encoding(headers: &HeaderMap) -> Result<(), ConditionalSourceError> {
    if let Some(value) = optional_header_string(headers, CONTENT_ENCODING)? {
        if !value.eq_ignore_ascii_case("identity") {
            return Err(ConditionalSourceError::Protocol("content encoding"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod conditional_reqwest_tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
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

    #[tokio::test]
    async fn real_http_head_and_if_match_range_are_exact() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: 6\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 206 Partial Content\r\nETag: \"v1\"\r\nContent-Length: 3\r\nContent-Range: bytes 1-3/6\r\nConnection: close\r\n\r\nbcd",
        ])
        .await;
        let client = ReqwestConditionalRangeClient::new(
            &url,
            ImmutableOperationControl::unlimited(),
        )
        .expect("client");
        let metadata = client.metadata().await.expect("metadata");
        assert_eq!(metadata.length, 6);
        assert_eq!(metadata.version, "\"v1\"");
        let version = StrongVersionToken::parse(metadata.version.clone()).expect("version");
        let range = client
            .read_range_if_match(&version, 1, 3, metadata.length)
            .await
            .expect("range");
        assert_eq!(range.body, b"bcd");
        server.await.expect("server");

        let captured = requests.lock().expect("requests");
        assert_eq!(captured.len(), 2);
        let head = captured[0].to_ascii_lowercase();
        assert!(head.starts_with("head /object http/1.1\r\n"));
        assert!(head.contains("accept-encoding: identity\r\n"));
        let get = captured[1].to_ascii_lowercase();
        assert!(get.starts_with("get /object http/1.1\r\n"));
        assert!(get.contains("range: bytes=1-3\r\n"));
        assert!(get.contains("if-match: \"v1\"\r\n"));
        assert!(get.contains("accept-encoding: identity\r\n"));
    }

    #[tokio::test]
    async fn version_change_and_malformed_content_range_fail_closed() {
        let (url, _, server) = scripted_server(vec![
            b"HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 206 Partial Content\r\nETag: \"v1\"\r\nContent-Length: 3\r\nContent-Range: bytes 2-4/6\r\nConnection: close\r\n\r\nbcd",
        ])
        .await;
        let client = ReqwestConditionalRangeClient::new(
            &url,
            ImmutableOperationControl::unlimited(),
        )
        .expect("client");
        let version = StrongVersionToken::parse("\"v1\"").expect("version");
        assert_eq!(
            client.read_range_if_match(&version, 1, 3, 6).await,
            Err(ConditionalSourceError::VersionChanged)
        );
        assert_eq!(
            client.read_range_if_match(&version, 1, 3, 6).await,
            Err(ConditionalSourceError::Protocol("content range"))
        );
        server.await.expect("server");
    }

    #[tokio::test]
    async fn redirect_is_not_followed() {
        let (url, requests, server) = scripted_server(vec![
            b"HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:9/elsewhere\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ])
        .await;
        let client = ReqwestConditionalRangeClient::new(
            &url,
            ImmutableOperationControl::unlimited(),
        )
        .expect("client");
        assert_eq!(
            client.metadata().await,
            Err(ConditionalSourceError::Protocol("redirect"))
        );
        server.await.expect("server");
        assert_eq!(requests.lock().expect("requests").len(), 1);
    }

    #[tokio::test]
    async fn cancellation_drops_a_stalled_response_body_future() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let (body_started_tx, body_started_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let _ = read_request(&mut socket).await;
            socket
                .write_all(
                    b"HTTP/1.1 206 Partial Content\r\nETag: \"v1\"\r\nContent-Length: 4\r\nContent-Range: bytes 0-3/4\r\nConnection: close\r\n\r\na",
                )
                .await
                .expect("headers and partial body");
            body_started_tx.send(()).expect("signal body");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let (control, cancellation) = ImmutableOperationControl::new(None);
        let client = ReqwestConditionalRangeClient::new(
            &format!("http://{address}/object"),
            control,
        )
        .expect("client");
        let version = StrongVersionToken::parse("\"v1\"").expect("version");
        let read = tokio::spawn(async move {
            client.read_range_if_match(&version, 0, 4, 4).await
        });
        body_started_rx.await.expect("body started");
        cancellation.cancel();
        let result = tokio::time::timeout(Duration::from_secs(1), read)
            .await
            .expect("cancellation latency")
            .expect("read task");
        assert_eq!(result, Err(ConditionalSourceError::Cancelled));
        server.abort();
    }

    #[tokio::test]
    async fn deadline_aborts_stalled_response_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let _ = read_request(&mut socket).await;
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let (control, _) = ImmutableOperationControl::new(Some(
            Instant::now() + Duration::from_millis(50),
        ));
        let client = ReqwestConditionalRangeClient::new(
            &format!("http://{address}/object"),
            control,
        )
        .expect("client");
        assert_eq!(
            client.metadata().await,
            Err(ConditionalSourceError::DeadlineExceeded)
        );
        server.abort();
    }
}
